use std::path::Path;
use std::sync::{Arc, Mutex};

use crate::ast_parser::AstParser;
use crate::budget_tracker::{BudgetTracker, UsageReport};
use crate::cache_manager::CacheManager;
use crate::confidence_router::ConfidenceRouter;
use crate::cost_calculator::{CostCalculator, SessionCostSummary};
use crate::ctx_format::CtxFormat;
use crate::error::{Result, SqzError};
use crate::model_router::ModelRouter;
use crate::pin_manager::PinManager;
use crate::pipeline::CompressionPipeline;
use crate::plugin_api::PluginLoader;
use crate::preset::{Preset, PresetParser};
use crate::session_store::{SessionStore, SessionSummary};
use crate::terse_mode::TerseMode;
use crate::types::{CompressedContent, PinEntry, Provenance, SessionId};
use crate::verifier::Verifier;

/// Top-level facade that wires all sqz_engine modules together.
///
/// # Concurrency design
///
/// `SqzEngine` is designed for single-threaded use on the main thread.
/// The only cross-thread sharing happens during preset hot-reload: the
/// file-watcher callback runs on a background thread and needs to update
/// the preset, pipeline, and model router. These three fields are wrapped
/// in `Arc<Mutex<>>` specifically for that purpose. All other fields are
/// owned directly — no unnecessary synchronization.
pub struct SqzEngine {
    // --- Hot-reloadable state (shared with file-watcher thread) ---
    preset: Arc<Mutex<Preset>>,
    pipeline: Arc<Mutex<CompressionPipeline>>,
    model_router: Arc<Mutex<ModelRouter>>,

    // --- Single-owner state (no cross-thread sharing needed) ---
    session_store: SessionStore,
    cache_manager: CacheManager,
    budget_tracker: BudgetTracker,
    cost_calculator: CostCalculator,
    ast_parser: AstParser,
    terse_mode: TerseMode,
    pin_manager: PinManager,
    confidence_router: ConfidenceRouter,
    _plugin_loader: PluginLoader,
}

impl SqzEngine {
    /// Create a new engine with the default preset and a persistent session store.
    ///
    /// Sessions are stored in `~/.sqz/sessions.db` for cross-session continuity.
    /// Falls back to a temp-file store if the home directory is unavailable
    /// OR the database there can't actually be opened (see
    /// [`Self::default_store_path`]).
    pub fn new() -> Result<Self> {
        let preset = Preset::default();
        let store_path = Self::default_store_path();
        Self::with_preset_and_store(preset, &store_path)
    }

