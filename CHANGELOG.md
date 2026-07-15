# Changelog

All notable changes to sqz will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## sqz-mac fork

Entries below this heading are specific to [`sqz-mac`](https://github.com/andrushkin2/sqz-mac),
a personal macOS-only fork maintained by Andrei Kozyakov, forked from upstream `sqz` at commit
`7fc171872ba24871d16a14133ec13f2840c5abb9` (after 1.3.0). See [FORK.md](FORK.md) for full scope.
Everything under "Upstream history" below is unmodified upstream `sqz` history.

## [1.3.0-mac.2] - 2026-07-15

### Fixed

- **MCP path traversal** — `sqz_read_file`/`sqz_list_dir`/`sqz_grep` now resolve
  requested paths against an allowed-roots guard (`allowed_roots()`/
  `resolve_guarded_path()` in `sqz-mcp/src/lib.rs`), instead of trusting
  caller-supplied paths outright.
- **MCP preset not loaded at startup** — an existing preset on disk is now
  loaded when the MCP server starts (`find_startup_preset()`/
  `default_preset_dir()`), rather than only taking effect on the first
  hot-reload.
- **Hardcoded bare binary names** — the Claude Code hook rewriter, the
  OpenCode plugin, and the generated Claude/Codex/OpenCode MCP client configs
  previously assumed `sqz`/`sqz-mcp` were on `PATH`. They now resolve and
  embed the actual running binary's path (`process_hook_for_platform`/
  `install_*_mcp_config_with_path`/`update_opencode_config_detailed_with_mcp_path`).
- **Session store had no fallback** — `SqzEngine::probe_or_fallback()` now
  falls back to a temp-directory session store (`temp_store_path()`) instead
  of failing outright when the primary store path can't be opened.

### Changed

- `Cargo.lock` is now tracked in git (previously gitignored) for reproducible
  builds of the `sqz`/`sqz-mcp` binary crates.
- Repo-wide `cargo fmt`.
- Resolved all `cargo clippy --all-targets -- -D warnings` findings across
  `sqz`, `sqz-mcp`, and `sqz_engine`.

## [1.3.0-mac.1] - 2026-07-15

First tagged release of the fork: platform pruning (macOS-only), the bug
fixes and issue verifications below, and this changelog/`FORK.md` writeup.

### Fixed

