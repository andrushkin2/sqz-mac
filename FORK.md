# Fork notice

This is a personal, macOS-only fork of [`sqz`](https://github.com/ojuschugh1/sqz)
by Ojus Chugh, maintained by Andrei Kozyakov for personal use.

- Forked from upstream commit: `7fc171872ba24871d16a14133ec13f2840c5abb9`
  (`fix(ci): Allow npm version to no-op when package.json already at target`)
- Fork date: 2026-07-15
- License: Elastic License 2.0 (ELv2), unchanged — see `LICENSE` and `CLA.md`.
  This fork does not remove license notices, does not offer the software as a
  hosted/managed service, and does not modify or bypass any license-key
  functionality.

## Scope of this fork

Personal macOS-only build with two goals, tracked as separate, complementary
efforts (platform pruning does not by itself fix correctness issues):

1. **Platform pruning** — remove distribution/packaging surface with no macOS
   role (Windows installer, browser/IDE extensions, WASM build, Python
   wrapper), while keeping `sqz`, `sqz_engine`, `sqz-mcp`, and `docs/`.
2. **Bug fixes**, found during adversarial review of the upstream project at
   the commit above:
   - Two confirmed UTF-8 slicing panics (`context_evictor.rs`,
     `kv_cache_optimizer.rs`) on multi-byte character boundaries.
   - `--mode safe` did not actually skip the lossy compression stages
     (RLE, sliding-window dedup, entropy-weighted truncation, token pruning)
     because `CompressionMode` was never threaded into `pipeline.compress()`;
     this fork gates the lossy subsystem behind an explicit opt-in
     (`--mode aggressive`), default OFF.
   - That opt-in gate had two gaps: the confidence router could still
     auto-select `Aggressive` for low-entropy content under `--mode auto`
     (the default), and `cli_proxy`'s adaptive session-pressure escalation
     could independently force `Aggressive` regardless of content. Both are
     now capped behind `SQZ_ALLOW_LOSSY=1` — auto-routing and pressure
     escalation only reach the lossy subsystem with explicit opt-in;
     `--mode aggressive` is unaffected.
   - Dangling `[→LN]` RLE back-references with no expand mechanism.
   - Compound shell commands (`&&`, `|`, `>`, `;`) were skipped entirely by
     the compression hook instead of having their sub-commands compressed.

See `~/Desktop/sqz-mac-fork.md` (local planning doc, not part of this repo)
for the full phase-by-phase plan.

## Relationship to upstream

`upstream` remote points at `ojuschugh1/sqz` for pulling future fixes/updates
manually. This fork is not intended to be upstreamed as-is; if any fix here
is broadly useful, it may be proposed back via a separate, minimal PR that
complies with `CLA.md`.