    /// Resolve the default session store path: `~/.sqz/sessions.db`.
    ///
    /// Falls back to a temp-file path in two cases:
    ///   1. The home directory (or `~/.sqz` within it) can't be created —
    ///      the original fallback.
    ///   2. `~/.sqz` exists and is writable, but the database file itself
    ///      can't be opened — e.g. its permissions were changed out from
    ///      under sqz, the volume is read-only, or disk space ran out.
    ///      Previously only case 1 was handled: an inaccessible-but-
    ///      present `sessions.db` made every session operation hard-fail
    ///      with "unable to open database file" instead of degrading to
    ///      a working (if non-persistent) session, because
    ///      `SessionStore::open_or_create`'s corruption recovery just
    ///      deletes and retries the *same* path — which doesn't help
    ///      when the problem is permissions, not corruption.
    ///
    /// The probe below actually attempts to open the store before
    /// committing to that path, so case 2 is caught here rather than
    /// surfacing as a fatal error deeper in `with_preset_and_store`.
    fn default_store_path() -> std::path::PathBuf {
        if let Some(home) = dirs_next::home_dir() {
            let sqz_dir = home.join(".sqz");
            if std::fs::create_dir_all(&sqz_dir).is_ok() {
                // Harden permissions on Unix: ~/.sqz/ contains session data
                // and cached content that may include sensitive output.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ =
                        std::fs::set_permissions(&sqz_dir, std::fs::Permissions::from_mode(0o700));
                }
                let candidate = sqz_dir.join("sessions.db");
                return Self::probe_or_fallback(candidate);
            }
        }
        Self::temp_store_path()
    }

    /// Try to actually open `candidate` as a session store; if that fails,
    /// fall back to a fresh temp-file path instead of propagating the
    /// error. Split out from [`Self::default_store_path`] so the
    /// probe-and-fallback behavior is unit-testable without touching the
    /// real `$HOME`.
    fn probe_or_fallback(candidate: std::path::PathBuf) -> std::path::PathBuf {
        match SessionStore::open_or_create(&candidate) {
            Ok(_) => candidate,
            Err(e) => {
                eprintln!(
                    "sqz warning: cannot use session store at '{}' ({e}). \
                     Falling back to a temporary database for this \
                     session — history will not persist across runs.",
                    candidate.display()
                );
                Self::temp_store_path()
            }
        }
    }

    /// A unique temp-file path for the session store, used when
    /// `~/.sqz/sessions.db` is unavailable (missing home dir, unwritable
    /// `~/.sqz`, or an inaccessible existing database).
    fn temp_store_path() -> std::path::PathBuf {
        let dir = std::env::temp_dir();
        dir.join(format!(
            "sqz_session_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ))
    }

    /// Create with a custom preset and a file-backed session store.
    ///
    /// Opens a single SQLite connection for the session store. The cache
    /// manager and pin manager share the same store via separate connections
    /// (SQLite WAL mode supports concurrent readers).
    pub fn with_preset_and_store(preset: Preset, store_path: &Path) -> Result<Self> {
        let pipeline = CompressionPipeline::new(&preset);
        let window_size = preset.budget.default_window_size;

        // One connection per consumer. SQLite WAL mode handles concurrency.
        let session_store = SessionStore::open_or_create(store_path)?;
        let cache_store = SessionStore::open_or_create(store_path)?;
        let pin_store = SessionStore::open_or_create(store_path)?;

        Ok(SqzEngine {
            preset: Arc::new(Mutex::new(preset.clone())),
            pipeline: Arc::new(Mutex::new(pipeline)),
            model_router: Arc::new(Mutex::new(ModelRouter::new(&preset))),
            session_store,
            cache_manager: CacheManager::new(cache_store, 512 * 1024 * 1024),
            budget_tracker: BudgetTracker::new(window_size, &preset),
            cost_calculator: CostCalculator::with_defaults(),
            ast_parser: AstParser::new(),
            terse_mode: TerseMode,
            pin_manager: PinManager::new(pin_store),
            confidence_router: ConfidenceRouter::new(),
            _plugin_loader: PluginLoader::new(Path::new("plugins")),
        })
    }

    /// Compress input text using the current preset.
    ///
    /// Two-pass pipeline:
    /// 1. Route to compression mode based on content entropy and risk patterns.
    /// 2. Compress using the pipeline (safe preset for Safe mode, default otherwise).
    /// 3. Verify invariants (error lines, JSON keys, diff hunks, etc.).
    /// 4. If verification confidence is low, fall back to safe mode and re-compress.
    pub fn compress(&self, input: &str) -> Result<CompressedContent> {
        let preset = self
            .preset
            .lock()
            .map_err(|_| SqzError::Other("preset lock poisoned".into()))?;
        let pipeline = self
            .pipeline
            .lock()
            .map_err(|_| SqzError::Other("pipeline lock poisoned".into()))?;
        let ctx = crate::pipeline::SessionContext {
            session_id: "engine".to_string(),
        };

        // Step 1: Route — check content risk before compressing
        let mode = self.confidence_router.route(input);

        // Step 1b: The router's auto-classification can pick Aggressive on
        // its own (low-entropy/repetitive content), but auto-selection of
        // the lossy subsystem is only allowed with explicit opt-in
        // (SQZ_ALLOW_LOSSY=1) — otherwise it downgrades to Default. This
        // is what makes bare `sqz compress` / `--mode auto` safe by
        // construction: the *only* way to reach the lossy subsystem
        // without opting in is an explicit `--mode aggressive` request,
        // which goes through `compress_with_mode` and never touches this
        // gate.
        let mode = crate::confidence_router::gate_auto_mode(
            mode,
            crate::confidence_router::lossy_allowed(),
        );

        // Step 2: If Safe mode, skip aggressive pipeline and go straight to safe compress
        if mode == crate::confidence_router::CompressionMode::Safe {
            eprintln!("[sqz] fallback: safe mode — content classified as high-risk (stack trace / migration / secret)");
            return self.compress_safe(input, &pipeline, &ctx);
        }

        // Step 3: Compress with the configured pipeline. `mode` here is
        // whatever the confidence router picked (Default, or Aggressive
        // only if SQZ_ALLOW_LOSSY=1 — Safe already returned above), so the
        // lossy subsystem only runs when the router explicitly classified
        // this content as safe to compress aggressively AND the caller has
        // opted in to automatic lossy compression.
        let mut result = pipeline.compress(input, &ctx, &preset, mode)?;

        // Step 4: Verify invariants
        let verify = Verifier::verify(input, &result.data);
        let fallback = verify.fallback_triggered;
        result.verify = Some(verify);

        // Step 5: If verifier signals low confidence, re-compress with safe settings
        if fallback && result.data != input {
            eprintln!("[sqz] fallback: verifier confidence {:.2} below threshold — re-compressing in safe mode",
                result.verify.as_ref().map(|v| v.confidence).unwrap_or(0.0));
            let safe_result = self.compress_safe(input, &pipeline, &ctx)?;
            return Ok(safe_result);
        }

        Ok(result)
    }

    /// Compress with dedup cache lookup.
    ///
    /// Unlike [`compress`], this method consults the persistent cache
    /// (`~/.sqz/sessions.db`). On a hit with a fresh ref, it returns a
    /// 13-token `§ref:HASH§` token instead of re-compressing. On a
    /// near-duplicate, it returns a compact delta. On a cache miss, it
    /// runs the full pipeline, stores the result, and returns it.
    ///
    /// This is the path the MCP `sqz_read_file` / `sqz_grep` /
    /// `sqz_list_dir` tools use — repeat reads in the same session
    /// (and across sessions if the DB survives) collapse to 13 tokens.
    /// Reported on issue #12: the MCP file tools bypassed the cache so
    /// dedup never fired, and users were getting 30%-range pipeline
    /// compression instead of the 92% dedup path advertised in the
    /// README.
    pub fn compress_with_cache(&self, input: &str) -> Result<crate::cache_manager::CacheResult> {
        let pipeline = self
            .pipeline
            .lock()
            .map_err(|_| SqzError::Other("pipeline lock poisoned".into()))?;
        // The `path` argument is informational only — cache keys are
        // SHA-256 of the content bytes, not the path. Pass an empty
        // path so we don't partition the cache by accidental file
        // location differences (e.g. `./src/main.rs` vs
        // `/abs/path/to/src/main.rs`).
        self.cache_manager
            .get_or_compress(std::path::Path::new(""), input.as_bytes(), &pipeline)
    }

    /// Defensive compression: any input in, `CompressedContent` out, guaranteed.
    ///
    /// Unlike `compress()` which returns `Result`, this method never returns
    /// an error. On any internal failure it returns the original input
    /// unchanged with a 1.0 compression ratio. This makes it safe to call
    /// from contexts where error handling is impractical (e.g. shell hooks,
    /// browser extension bridges).
    pub fn compress_or_passthrough(&self, input: &str) -> CompressedContent {
        match self.compress(input) {
            Ok(result) => result,
            Err(_) => {
                let tokens = (input.len() as u32).div_ceil(4);
                CompressedContent {
                    data: input.to_string(),
                    tokens_compressed: tokens,
                    tokens_original: tokens,
                    stages_applied: vec![],
                    compression_ratio: 1.0,
                    provenance: crate::types::Provenance::default(),
                    verify: None,
                }
            }
        }
    }

    /// Compress with explicit mode override, bypassing the confidence router.
    ///
    /// - `CompressionMode::Safe` → safe pipeline only (ANSI strip + condense),
    ///   the lossy subsystem never runs.
    /// - `CompressionMode::Default` → standard pipeline, lossy subsystem
    ///   (RLE / sliding-window dedup / entropy truncation / token pruning)
    ///   does not run.
    /// - `CompressionMode::Aggressive` → standard pipeline with the lossy
    ///   subsystem enabled.
    ///
    /// The requested `mode` is honored end-to-end — this method does *not*
    /// re-route through [`ConfidenceRouter`] like [`compress`](Self::compress)
    /// does, so a caller that explicitly asks for `Aggressive` (e.g.
    /// `cli_proxy`'s session-pressure escalation) actually gets it, rather
    /// than being silently downgraded back to whatever the router would
    /// have picked. A low-confidence verifier result still falls back to
    /// safe mode regardless of the requested mode — that safety net stays
    /// on for every mode.
    pub fn compress_with_mode(
        &self,
        input: &str,
        mode: crate::confidence_router::CompressionMode,
    ) -> Result<CompressedContent> {
        let preset = self
            .preset
            .lock()
            .map_err(|_| SqzError::Other("preset lock poisoned".into()))?;
        let pipeline = self
            .pipeline
            .lock()
            .map_err(|_| SqzError::Other("pipeline lock poisoned".into()))?;
        let ctx = crate::pipeline::SessionContext {
            session_id: "engine".to_string(),
        };

        if mode == crate::confidence_router::CompressionMode::Safe {
            return self.compress_safe(input, &pipeline, &ctx);
        }

        // Default and Aggressive: compress with the caller's explicit mode,
        // then verify invariants and fall back to safe mode if confidence
        // is low — same safety net as `compress()`, minus the initial
        // confidence-router pass (the caller already decided the mode).
        let mut result = pipeline.compress(input, &ctx, &preset, mode)?;
        let verify = Verifier::verify(input, &result.data);
        let fallback = verify.fallback_triggered;
        result.verify = Some(verify);

        if fallback && result.data != input {
            eprintln!("[sqz] fallback: verifier confidence {:.2} below threshold — re-compressing in safe mode",
                result.verify.as_ref().map(|v| v.confidence).unwrap_or(0.0));
            return self.compress_safe(input, &pipeline, &ctx);
        }

        Ok(result)
    }

    /// Safe-mode compression: minimal transforms only (ANSI strip + condense).
    fn compress_safe(
        &self,
        input: &str,
        pipeline: &crate::pipeline::CompressionPipeline,
        ctx: &crate::pipeline::SessionContext,
    ) -> Result<CompressedContent> {
        use crate::preset::{
            BudgetConfig, CompressionConfig, CondenseConfig, CustomTransformsConfig, ModelConfig,
            PresetMeta, TerseLevel, TerseModeConfig, ToolSelectionConfig,
        };

        let safe_preset = Preset {
            preset: PresetMeta {
                name: "safe".to_string(),
                version: "1.0".to_string(),
                description: "Safe fallback — minimal compression".to_string(),
            },
            compression: CompressionConfig {
                stages: vec!["condense".to_string()],
                keep_fields: None,
                strip_fields: None,
                condense: Some(CondenseConfig {
                    enabled: true,
                    max_repeated_lines: 3,
                }),
                git_diff_fold: None,
                strip_nulls: None,
                flatten: None,
                truncate_strings: None,
                collapse_arrays: None,
                custom_transforms: Some(CustomTransformsConfig { enabled: false }),
            },
            tool_selection: ToolSelectionConfig {
                max_tools: 5,
                similarity_threshold: 0.7,
                default_tools: vec![],
            },
            budget: BudgetConfig {
                warning_threshold: 0.70,
                ceiling_threshold: 0.85,
                default_window_size: 200_000,
                agents: Default::default(),
            },
            terse_mode: TerseModeConfig {
                enabled: false,
                level: TerseLevel::Moderate,
            },
            model: ModelConfig {
                family: "anthropic".to_string(),
                primary: String::new(),
                local: String::new(),
                complexity_threshold: 0.4,
                pricing: None,
            },
        };

        let mut result = pipeline.compress(
            input,
            ctx,
            &safe_preset,
            crate::confidence_router::CompressionMode::Safe,
        )?;
        let verify = Verifier::verify(input, &result.data);
        result.verify = Some(verify);
        result.provenance = Provenance {
            label: Some("safe-fallback".to_string()),
            ..Default::default()
        };
        Ok(result)
    }

    /// Compress with explicit provenance metadata attached to the result.
    pub fn compress_with_provenance(
        &self,
        input: &str,
        provenance: Provenance,
    ) -> Result<CompressedContent> {
        let mut result = self.compress(input)?;
        result.provenance = provenance;
        Ok(result)
    }

    /// Export a session to CTX format.
    pub fn export_ctx(&self, session_id: &str) -> Result<String> {
        let session = self.session_store.load_session(session_id.to_string())?;
        CtxFormat::serialize(&session)
    }

    /// Import a CTX string and save as a new session.
    pub fn import_ctx(&self, ctx: &str) -> Result<SessionId> {
        let session = CtxFormat::deserialize(ctx)?;
        self.session_store.save_session(&session)
    }

    /// Pin a conversation turn.
    pub fn pin(
        &self,
        session_id: &str,
        turn_index: usize,
        reason: &str,
        tokens: u32,
    ) -> Result<PinEntry> {
        self.pin_manager.pin(session_id, turn_index, reason, tokens)
    }

    /// Unpin a conversation turn.
    pub fn unpin(&self, session_id: &str, turn_index: usize) -> Result<()> {
        self.pin_manager.unpin(session_id, turn_index)
    }

    /// Search sessions by keyword.
    pub fn search_sessions(&self, query: &str) -> Result<Vec<SessionSummary>> {
        self.session_store.search(query)
    }

    /// Get usage report for an agent.
    pub fn usage_report(&self, agent_id: &str) -> UsageReport {
        self.budget_tracker.usage_report(agent_id.to_string())
    }

    /// Get cost summary for a session.
    pub fn cost_summary(&self, session_id: &str) -> Result<SessionCostSummary> {
        let session = self.session_store.load_session(session_id.to_string())?;
        Ok(self.cost_calculator.session_summary(&session))
    }

    /// Reload the preset from a TOML string (hot-reload support).
    pub fn reload_preset(&mut self, toml: &str) -> Result<()> {
        let new_preset = PresetParser::parse(toml)?;
        if let Ok(mut pipeline) = self.pipeline.lock() {
            pipeline.reload_preset(&new_preset)?;
        }
        if let Ok(mut router) = self.model_router.lock() {
            *router = ModelRouter::new(&new_preset);
        }
        if let Ok(mut preset) = self.preset.lock() {
            *preset = new_preset;
        }
        Ok(())
    }

    /// Spawn a background thread that watches `path` for preset file changes.
    ///
    /// Only the preset, pipeline, and model_router are shared with the watcher
    /// thread (via `Arc<Mutex<>>`). All other engine state stays on the main thread.
    pub fn watch_preset_file(&self, path: &Path) -> Result<notify::RecommendedWatcher> {
        use notify::{Event, EventKind, RecursiveMode, Watcher};

        let preset_arc = Arc::clone(&self.preset);
        let pipeline_arc = Arc::clone(&self.pipeline);
        let router_arc = Arc::clone(&self.model_router);
        let watched_path = path.to_owned();

        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                    match std::fs::read_to_string(&watched_path) {
                        Ok(toml_str) => match PresetParser::parse(&toml_str) {
                            Ok(new_preset) => {
                                if let Ok(mut p) = pipeline_arc.lock() {
                                    let _ = p.reload_preset(&new_preset);
                                }
                                if let Ok(mut r) = router_arc.lock() {
                                    *r = ModelRouter::new(&new_preset);
                                }
                                if let Ok(mut pr) = preset_arc.lock() {
                                    *pr = new_preset;
                                }
                            }
                            Err(e) => eprintln!("[sqz] invalid preset: {e}"),
                        },
                        Err(e) => eprintln!("[sqz] preset read error: {e}"),
                    }
                }
            }
        })
        .map_err(|e| SqzError::Other(format!("watcher error: {e}")))?;

        watcher
            .watch(path, RecursiveMode::NonRecursive)
            .map_err(|e| SqzError::Other(format!("watch error: {e}")))?;

        Ok(watcher)
    }

    /// Access the underlying `SessionStore`.
    pub fn session_store(&self) -> &SessionStore {
        &self.session_store
    }

    /// Access the `CacheManager` for persistent dedup.
    pub fn cache_manager(&self) -> &CacheManager {
        &self.cache_manager
    }

    /// Access the `AstParser`.
    pub fn ast_parser(&self) -> &AstParser {
        &self.ast_parser
    }

    /// Access the `TerseMode` helper.
    pub fn terse_mode(&self) -> &TerseMode {
        &self.terse_mode
    }

    /// Reorder context sections using the LITM positioner to mitigate
    /// the "Lost In The Middle" attention bias in long-context models.
    ///
    /// Places highest-priority sections at the beginning and end of the
    /// context window, lowest-priority in the middle.
    pub fn reorder_context(
        &self,
        sections: &mut Vec<crate::litm_positioner::ContextSection>,
        strategy: crate::litm_positioner::LitmStrategy,
    ) {
        let positioner = crate::litm_positioner::LitmPositioner::new(strategy);
        positioner.reorder(sections);
    }

    /// Route content to the appropriate compression mode based on entropy
    /// and risk pattern analysis.
    pub fn route_compression_mode(
        &self,
        content: &str,
    ) -> crate::confidence_router::CompressionMode {
        self.confidence_router.route(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BudgetState, CorrectionLog, ModelFamily, SessionState};
    use chrono::Utc;
    use std::path::PathBuf;

    fn make_session(id: &str) -> SessionState {
        let now = Utc::now();
        SessionState {
            id: id.to_string(),
            project_dir: PathBuf::from("/tmp/test"),
            conversation: vec![],
            corrections: CorrectionLog::default(),
            pins: vec![],
            learnings: vec![],
            compressed_summary: "test session".to_string(),
            budget: BudgetState {
                window_size: 200_000,
                consumed: 0,
                pinned: 0,
                model_family: ModelFamily::AnthropicClaude,
            },
            tool_usage: vec![],
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn test_engine_new() {
        let engine = SqzEngine::new();
        assert!(engine.is_ok(), "SqzEngine::new() should succeed");
    }

    #[test]
    fn test_probe_or_fallback_uses_temp_path_when_candidate_unopenable() {
        // Regression test: an existing-but-inaccessible session store
        // (permission denied, path is actually a directory, etc.) must
        // fall back to a temp-file store instead of propagating a hard
        // error. `open_or_create`'s corruption recovery deletes-and-
        // retries the *same* path, which doesn't help here — a directory
        // can't be removed with `remove_file`, so without a fallback this
        // would have surfaced as "failed to create new session store".
        let dir = tempfile::tempdir().expect("tempdir");
        let candidate = dir.path().join("sessions.db");
        // Make the candidate a directory, not a file — SQLite can't open
        // a directory as a database, and `remove_file` can't delete one,
        // so `open_or_create`'s recovery path is guaranteed to fail too.
        std::fs::create_dir(&candidate).expect("create dir standing in for the db file");

        let resolved = SqzEngine::probe_or_fallback(candidate.clone());

        assert_ne!(
            resolved, candidate,
            "must not resolve to the unopenable candidate path"
        );
        assert!(
            resolved.starts_with(std::env::temp_dir()),
            "must fall back to a path under the system temp dir, got: {}",
            resolved.display()
        );

        // The fallback path itself must actually be usable.
        assert!(
            SessionStore::open_or_create(&resolved).is_ok(),
            "the fallback path returned by probe_or_fallback must itself be openable"
        );
    }

    #[test]
    fn test_probe_or_fallback_keeps_candidate_when_openable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let candidate = dir.path().join("sessions.db");

        let resolved = SqzEngine::probe_or_fallback(candidate.clone());

        assert_eq!(
            resolved, candidate,
            "a perfectly openable candidate path should be used as-is"
        );
    }

    #[test]
    fn test_compress_or_passthrough_returns_result_on_valid_input() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.compress_or_passthrough("hello world");
        assert_eq!(result.data, "hello world");
        assert!(result.tokens_original > 0);
    }

    #[test]
    fn test_compress_or_passthrough_never_panics_on_empty() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.compress_or_passthrough("");
        assert_eq!(result.data, "");
        assert_eq!(result.compression_ratio, 1.0);
    }

    #[test]
    fn test_compress_or_passthrough_handles_json() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.compress_or_passthrough(r#"{"key":"value"}"#);
        // Should compress successfully — data may be TOON-encoded
        assert!(!result.data.is_empty());
    }

    #[test]
    fn test_compress_or_passthrough_handles_binary_garbage() {
        let engine = SqzEngine::new().unwrap();
        // Feed it something weird — should never panic, always return something
        let garbage = "\x00\x01\x02\x7f invalid control chars \t\n\r";
        let result = engine.compress_or_passthrough(garbage);
        assert!(!result.data.is_empty());
    }

    #[test]
    fn test_compress_plain_text() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.compress("hello world");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().data, "hello world");
    }

    #[test]
    fn test_compress_json_applies_toon() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.compress(r#"{"name":"Alice","age":30}"#).unwrap();
        assert!(
            result.data.starts_with("TOON:"),
            "JSON should be TOON-encoded"
        );
    }

    /// Phase 5 regression: `engine.compress()` (the "auto" / default path
    /// used by bare `sqz compress` with no `--mode` flag) must never run
    /// the lossy subsystem on its own, even on content the confidence
    /// router would otherwise auto-classify as Aggressive (low-entropy /
    /// repetitive). Without `SQZ_ALLOW_LOSSY=1` set, this must be gated
    /// back down to Default. This test intentionally does not touch the
    /// env var — it relies on the default (unset) test environment, since
    /// mutating process env is racy under parallel test execution.
    #[test]
    fn test_compress_auto_never_runs_lossy_without_opt_in() {
        assert!(
            std::env::var("SQZ_ALLOW_LOSSY").is_err(),
            "this test assumes SQZ_ALLOW_LOSSY is unset in the default test environment"
        );
        let engine = SqzEngine::new().unwrap();
        // Low-entropy content — near-duplicate short lines that differ only
        // by a small varying digit. Chosen (over a single fully-identical
        // repeated line) because it survives the deterministic CondenseStage
        // largely intact, so it still has enough length/structure left for
        // the lossy subsystem to act on if it were allowed to run — see the
        // `..._fixture_actually_triggers_lossy_stages_when_explicitly_requested`
        // test below, which proves this same fixture is not vacuous.
        let mut input = String::new();
        for i in 0..300 {
            input.push_str(&format!("000000000000000{}\n", i % 7));
        }
        // Sanity: confirm this fixture really is what the router would
        // pick Aggressive for — otherwise this test passes vacuously.
        assert_eq!(
            engine.route_compression_mode(&input),
            crate::confidence_router::CompressionMode::Aggressive,
            "fixture should be classified Aggressive by the raw router (pre-gate)"
        );

        let result = engine.compress(&input).unwrap();
        for lossy in [
            "rle",
            "sliding_window_dedup",
            "entropy_truncate",
            "token_prune",
        ] {
            assert!(
                !result.stages_applied.contains(&lossy.to_owned()),
                "engine.compress() ran lossy stage {:?} without SQZ_ALLOW_LOSSY opt-in: {:?}",
                lossy,
                result.stages_applied
            );
        }
        assert!(
            !result.data.contains("[→L"),
            "dangling back-reference in output: {}",
            result.data
        );
    }

    /// Companion to `test_compress_auto_never_runs_lossy_without_opt_in`:
    /// proves that fixture isn't passing vacuously (i.e. it's not just
    /// content the lossy subsystem would never touch anyway). Uses
    /// `compress_with_mode(Aggressive)`, which bypasses the router/gate
    /// entirely, so it needs no env var mutation — it's a direct, explicit
    /// request that must always work regardless of `SQZ_ALLOW_LOSSY`.
    #[test]
    fn test_low_entropy_fixture_actually_triggers_lossy_stages_when_explicitly_requested() {
        let engine = SqzEngine::new().unwrap();
        let mut input = String::new();
        for i in 0..300 {
            input.push_str(&format!("000000000000000{}\n", i % 7));
        }
        let result = engine
            .compress_with_mode(
                &input,
                crate::confidence_router::CompressionMode::Aggressive,
            )
            .unwrap();
        assert!(
            result.stages_applied.iter().any(|s| {
                s == "rle" || s == "sliding_window_dedup" || s == "entropy_truncate" || s == "token_prune"
            }),
            "fixture should trigger at least one lossy stage under explicit Aggressive mode, got: {:?}",
            result.stages_applied
        );
    }

    /// Regression test for upstream issue #34: multi-byte UTF-8 (non-ASCII)
    /// input used to panic when a stage sliced a `&str` at a byte offset
    /// that landed inside a multi-byte character instead of on a char
    /// boundary. Fixed by routing all slicing through `safe_truncate`/
    /// `safe_split_at`. Covers both the default and Aggressive pipelines
    /// (Aggressive exercises entropy_truncate, the stage most likely to
    /// slice at an arbitrary byte offset), across several multi-byte
    /// scripts (Cyrillic, CJK, emoji with combining sequences).
    #[test]
    fn test_multibyte_utf8_never_panics_issue_34() {
        let engine = SqzEngine::new().unwrap();
        let fixtures: [&str; 4] = [
            &"日本語のテキストです。".repeat(200),
            &"Привет мир, это тестовая строка на русском языке. ".repeat(100),
            &"emoji test 🎉🎉🎉 combining é a\u{0301} family 👨‍👩‍👧‍👦 ".repeat(80),
            &"mixed ASCII and 多字节 UTF-8 été ".repeat(150),
        ];

        for input in fixtures {
            // Default pipeline (what every real command output goes through).
            let result = engine.compress(input);
            assert!(
                result.is_ok(),
                "default pipeline panicked/errored on multi-byte input"
            );

            // Aggressive pipeline (exercises entropy_truncate's slicing paths).
            let aggressive = engine
                .compress_with_mode(input, crate::confidence_router::CompressionMode::Aggressive);
            assert!(
                aggressive.is_ok(),
                "aggressive pipeline panicked/errored on multi-byte input"
            );
        }
    }

    #[test]
    fn test_export_import_ctx_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("store.db");
        let engine = SqzEngine::with_preset_and_store(Preset::default(), &store_path).unwrap();

        let session = make_session("sess-rt");
        engine.session_store().save_session(&session).unwrap();

        let ctx = engine.export_ctx("sess-rt").unwrap();
        let imported_id = engine.import_ctx(&ctx).unwrap();
        assert_eq!(imported_id, "sess-rt");
    }

    #[test]
    fn test_search_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("store.db");
        let engine = SqzEngine::with_preset_and_store(Preset::default(), &store_path).unwrap();

        let mut session = make_session("sess-search");
        session.compressed_summary = "authentication refactor".to_string();
        engine.session_store().save_session(&session).unwrap();

        let results = engine.search_sessions("authentication").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "sess-search");
    }

    #[test]
    fn test_usage_report_starts_at_zero() {
        let engine = SqzEngine::new().unwrap();
        let report = engine.usage_report("default");
        assert_eq!(report.consumed, 0);
        assert_eq!(report.available, report.allocated);
    }

    #[test]
    fn test_cost_summary() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("store.db");
        let engine = SqzEngine::with_preset_and_store(Preset::default(), &store_path).unwrap();

        let session = make_session("sess-cost");
        engine.session_store().save_session(&session).unwrap();

        let summary = engine.cost_summary("sess-cost").unwrap();
        assert_eq!(summary.total_tokens, 0);
        assert!((summary.total_usd - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reload_preset_updates_state() {
        let mut engine = SqzEngine::new().unwrap();
        let toml = r#"
[preset]
name = "reloaded"
version = "2.0"

[compression]
stages = []

[tool_selection]
max_tools = 5
similarity_threshold = 0.7

[budget]
warning_threshold = 0.70
ceiling_threshold = 0.85
default_window_size = 200000

[terse_mode]
enabled = false
level = "moderate"

[model]
family = "anthropic"
primary = "claude-sonnet-4-20250514"
complexity_threshold = 0.4
"#;
        assert!(engine.reload_preset(toml).is_ok());
        // Verify the preset was actually updated
        let preset = engine.preset.lock().unwrap();
        assert_eq!(preset.preset.name, "reloaded");
    }

    #[test]
    fn test_reload_invalid_preset_returns_error() {
        let mut engine = SqzEngine::new().unwrap();
        let result = engine.reload_preset("not valid toml [[[");
        assert!(result.is_err(), "invalid TOML should return error");
    }

    #[test]
    fn test_export_nonexistent_session_returns_error() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.export_ctx("does-not-exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_invalid_ctx_returns_error() {
        let engine = SqzEngine::new().unwrap();
        let result = engine.import_ctx("not valid json {{{");
        assert!(result.is_err());
    }

    // ── Phase 3: mode plumbing ───────────────────────────────────────────

    /// Content that the confidence router classifies as `Default` on its
    /// own (not low-entropy enough to auto-route to `Aggressive`), but
    /// that the lossy subsystem *does* mutate when explicitly requested —
    /// used to prove `compress_with_mode` honors an explicit override
    /// rather than silently re-routing through the confidence router.
    fn repeated_line_fixture() -> String {
        let mut s = String::new();
        for i in 0..80 {
            s.push_str(&format!(
                "heartbeat check {} status ok all systems nominal\n",
                i % 3
            ));
        }
        s
    }

    #[test]
    fn test_compress_with_mode_default_skips_lossy_subsystem() {
        let engine = SqzEngine::new().unwrap();
        let input = repeated_line_fixture();
        let result = engine
            .compress_with_mode(&input, crate::confidence_router::CompressionMode::Default)
            .unwrap();
        for lossy in [
            "rle",
            "sliding_window_dedup",
            "entropy_truncate",
            "token_prune",
        ] {
            assert!(
                !result.stages_applied.contains(&lossy.to_owned()),
                "Default mode ran lossy stage {:?}: {:?}",
                lossy,
                result.stages_applied
            );
        }
    }

    #[test]
    fn test_compress_with_mode_safe_skips_lossy_subsystem() {
        let engine = SqzEngine::new().unwrap();
        let input = repeated_line_fixture();
        let result = engine
            .compress_with_mode(&input, crate::confidence_router::CompressionMode::Safe)
            .unwrap();
        for lossy in [
            "rle",
            "sliding_window_dedup",
            "entropy_truncate",
            "token_prune",
        ] {
            assert!(
                !result.stages_applied.contains(&lossy.to_owned()),
                "Safe mode ran lossy stage {:?}: {:?}",
                lossy,
                result.stages_applied
            );
        }
    }

    /// Regression test: an explicit `Aggressive` request must actually run
    /// the lossy subsystem end-to-end, even for content the confidence
    /// router would otherwise classify as `Default` on its own. Before the
    /// Phase 3 fix, `compress_with_mode` for any non-`Safe` mode dropped
    /// the caller's requested mode entirely and delegated to `compress()`,
    /// which re-routes through the confidence router — silently
    /// downgrading an explicit `Aggressive` escalation (e.g. `cli_proxy`'s
    /// session-pressure-based escalation) back to whatever the router
    /// picked.
    #[test]
    fn test_compress_with_mode_aggressive_is_honored_end_to_end() {
        let engine = SqzEngine::new().unwrap();
        let input = repeated_line_fixture();

        // Sanity: the confidence router does NOT pick Aggressive for this
        // content on its own.
        let auto_mode = engine.route_compression_mode(&input);
        assert_ne!(
            auto_mode,
            crate::confidence_router::CompressionMode::Aggressive,
            "fixture should not auto-route to Aggressive, otherwise this test doesn't \
             prove the explicit override is honored"
        );

        let result = engine
            .compress_with_mode(
                &input,
                crate::confidence_router::CompressionMode::Aggressive,
            )
            .unwrap();
        assert!(
            result.stages_applied.iter().any(|s| {
                s == "rle"
                    || s == "sliding_window_dedup"
                    || s == "entropy_truncate"
                    || s == "token_prune"
            }),
            "explicit Aggressive request should run the lossy subsystem even though \
             the confidence router would have picked {:?}, got stages: {:?}",
            auto_mode,
            result.stages_applied
        );
    }
}