- **`git` command output corrupted under a non-English shell locale** (upstream
  issue [#30](https://github.com/ojuschugh1/sqz/issues/30)) — `cmd_formatters/git.rs`
  keys off English labels (`modified:`, `new file:`, `nothing to commit`, etc.).
  With e.g. `LANG=it_IT`, git emits localized labels the parser doesn't
  recognize, so it falls through to the `git status -s` short-format heuristic
  and misparses the localized text as status codes, garbling the output. Both
  hook rewrite paths (Claude Code's `tool_hooks.rs` and the OpenCode plugin)
  now execute `git` commands with `LC_ALL=C` forced, so this formatter only
  ever sees English output regardless of the user's shell locale.

- **Safe mode wasn't actually safe** — `CompressionMode` was never threaded into
  `pipeline.compress()`, so the lossy subsystem (RLE, sliding-window dedup,
  entropy-weighted truncation, token pruning) ran unconditionally regardless of
  `--mode`. `--mode safe` could still truncate and structurally corrupt real
  source files. The lossy subsystem is now gated: it only runs under an explicit
  `--mode aggressive`, or with `SQZ_ALLOW_LOSSY=1` set. Default (`safe`/`default`/
  `auto`) modes never run it.
- **Auto-routing could still reach the lossy subsystem without opt-in** — the
  confidence router could auto-select `Aggressive` for low-entropy content under
  `--mode auto` (the default), and `cli_proxy`'s adaptive session-pressure
  escalation could independently force `Aggressive` regardless of content or
  explicit mode. Both paths are now capped behind `SQZ_ALLOW_LOSSY=1` via
  `confidence_router::gate_auto_mode()`; `--mode aggressive` is unaffected.
- **UTF-8 slicing panics** on multi-byte character boundaries in
  `context_evictor.rs` (`summarize_for_eviction`, used by `sqz compact`) and
  `kv_cache_optimizer.rs` (`compress_with_sinks` / `compress_with_custom_sinks`).
  Both now use a shared `safe_truncate`/`safe_split_at` helper that finds the
  nearest valid char boundary instead of raw byte slicing.
- **Dangling `[→LN]` RLE back-references** — `rle_compressor.rs`'s sliding-window
  dedup emits back-references using line numbers from *before* later stages
  (entropy truncation) can remove/shift lines, with no expand mechanism (unlike
  `§ref:hash§` dedup, which `sqz expand` can resolve). Fixed as part of gating
  the lossy subsystem behind opt-in, since it's the primary path that produced
  these references without user awareness.
- **Compound shell commands were skipped entirely** — `opencode_plugin.rs` and
  the tool-hook rewriter skipped compression for any command containing `&&`,
  `|`, `>`, or `;` (confirmed with `npm install && npm test`, `cat file | grep
  foo`, `npm install > out.log` all passing through unmodified). The OpenCode
  plugin now splits compound commands on shell operators and compresses each
  sub-command's output individually instead of abandoning compression for the
  whole line, while leaving redirect targets untouched.

### Verified

Reviewed all upstream open issues against this fork's macOS-only, PowerShell-free
scope. Windows-only reports ([#26](https://github.com/ojuschugh1/sqz/issues/26),
[#20](https://github.com/ojuschugh1/sqz/issues/20)) don't apply. The rest were
already resolved and are now locked in with regression tests:

- [#34](https://github.com/ojuschugh1/sqz/issues/34) (multi-byte UTF-8 panics) —
  confirmed fixed by the `safe_truncate`/`safe_split_at` work above; added a
  regression test covering the default and Aggressive pipelines across several
  scripts (Cyrillic, CJK, emoji).
- [#32](https://github.com/ojuschugh1/sqz/issues/32) (`entropy_truncate` runs on
  MCP file reads) — confirmed `cache_manager.rs` (the file-read/dedup path)
  always uses `CompressionMode::Default`, which never reaches the lossy
  subsystem per the gating fix above.
- [#22](https://github.com/ojuschugh1/sqz/issues/22) (OpenCode plugin corrupts
  shell-operator commands) — confirmed the `has_shell_operators` guard is
  present and tested in both hook paths.
- [#21](https://github.com/ojuschugh1/sqz/issues/21) (`sqz init` not idempotent)
  — confirmed the sentinel-based upsert/removal fix (inherited from upstream)
  holds end-to-end: verified 4x `sqz init --global` produces exactly one hook
  entry per event, and `sqz uninstall` removes them all. Added a round-trip
  unit test.
- [#18](https://github.com/ojuschugh1/sqz/issues/18) (stats stay at zero) — the
  reported trigger (a VS Code extension not wired to the stats DB) doesn't
  apply; this fork ships no VS Code extension (see Removed, below). Confirmed
  both paths this fork does ship — the CLI/shell-hook path and the MCP server —
  correctly log to `sqz stats`. Added end-to-end CLI integration tests.

### Removed

- Non-macOS packaging and distribution surface with no role in a personal
  macOS build: Windows installer (`install.ps1`), Dockerfile, VS Code
  extension, JetBrains plugin, Chrome/Firefox browser extensions, npm package,
  Python wrapper (`pyproject.toml`), and the `sqz-wasm` crate (removed from the
  Cargo workspace).
- PowerShell shell-hook and completion support (`ShellHook::PowerShell`,
  `sqz.ps1`) — bash, zsh, fish, and Nushell remain supported.
- Non-shell completion scripts beyond fish/nu/bash/zsh.
- Generated rustdoc snapshot (`docs/doc/`) from version control — it's rebuilt
  and published by `.github/workflows/docs.yml` on every push, so committing
  it was redundant (and had gone stale, still referencing the removed
  `sqz-wasm` crate).

## Upstream history

## [1.3.0] — 2026-06-21

### Added

- **`sqz dashboard` live metrics** — background thread polls the session
  store every 5s and pushes token totals, cache hit/miss counters, per-tool
  and per-command breakdowns, and session history through the SSE endpoint.
  `cmd_dashboard` uses `dirs-next` for cross-platform home dir lookup.
- **Expanded per-command formatters** — from ~12 commands to 40+ across 9
  ecosystems (git, Rust, JS/TS, Python, Go, Cloud, Containers, JVM, System).
  Purpose-built compression now covers the vast majority of commands seen in
  AI coding sessions.
- **New command support** — gh (GitHub CLI with JSON parsing), ruff, mypy, pip,
  go test `-json` event stream parser, go build/vet, golangci-lint, grep/rg/ag
  (group by file), tree (collapse noise directories), curl/wget (strip progress),
  aws, terraform plan/apply/init, gcloud, gradle build/test, maven.
- **Ruby ecosystem** — rspec (JSON + text), rubocop, rake (test tasks),
  bundle (install/exec unwrap).
- **pnpm/yarn/bun support** — package manager detection and formatting for
  install, audit, outdated commands.
- **ANSI pre-processing** — formatters now strip ANSI escape codes and progress
  bars before parsing, improving reliability on colored terminal output.
- **Truncation caps** — large outputs (>30 errors, >15 warnings) are capped
  with "...+N more" summaries to prevent unbounded context consumption.
- **Property tests** — 7 proptest properties ensure formatters never panic,
  never expand output, and behave deterministically across arbitrary inputs
  including binary data and unicode.
- **MCP-path compression logging** — MCP tool calls (compress, sqz_read_file,
  sqz_list_dir, sqz_grep) now log their savings to the session store, so
  `sqz stats` and `sqz gain` reflect MCP usage, not just the shell-hook path.
- **Reproducible benchmark fixtures** — `cmd_formatter_bench.rs` provides the
  test inputs behind every BENCHMARKS.md number. Run
  `cargo test -p sqz-engine cmd_formatter_bench -- --nocapture` to regenerate.

### Changed

- **Modular cmd_formatters architecture** — `cmd_formatters.rs` split into a
  directory of 18 files organized by ecosystem. Each ecosystem module is
  independently testable and extensible.
- **Cargo formatter rewrite** — block-based build parser (skips Compiling/
  Downloading noise, groups errors with source context), multi-suite test
  result aggregation, proper clippy formatter with error blocks and
  warning-by-rule grouping with source locations.
- **Git formatter improvements** — diff now includes a file-level summary
  header (file count, +/- stats), new sub-commands: show, stash, remote,
  fetch.
- **npm formatter improvements** — audit (JSON + human-readable), outdated
  package listing with truncation.

### Fixed

- **Shell hook drops subcommand from `--cmd`** — the Bash hook passed only
  the base name (`extract_base_command("git status")` → `"git"`), so every
  subcommand-routed formatter (git/cargo/go/jvm/rake) silently fell through
  to generic compression in the live hook. Now passes the full command.
  Bug predates all formatter work.
- **PowerShell tool not recognized on Windows Claude Code** (issue #26) —
  Claude Code 2.1.126+ on Windows exposes a `PowerShell` tool. The hook's
  is_shell allowlist only matched Bash/Shell variants, so PowerShell calls
  passed through unchanged. The installed matcher was also hard-coded to
  `Bash`. Now recognises `PowerShell`/`pwsh` and uses `Bash|PowerShell`
  regex matcher in `.claude/settings.local.json` and
  `~/.claude/settings.json`.
- **vizit fails to compile on Windows MSVC** (PR #25) — three `libc::STDOUT_FILENO`
  / `STDIN_FILENO` calls were not gated behind `cfg(unix)`. On Windows the
  dashboard now prints a single plain-text snapshot and exits, skipping the
  unsupported raw-mode path.
- **vizit non-TTY snapshot truncated to footer** — `strip_ansi` only handled
  CSI codes ending in `m`, so the cursor-home (`\x1b[H`) and hide-cursor
  (`\x1b[?25l`) prefix from `render_frame` made the function eat everything
  up to the first `m` byte further down. Now treats any byte in `0x40..=0x7e`
  as a CSI final byte.
- **`sqz gain` reported "No compression data yet" with stats present** —
  `cmd_gain` couldn't distinguish "no data at all" from "no data in the
  selected time window". Now checks `stats.total_compressions` to give the
  correct message in each case.

### Chore / Docs

- `.kiro/` added to `.gitignore` (PR #27) — Kiro hooks generated by
  `sqz init` contain machine-specific paths and shouldn't be committed.
- `docs/integrations/level3/api-proxy.md`: status flipped from
  "Not yet implemented" to "Implemented (since v0.5.0)" (PR #28).
- README: added `sqz reset` to the CLI command listing.

## [1.2.0] — 2026-06-02

### Added

- **`sqz reset` command** (issue #17) — clear the dedup cache, compression
  stats, or both. Supports `--cache-only`, `--stats-only`, `--project .`
  for scoped resets. Gives users control when switching models or starting
  fresh on a project where stale `§ref:HASH§` tokens confuse the agent.

### Fixed

- **Windows: `sqz init` not idempotent** (issue #21) — re-running `sqz init
  --global` appended duplicate hook entries instead of replacing them. Root
  cause: the upsert sentinel was `"sqz hook claude"` which doesn't match
  `"sqz.exe hook claude"` on Windows (`.exe` breaks the substring match).
  Fixed by using subcommand-only sentinels (`"hook claude"`, etc.).
- **Windows: hook command fails under Git Bash** (issue #20) — Claude Code
  runs hooks through Git Bash where `\` is an escape character. The stored
  path `C:\Users\...\sqz.exe` collapsed to `C:Users...sqz.exe`. Fixed by
  normalizing all backslashes to forward slashes (`C:/Users/.../sqz.exe`)
  which works in Git Bash, cmd.exe, and PowerShell.
- **OpenCode: shell operators corrupt commands** (issue #22) — the OpenCode
  plugin was missing the `hasShellOperators` guard that the Claude Code path
  already had. Commands with heredocs, pipes, redirects, and compound
  operators (`&&`, `||`) got `2>&1 | sqz compress` appended, breaking the
  command structure. Fixed in both the generated TS plugin and the Rust hook
  processor.
- **CI: Hardened homebrew dispatch** (issue #15) — removed dead
  `update-homebrew` job and stale `homebrew/sqz.rb`. Dispatch now skips
  gracefully if `HOMEBREW_TAP_TOKEN` is unset and surfaces the actual API
  error on failure.

## [1.1.1] — 2026-05-10

### Added

- **Kiro IDE and CLI integration** — `sqz init` now configures Kiro with
  PreToolUse hooks and MCP server registration. Supports both Kiro IDE
  (`.kiro/settings/mcp.json`) and Kiro CLI hook format.
- **Proptest regression seeds** — added seeds for vizit and api_proxy to
  prevent flaky test regressions.

### Changed

- README: Updated stats output examples and CLI commands section.
- CLAUDE.md added to `.gitignore` (generated per-project, not committed).

## [1.1.0] — 2026-05-09

### Added

- **`sqz vizit`** — live terminal dashboard showing real-time compression
  stats, cache hits, and session activity in a TUI interface.
- **Colorful terminal output** — `sqz stats` and `sqz gain` now use colored
  output with progress bars and visual indicators.
- **Per-command breakdown** — stats now show compression savings broken down
  by command type (git, cargo, npm, etc.).
- **Adaptive compression pressure** — automatically adjusts aggressiveness
  based on observed savings patterns.
- **Homebrew tap** — `brew install ojuschugh1/tap/sqz` now works. CI
  dispatches to the homebrew-sqz tap on release.
- **Featured badge** — linking to NextGen Tech Insider article.

### Fixed

- **VS Code extension wired to real stats** (issue #12) — the extension
  now reads from the actual sqz session store instead of showing zeros.
- **npm auto-publish on release** — the publish workflow now triggers
  correctly and binary wrappers are improved.

### Changed

- README: Added cargo install instructions for both `sqz-cli` and `sqz-mcp`.

## [1.0.9] — 2026-04-23

### Fixed

- **MCP tools now use dedup cache** (issue #12 follow-up) — all 4 MCP tool
  handlers (`compress`, `sqz_read_file`, `sqz_grep`, `sqz_list_dir`) were
  calling `engine.compress()` directly, bypassing `CacheManager`. The dedup
  cache — sqz's headline 92% savings feature — never fired through the MCP
  path. Now routed through `compress_with_cache()`. Second read of the same
  content returns a 13-token `§ref:HASH§` instead of re-compressing.
- **Test isolation** — `make_server()` in sqz-mcp tests now uses a per-tempdir
  SQLite store so cache state doesn't bleed between tests.

### Added

- **`sqz stats --project` / `sqz gain --project`** — filter stats and gain
  charts to the current project directory.
- **`sqz print-opencode-plugin`** — dump the generated OpenCode plugin to
  stdout for manual install without running `sqz init`.

## [1.0.8] — 2026-04-23

### Fixed

- **JSONC trailing commas broke OpenCode config merge** (issue #6) —
  `strip_jsonc_comments` removed `//` and `/* */` comments but left
  trailing commas intact. Trailing commas are valid JSONC but invalid
  JSON, so `serde_json` failed to parse configs like
  `{ "mcp": { "dart": { ... }, }, }`. The error was silently swallowed,
  so `sqz init` printed "OpenCode hook installed" while the `.jsonc`
  file was never modified. Fix: added a string-aware second pass that
  strips commas before `]` and `}`.
- **Codex integration test env-var races** — replaced
  `std::env::set_var("CODEX_HOME")` with home-dir-injectable `_at`
  variants (same pattern as `claude_md_integration`), eliminating flaky
  failures in `remove_codex_mcp_config` and `prop_compressed_output_is_valid_json`.

## [1.0.7] — 2026-04-22

### Fixed

- **Claude Code utilization gap** (issue #12 follow-up) — sqz-mcp was
  connected but Claude Code never called its tools because the MCP server
  only offered `compress` while the PreToolUse hook already compressed
  Bash output. Added guidance in the tool description so Claude knows
  when to prefer the MCP tool over the built-in Read/Grep.
- **OpenCode plugin always regenerated on init** — `sqz init` now
  overwrites `~/.config/opencode/plugins/sqz.ts` every time to pick up
  fixes (V1 shape, `--cmd` rewrite, etc.) without requiring manual
  deletion. Also adds `enabled: true` to the MCP entry in opencode.json.
- **Codex integration test env-var races** — eliminated `std::env::set_var`
  in codex tests; each test now uses isolated temp directories with
  `CODEX_HOME` override instead of mutating the global `HOME`.

## [1.0.6] — 2026-04-22

### Fixed

- **MCP: `notifications/initialized` caused Claude Code to mark server as
  failed** (issue #12) — the server was returning an error response to
  JSON-RPC notifications (messages without an `id`). Per the spec,
  notifications are one-way and must produce no output. Claude Code saw
  the unexpected error and marked sqz-mcp as failed. Fix: silently ignore
  all notifications. 2 regression tests added.

## [1.0.5] — 2026-04-22

### Added

- **`sqz init --only <agents>`** — comma-separated list of agents to install.
  Only the named tools get configured; all others are skipped. Accepts:
  claude, cursor, windsurf, cline, gemini, opencode, codex. Aliases like
  `claude-code`, `gemini-cli`, `roo` (= cline) are also recognised.
  Requested in issue #11 (@shochdoerfer).
- **`sqz init --skip <agents>`** — exclude list (complementary to `--only`).
  Cannot be combined with `--only`.
- **`sqz compress --cmd <name>`** — pass the base command label as a CLI
  argument instead of the `SQZ_CMD=` env var. Shell-neutral: works in
  PowerShell, cmd.exe, and POSIX shells.

### Fixed

- **OpenCode plugin shows as `file:///...` instead of "sqz"** — migrated
  the TS plugin to OpenCode's V1 shape with `export default { id: "sqz",
  server: factory }`. Modern OpenCode reads the `id` field and displays
  "sqz" in the plugin list. Legacy OpenCode still loads via the named
  export fallback. Both paths fire exactly once (identity dedup).
- **Windows: `SQZ_CMD=cmd` CommandNotFoundException** (issue #10) — the
  rewritten command used sh-style inline env-var syntax which fails in
  PowerShell and cmd.exe. Changed all 3 emission sites (tool_hooks.rs,
  opencode_plugin.rs TS, opencode_plugin.rs Rust) to use `--cmd NAME`
  instead. `SQZ_CMD=` still recognised as legacy fallback.
- **OpenCode duplicate plugin load** (issue #10) — `sqz init` registered
  the plugin both via `opencode.json` (`"plugin": ["sqz"]`) and via
  `~/.config/opencode/plugins/sqz.ts`. OpenCode loaded both. Fix: stop
  writing the `plugin` array entry; rely on the auto-discovered .ts file.
  Legacy entries cleaned up on re-run.
- **npm install: nested tarball layout** — handle pre-v1.0 release tarballs
  that contain a wrapper directory instead of a flat binary.

### Testing

- 1099 tests total, 0 failures

## [1.0.4] — 2026-04-21

### Fixed

- **CI: `rm: sqz: is a directory`** — the packaging step copied the binary
  to `./sqz` which collided with the `sqz/` crate directory. Now stages
  binaries in a temp directory before creating tarballs.

## [1.0.3] — 2026-04-21

### Fixed

- **CI: Exclude sqz-wasm from native builds** — `sqz-wasm` depends on
  `wasm-bindgen` which can't compile for non-wasm targets. The release
  workflow now uses `--bin sqz --bin sqz-mcp` (single command, two binaries)
  instead of building the full workspace. Benchmarks use `--exclude sqz-wasm`.
  Docs use `--exclude sqz-wasm`. This was the root cause of v1.0.0, v1.0.1,
  and v1.0.2 build failures on Linux and macOS.

## [1.0.2] — 2026-04-21

### Fixed

- **CI: Release workflow builds all targets again** — v1.0.0 and v1.0.1
  failed to build Linux and macOS binaries because the workflow used
  `--bin sqz` + `--bin sqz-mcp` as separate commands. Changed to build
  the full workspace in one pass. Packaging step now gracefully skips
  missing binaries instead of failing the entire job.

## [1.0.1] — 2026-04-21

### Added

- **`sqz expand <ref>`** — CLI command to recover original content from a
  `§ref:HASH§` dedup token. Accepts hash prefixes or the full `§ref:...§`
  token. Returns exact original bytes from the cache. Exit codes distinguish
  hit (0), no-match (1), ambiguous (1), and error (2).
- **`sqz compress --no-cache`** — per-invocation opt-out from dedup. The
  compression pipeline still runs but the 13-token shortcut never fires.
- **`SQZ_NO_DEDUP=1` env var** — same effect as `--no-cache`, settable once
  in shell config for models that can't handle `§ref:...§` tokens.
- **MCP `passthrough` tool** — returns input byte-exact with zero transforms.
  Agents that need raw data can call this instead of `compress`.
- **MCP `expand` tool** — MCP equivalent of `sqz expand`. Agents can resolve
  dedup refs without shelling out.
- **Original bytes stored in cache** — new `original` BLOB column on
  `cache_entries` so `expand` returns true uncompressed content, not the
  compressed version. Additive migration; pre-migration rows return
  compressed-only with a note.
- **Escape hatch docs in rules files** — Cursor, Windsurf, Cline, and Codex
  AGENTS.md templates now include the four escape paths so agents discover
  them without human intervention.

### Fixed

- Agents that can't parse `§ref:HASH§` tokens (e.g., GLM 5.1 on Synthetic)
  now have four independent ways to bypass dedup, breaking the 500-tiny-call
  loop reported by SquireNed.

## [1.0.0] — 2026-04-21

## [0.10.0] — 2026-04-21

### Added

- **`sqz init --global` / `-g`** — installs Claude Code hooks to user-scope
  `~/.claude/settings.json` so compression works across all projects without
  per-repo setup. Merges with existing user settings (preserves permissions,
  env, statusLine, unrelated hooks). Following RTK's model and Anthropic's
  official scope table (Managed > Local > Project > User).
- **Native OpenAI Codex integration** — `sqz init` now configures Codex via
  `~/.codex/config.toml` MCP server entry.
- **Release workflow ships sqz-mcp** — both `sqz` and `sqz-mcp` binaries are
  now built and packaged for all 5 platforms. npm/pip/curl installers updated
  to install both (sqz-mcp is optional — soft failure if tarball missing).

### Fixed

- **npm install silent failure** — the postinstall script expected sqz-mcp
  tarballs that weren't in the release. Now handles missing sqz-mcp gracefully
  and rejects tarballs that unpack as directories instead of binaries.
- **`sqz init` project-scope was invisible across projects** — hooks written to
  `.claude/settings.local.json` only applied inside that one repo. `--global`
  is now the recommended first-install path (documented in README).
- **OpenCode plugin double-wrap** — `SQZ_CMD=SQZ_CMD=ddev ...` runaway prefix
  from issue #5 follow-up. Added `isAlreadyWrapped()` guard checking for
  `SQZ_CMD=`, `sqz compress`, pipe-to-sqz, and bare sqz invocations.
- **OpenCode plugin env-var base extraction** — `FOO=bar make test` now picks
  `make` as the base command, not `FOO=bar`.
- **MCP `tools/list` outputSchema** (issue #5) — dropped invalid
  `outputSchema: {type: "string"}` from all tools. OpenCode's validator
  requires `type: "object"` when present; our tools return plain text so
  outputSchema is now omitted entirely.

### Changed

- `sqz uninstall` now also cleans up user-scope Claude Code settings
  (`~/.claude/settings.json`), removing only sqz entries and preserving
  everything else.
- README updated: `--global` is the recommended install path, Star History
  chart added.

### Testing

- 1062 tests total, 0 failures
- 8 new tests for global install: fresh install, merge semantics, idempotency,
  stale-hook upgrade, uninstall preserves user config, uninstall deletes
  sqz-only files, no-op on missing, refuses corrupted JSON

## [0.9.0] — 2026-04-20

## [0.8.0] — 2026-04-19

## [0.7.0] — 2026-04-18

### Added

- **Structural summary extraction** — code files compressed to imports + function
  signatures + call graph (~70% reduction). The model sees the architecture, not
  implementation noise.

### Fixed

- **MCP `initialize` capability (issue #3)** — changed `"tools": {}` to
  `"tools": {"listChanged": false}` per MCP 2024-11-05 spec. OpenCode and other
  compliant clients were interpreting the empty object as "no tools" and skipping
  `tools/list`. Regression test added.
- **MCP `tools/list` outputSchema (issue #5)** — all 8 tools declared
  `outputSchema: {type: "string"}` which violates the MCP spec (root type must be
  `"object"` when present). OpenCode rejected all tools during discovery. Fix:
  dropped outputSchema entirely since all tools return plain text via
  `content[{type:"text"}]`, not structured content. Two regression tests added.
- **Windows path escaping in hook configs (issue #2)** — `std::env::current_exe()`
  returns backslash paths on Windows. These were interpolated raw into JSON/TS
  string literals, producing invalid JSON. Added `json_escape_string_value()` helper
  implementing RFC 8259 escaping. Markdown rules files (Windsurf/Cline) keep raw
  paths for copy-paste readability. 7 new tests.
- **Hook format corrections** — matched hook JSON output to official docs for Claude
  Code (`hookSpecificOutput.updatedInput`), Cursor (flat `permission` + `updated_input`,
  `"version": 1`, matcher `"Shell"`), Gemini CLI (`decision` + `hookSpecificOutput.tool_input`),
  and Windsurf (`agent_action_name` + `tool_info.command_line`). Windsurf/Cline
  downgraded to prompt-level `.windsurfrules`/`.clinerules` guidance since they
  don't support command rewriting via hooks.
- **Word abbreviation removed from CLI and WASM paths** — the n-gram abbreviator
  was mangling directory names and filenames in `ls -l` output. Removed from the
  shell hook compression path and browser extension.
- **RLE false-positive on `ls -l` output** — the pattern-run compressor was
  collapsing filenames that happened to share prefixes. Fixed.
- **GitDiffFoldStage false-positive on `ls -l`** — the diff folder was triggering
  on lines starting with `d` (directory entries). Fixed.
- **`sqz init` now asks for confirmation** before modifying existing files.
- **Audit findings addressed** — H-1, M-1, M-2, M-6, M-9, M-12, L-13 from
  external security audit.

### Changed

- Benchmark doc corrected: edited file re-reads use delta encoding (~60-75 tokens),
  not dedup refs (13 tokens). Session totals updated accordingly.
- npm README synced with root README.

### Testing

- 1010 tests total (up from 947 in 0.6.0), 0 new failures
- 1 pre-existing flaky proptest in `api_proxy` (SQLite temp file race, unrelated)

### Also in this release

- **PreCompact hook** — invalidates dedup refs before context compaction so stale
  references don't survive into the next context window.
- **Dedup freshness persistence** — dedup hit tracking now persists across sqz
  processes via SQLite, so `sqz stats` reflects real savings.
- **Dedup stats logging** — dedup hits are now logged so `sqz stats` shows them.
- **Preservation-token verifier** — catches silent identifier rewrites during
  compression (e.g., function names mangled by abbreviation).
- **Cursor downgraded to rules-based guidance** — Cursor cannot rewrite commands
  via hooks; switched to `.cursorrules` prompt-level guidance.
- **Windows install docs** — pointed Windows users at prebuilt binary paths.

## [0.6.0] — 2026-04-17

### Added

- **OpenCode plugin support** — transparent compression for OpenCode via a TypeScript plugin
  (`~/.config/opencode/plugins/sqz.ts`). Unlike other tools that use JSON hook configs,
  OpenCode requires a TS factory function. `sqz init` installs the plugin, creates
  `opencode.json` with MCP config, and handles idempotent re-runs. New `sqz hook opencode`
  subcommand routes to the OpenCode-specific hook processor which handles OpenCode's
  `tool + args` field format (vs `toolName + toolCall` used by Claude Code / Cursor).
  15 new tests covering plugin generation, install, config update, and hook processing.

- **Schema-Aware JSON Projection** — `project_json()` strips API responses to only the
  fields the agent needs, going beyond null removal to eliminate entire irrelevant keys.
  Configurable via field allowlist or deny list. Particularly effective on large API
  responses (GitHub issues, REST payloads) where agents need 3-5 fields out of 50+.

- **`sqz compact` command** — proactive context eviction. The agent can call `sqz compact`
  to summarize and evict stale session context before the window fills, rather than waiting
  for reactive compaction. Supports `--strategy` (keep_recent, keep_relevant, keep_errors)
  and `--retain-minutes` flags.

### Changed

- `generate_hook_configs()` now includes OpenCode in the returned config list
- `install_tool_hooks()` also installs the OpenCode TypeScript plugin (user-level)
- README: OpenCode added to the supported tools table
- `cmd_hook()` in CLI now dispatches `"opencode"` to `process_opencode_hook()` instead
  of the generic `process_hook()`

### Testing

- 947 tests total (up from 800 in 0.5.0), 0 failures
- 15 new OpenCode plugin tests
- 1 pre-existing flaky proptest in `api_proxy` (SQLite temp file race, unrelated)

## [0.5.0] — 2026-04-16

### Added

#### Novel Features (no competitor has these)
- **Compression Transparency Protocol** — structured annotations (`[sqz: 847→312 tokens | stripped: 12 nulls | confidence: 0.97 ✓]`) that tell the LLM exactly what was compressed, so it can decide whether to re-read content in full
- **Compression Regret Tracker** — learns from compression mistakes per-file. When the LLM re-reads dedup'd content or the verifier triggers a fallback, aggressiveness is reduced for that file. Successful compressions slowly recover aggressiveness. Produces per-file profiles and regret reports
- **Compression Cascades** — multi-level degradation as content ages out of relevance: Fresh (full compressed) → Aging (signatures + changed lines) → Old (file name + public API count) → Ancient (one-line reference). Configurable turn thresholds. sqz controls what's lost, not the LLM's unpredictable compaction

#### Advanced Compression Algorithms
- **MinHash + LSH** — locality-sensitive hashing for O(1) near-duplicate detection in the cache, replacing linear scans
- **Parse Tree Compressor** — tree-sitter-based code compression that collapses low-entropy AST subtrees while preserving high-entropy (information-dense) nodes
- **AST Delta Encoding** — tree-sitter-powered semantic diffs that produce compact change descriptions instead of line-level diffs
- **KV Cache Optimizer** — preserves attention sink tokens (first N tokens) and prompt cache boundaries during compression for better LLM comprehension
- **Adaptive Semantic Tree** — builds a priority-scored tree from document structure and prunes to a token budget, with optional query-aware relevance boosting

#### API Proxy
- `sqz proxy --port 8080` — HTTP proxy that intercepts full LLM API request payloads (OpenAI, Anthropic, Google formats) and compresses them before forwarding. Tracks per-request compression stats

### Changed
- README rewritten — honest benchmark numbers, separated measured (single-command) from session-level (with dedup) savings tables
- Benchmark table now matches actual `cargo test -p sqz-engine benchmarks` output exactly

### Fixed
- Removed unused imports from `regret_tracker` and `cascade_compressor`
- Confidence router no longer false-positives on git logs containing words like "password" or "migration" in commit messages

### Testing
- 800 tests (796 unit + 4 doc tests), 0 failures
- Property-based tests cover all new modules

## [0.1.0] — 2026-04-11

### Added

#### Phase 1 — Core Engine + CLI Proxy
- Rust workspace with 4 crates: `sqz_engine`, `sqz`, `sqz-mcp`, `sqz-wasm`
- Core data model types and enums (`Content`, `Session`, `Preset` with `PresetHeader`, `CacheResult`, etc.; `SessionState` / `PresetMeta` kept as compatibility aliases)
- TOON encoder/decoder — lossless JSON compression with ASCII-safe output
- 8-stage compression pipeline (keep_fields, strip_fields, condense, strip_nulls, flatten, truncate_strings, collapse_arrays, custom_transforms)
- TOML preset parser with validation and hot-reload
- SQLite FTS5 session store with full-text search
- SHA-256 file cache with LRU eviction and cross-session persistence
- Immutable correction log with compaction protection
- Cost calculator with per-tool USD breakdown and cache discount awareness
- Budget tracker with multi-agent support and predictive warnings
- Pin/unpin content protection from compaction
- Tree-sitter AST parser for 18 programming languages
- Prompt cache detector for Anthropic (90%) and OpenAI (50%) boundaries
- Model router with complexity-based local/remote routing
- Terse mode system prompt injection (3 levels)
- CTX format serializer/deserializer for cross-model session portability
- Plugin API (Rust trait + WASM interface) with priority-ordered pipeline insertion
- SqzEngine facade wiring all modules together
- CLI binary with shell hooks (Bash, Zsh, Fish, PowerShell)
- CLI commands: init, compress, export, import, status, cost
- 100+ CLI compression patterns
- Cross-compilation configs for 5 platforms (Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64)
- Distribution: cargo, brew, npm, pip, curl script, Docker, GitHub Releases

#### Phase 2 — MCP Server
- MCP server with stdio and SSE transports
- Tool selector with Jaccard similarity matching
- Preset hot-reload via file watcher (<2s)
- JSON-RPC 2.0 handler (initialize, tools/list, tools/call)
- Platform integration configs for 15 Level 1 + Level 2 platforms
- npm and pip distribution wrappers
- Homebrew formula

#### Phase 3 — Browser Extension (WASM)
- WASM build target with self-contained TOON encoder
- Chrome extension manifest v3
- Content scripts for 5 web UIs (ChatGPT, Claude.ai, Gemini, Grok, Perplexity)
- Compression preview banner for content > 500 tokens
- Settings popup with stats display

#### Phase 4 — IDE Native Extensions
- VS Code extension with CLI bridge, status bar widget, 7 commands
- JetBrains plugin with CLI bridge, status bar widget, 5 actions
- Image-to-semantic-description compression (95%+ reduction)
- Level 3 platform publishing guides (VS Code Marketplace, JetBrains Marketplace, Chrome Web Store, API proxy)

#### Testing
- 753 tests across all crates
- 81 property-based correctness properties via proptest
- Property tests cover: TOON round-trip, token reduction, ASCII safety, cache dedup/invalidation/LRU/persistence, budget invariants, pin round-trips, CTX round-trip, preset round-trip, plugin priority, tool selection cardinality, model routing, terse mode injection, prompt cache preservation, cross-tokenizer determinism, and more
