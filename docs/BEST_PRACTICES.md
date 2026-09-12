# Best Practices Audit + Flexible Multi-Layout Engine Proposal

Read-only audit (2026-09-12). No code changed. Verified by reading
`core-rust/src/*.rs`, `core-rust/Cargo.toml`, `android/*/build.gradle.kts`,
`android/gradle/libs.versions.toml`, `android/ime/.../*.kt`,
`android/core-bridge/.../Predictor.kt`, `android/sync/.../SyncMerge.kt`,
`scripts/build_pack.py`, `packs/*.json` + `packs/README.md`,
`kbpack.schema.json`, `mise.toml`, `justfile`, `docs/*.md`.
Tests: `cargo test -p kbcore` → 35 passed.

Legend: GOOD = keep. FIX = concrete follow-up (not applied here).

## 1. Rust core (`core-rust/`)

| Area | Verdict | Evidence | Action |
|---|---|---|---|
| Crate shape `cdylib + rlib`, edition 2021 | GOOD | `Cargo.toml:4,10` | Keep. |
| Error handling as `Result<_, String>` on loaders/DB | GOOD | `pack.rs:68,73`, `personal.rs:275,283,326`, `session.rs:103,112,138` | Keep; consider `thiserror` only if callers need typed matching (optional). |
| Serde compat (`del` canonical, `deleted` alias) | GOOD | `personal.rs:23` `#[serde(rename="del", alias="deleted")]` + test `tombstone_serde_compat_del_deleted`; `SyncMerge.fromJsonl` mirrors it | Keep. Task's "del/deleted drift" is already resolved — do not re-fix. |
| rusqlite/bundled single-writer discipline | GOOD | `personal.rs:238-244`, `session.rs:97-101`: short-lived `Connection`, WAL, one tx, `Mutex<Inner>` in `predictor.rs:18` | Keep. NDK-friendly (no system libsqlite3). |
| UniFFI narrow FFI, batch-string discipline | GOOD with one gap | `lib.rs:14` `setup_scaffolding!`, `predictor.rs:15-16,22-23`: `new(base_json)`, `suggest/learn/forget/reject/export_session`, JSON in / JSONL out | FIX (one commit): `Predictor::reject` exists in core (`predictor.rs:76-88`) but `docs/ENGINEERING.md:115` still says "not yet exported" — update that doc line + regen Kotlin with `uniffi-bindgen 0.32` so `StubPredictor` retires. Keep surface narrow; no per-word FFI. |
| Tests per module | GOOD | `mapping` 9, `rank` 5, `stack` 6, `personal` 8, `session` 3, `pack` 3, `predictor` 1 = 35 green | Keep. Add layout-registry tests when §7 lands. |
| Doc drift: `core-rust/README.md` claims `build.rs` + `redb` | FIX | No `core-rust/build.rs` on disk; `Cargo.toml:16` is rusqlite, README §§Storage choice/migration describe both | FIX (one commit): rewrite README storage section to rusqlite-only + drop `build.rs` line (UDL-less proc-macro needs none). |
| Hardcoded 9-key map | FIX (core of §7) | `mapping.rs:18-53` `match` on chars; `key_pos` 3×4 grid `mapping.rs:63-79`; `stack.rs`/`predictor.rs` call `encode_word` directly; `build_pack.py:47-82` duplicates the table in Python | FIX: introduce `LayoutSpec` registry (§7). `mapping.rs` becomes the `t9-9` built-in impl, not the only impl. Dedupe Python by generating `_T9_MAP` from an exported `layouts/*.json` or by calling the Rust encoder in CI parity check. |

## 2. Android

| Area | Verdict | Evidence | Action |
|---|---|---|---|
| Toolchain AGP / SDK / Java | GOOD | `libs.versions.toml:2` AGP 9.4.0; `compileSdk 36` all modules, `targetSdk 36` app-only, `minSdk 26` all; `VERSION_17` + `JVM_17` everywhere | Keep. |
| Kotlin version pin | FIX (doc-only) | Task brief says Kotlin 2.2; `libs.versions.toml:3` says `2.3.20` (+ KSP `2.3.12`) while `android/README.md:3-5` and `QA_REPORT.md:11` say 2.2.20 | FIX (one commit): decide 2.2.20 vs 2.3.20, pin one version in catalog + README + QA report together. Do not mix. |
| Plugin choice (`kotlin.compose`, no `kotlin.android`) | GOOD | `libs.versions.toml:36` `org.jetbrains.kotlin.plugin.compose`; module files apply only `android.*` + `kotlin.compose`/`ksp` | Keep. Task's suspected "`kotlin.android` plugin" issue does not exist — verified absent. |
| `:ime → :sync` dependency | GOOD (already fixed) | `ime/build.gradle.kts:32-36` depends on `:plugin-api` + `:core-bridge` only, with an explicit no-`:sync` comment; `:app` owns `:sync` | Keep. Task's suspected dep does not exist — verified absent. |
| IME lifecycle (buffers, composing, scope cancel) | GOOD | `KbInputMethodService`: `onStartInput` resets `seq`/`qwertyBuffer`, `onFinishInput/View` finishes composing + clears, `onUpdateSelection` keeps no stale offsets, `serviceScope.cancel()` + `predictor.close()` in `onDestroy`, `shouldOfferSwitchingToNextInputMethod=true`, API-P+ guard on `switchToNextInputMethod` | Keep. |
| Compose + Views split | GOOD | Views: `KeyPadView` (GridLayout), `QwertyView` (LinearLayout); Compose: `SuggestionStrip` (LazyRow, 3 inline), `CategoryTabs` (ScrollableTabRow), `ImeScreen` + expand-all `LazyColumn` (30) | Keep split; add `PadView` variants per §7 reusing the same host callbacks. |
| Engine sharing T9/QWERTY | GOOD | Service comment + `onDigit` vs `onQwertyKey` both feed `Predictor.suggest/learn`; `QwertyView.kt:9-13` documents encoder-only difference | Keep; extend to 9/12/16 pads (§7). |
| Privacy/permissions | GOOD | Manifest declares only `BIND_INPUT_METHOD`, no `INTERNET`; `shouldLearnNow()` gates password + incognito (`IME_FLAG_NO_PERSONALIZED_LEARNING`/privateImeOptions) + non-TEXT `InputMode` + email/URI + per-category `privacy.learn=false` (numbers/math); sync opt-in off by default | Keep. Note `docs/PRIVACY.md:38` still says storage is "`redb` via core-rust" — stale, fix with the README commit above. |
| DI/testability | FIX | `KbInputMethodService:70` hard-codes `StubPredictor()`; no constructor injection, no `active_tab`/`limit` plumb-through (`refreshSuggestions` calls `suggest(seq, prev)` with default limit) | FIX (one commit): inject `Predictor` via field/setter or ServiceLocator; pass `activeAssetId` + explicit limit (3 inline / 30 expand-all) once bindgen regen lands; add Robolectric/Compose UI tests for pad variants + KSPW/mis-press harness (§7). |
| Hardcoded 9-key UI | FIX (core of §7) | `KeyPadView.kt:24-28` hardcoded 9 labels, `('1' + i)` digits, fixed 4-row grid | FIX: replace with layout-driven `PadView(layout: LayoutSpec)` (§7). |
| 12-key mode | Missing | SPEC §0 wants a 12-key toggle (`Sym|Space|⌫/⏎` bottom row); only 9-key + QWERTY exist | FIX with §7: ship 9/12/16 variants from one component. |

## 3. Packs / tooling

| Area | Verdict | Evidence | Action |
|---|---|---|---|
| T9 parity (`mapping.rs` vs `build_pack.py`) | GOOD but fragile | `packs/README.md:34-36` parity-checked on vectors `hello→43556`, `कमल→267`; `validate --all` + Rust loader double-check, entries == words for all 9 packs | FIX (one commit with §7): single source of truth — export canonical `layouts/t9-9.json` from Rust (or generate Python map from it) instead of maintaining two hand-synced tables. |
| Zipf freqs 1..1e6 | GOOD | `build_pack.py:116-119` `f0=60000, alpha=0.9, fmin=100, fmax=1e6`; `validate_pack` enforces range; observed `120..1e6` | Keep. |
| Size caps | GOOD | 6613 words / ~385 KiB total (`words_en` 5150/296K … `numbers` 10/4K); lean-by-design documented | Keep; add a CI budget check (e.g. total < 512 KiB, per-pack < 350 KiB) in the same commit as CI (§5). |
| Schema scope | GOOD | `kbpack.schema.json` validates `.kbpack` manifests (SPEC §3, 12 fields), not dev-time `packs/*.json` wordlists — `docs/ENGINEERING.md:119` says so explicitly | Keep; do not conflate the two formats. |
| Canonical ids | GOOD with one loose end | 9 ids agree (`words ne js rust html emoji numbers math medical`); `nepali.json` rows still carry display-case `cat "NE"` vs engine id `ne` (`packs/README.md:87-91`, deferred with `cat_boost` exact-match) | FIX (one commit): lowercase cats + case-insensitive `cat_boost` together. |
| Personal cap SPEC vs code | Open decision | Code 10k LFU (`personal.rs:10`), SPEC §4 20k LRU (`packs/README.md:95-97`) | FIX (one commit): pick one (recommend 20k LRU to match SPEC, tombstones still exempt) or amend SPEC to 10k. Blocking for §7 test-harness comparability. |

## 4. Infra (mise / just / git / CI)

| Area | Verdict | Evidence | Action |
|---|---|---|---|
| `mise.toml` pins | GOOD | rust stable, java 17, gradle 9.6.0, python 3.12, just/bun | Keep. Consider pinning `rust = "1.8x"` exact if NDK builds prove sensitive — optional. |
| `justfile` lean wrappers | GOOD | `test-core`, `check-core` (clippy non-blocking), `validate-packs` stdlib-only, `docs-check`, `android-assemble`, read-only `sync-report` | Keep. Add `just validate-layouts` + `just test-all` with §7. |
| Small atomic commits | N/A (fresh repo) | `git log` → no commits yet; `git status` → all files untracked | Process rule (§6): one commit per area below. |
| CI readiness | FIX | No `.github/workflows`, no lint/test automation | FIX (one commit): minimal CI — `cargo test + clippy`, `build_pack.py validate --all`, `docs-check`, pack-size budget. Android `assembleDebug` as a second job once a JDK runner exists. |

## 5. Reusability today (why §7 is needed)

Hardcoded 9-key assumptions found in three places:

1. `core-rust/src/mapping.rs` — `char_to_digit` match arms, `key_pos` 3×4 grid, `key_label`.
2. `scripts/build_pack.py:44-92` — hand-ported `_T9_MAP` (drift risk).
3. `android/ime/.../KeyPadView.kt:24-37` — 9 labels + `('1' + i)` + fixed rows.

`stack.rs` / `predictor.rs` take raw digit strings with no `layout_id`, so a
12/16-key seq is indistinguishable from a 9-key seq. `Predictor::suggest`
signatures already differ across layers (Rust `(ctx, digits, active_tab,
limit)` vs Kotlin stub `(seq, prev, limit)`) — §7 aligns them while adding
the layout dimension.

## 6. Layout flexibility design (proposal — not implemented)

Goal: engine supports 9 / 12 / 16-key pads plus future custom layouts
(4×4 symbol pad, QWERTY-cluster stenography pads) with zero engine forks:
only the `LayoutSpec` registration differs.

### 6.1 `LayoutSpec` abstraction (Rust, new `core-rust/src/layout.rs`)

```rust
pub struct LayoutSpec {
    pub id: String,              // "t9-9" | "t9-12" | "t9-16" | custom
    pub keys: Vec<LayoutKey>,    // key id, label, symbols, digit/code emitted
    pub adjacency: Vec<(char, Vec<char>)>, // fat-finger neighbor graph
}

pub struct LayoutKey {
    pub code: char,         // emitted symbol: '2'..'9' | '0'..'9' | custom
    pub label: &'static str,
    pub symbols: &'static str, // letters / glyphs on this key, for encode
}

pub trait KeyMapping {
    fn layout_id(&self) -> &str;
    fn encode_word(&self, word: &str) -> String;   // word -> seq under this layout
    fn neighbors(&self, a: char, b: char) -> bool; // adjacency-graph lookup
    fn key_label(&self, code: char) -> &str;
}
```

- Built-ins: `t9-9` (current `mapping.rs` behavior, frozen as reference),
  `t9-12` (adds `* 0 #` bottom row as first-class codes, not control-skip),
  `t9-16` (4×4 minus control cluster; finer splits e.g. `abc→2` stays,
  `pqrs→7a/7b`), plus `qwerty-clusters` (multi-letter cluster encoder for the
  fallback view).
- Custom layouts register at runtime: `LayoutRegistry::register(spec)` from a
  `layouts/*.json` file (same JSON ships to Python + Android assets — single
  source of truth, kills the `build_pack.py` duplication).
- `encode`/`decode` fns live on the spec: `encode_word(word, layout_id)`,
  `decode_prefix(seq, layout_id)`. The FST index in `stack.rs` becomes
  per-layout (`HashMap<layout_id, fst::Set>`) or stores `(layout_id, seq)`
  composite keys — decide at implementation time, keep the 200-cap + top-30
  heap semantics unchanged.

### 6.2 Engine API

```rust
// Predictor gains a layout dimension; old 4-arg suggest stays as
// suggest_default() delegating to layout "t9-9" for back-compat.
pub fn suggest(&self, ctx: String, seq: String, layout_id: String,
               active_tab: String, limit: u32) -> Vec<Suggestion>;
pub fn encode(&self, word: String, layout_id: String) -> String;
pub fn layouts(&self) -> Vec<String>; // registered ids
```

- `Suggestion` gains `layout_id: String` so the strip can show mixed-layout
  provenance during A/B tests.
- Kotlin mirror: `suspend fun suggest(seq, prev, layoutId, limit)` with
  default `layoutId = "t9-9"` — no call-site breakage; service passes
  `activePad.layoutId` + `activeAssetId` + limit 3 (strip) / 30 (expand-all).
- `stack.suggest` takes `&dyn KeyMapping` (or `layout_id` + registry lookup)
  instead of calling the free `encode_word`/`is_one_edit_neighbor` globals.

### 6.3 Android `PadView` variants

- One layout-driven component: `PadView(context, spec: LayoutSpecUi)` builds
  its grid from `spec.keys` (rows/cols/labels/48dp targets from data, not
  code). `KeyPadView` (9), `TwelveKeyPadView` (9 + `Sym|Space|⌫/⏎` row per
  SPEC §0), `SixteenKeyPadView` (4×4) become thin constructors over it.
- All variants share the same `Predictor` path: pad emits `code`s, service
  calls `suggest(seq, prev, layoutId, limit)`. Gestures (swipe-left = delete,
  swipe-up = space, long-press = digit/symbols) attach at the `PadView` level
  so every size inherits them.
- Test harness (new `:ime` androidTest + Rust `#[cfg(test)]` vectors):
  per-layout KSPW (keystrokes-per-word) and mis-press recovery
  (1-edit-neighbor hit rate) on a shared fixture wordlist; compare
  9 vs 12 vs 16 in one table to pick the default. Harness feeds identical
  `seq` streams through each registered layout — possible only after §6.2.

### 6.4 Packs + tooling

- `layouts/*.json` checked in (starting with `t9-9.json` generated from
  current `mapping.rs` behavior + `t9-12.json` + `t9-16.json` drafts).
- `build_pack.py` loads the layout JSON for `--layout` (default `t9-9`);
  `validate --all` checks every pack under every layout it declares.
- Pack rows may carry `"layout": "t9-16"` overrides; loader keeps explicit
  `seq` verbatim (emoji/math path unchanged).

## 7. Small-commit plan (one commit per area, in order)

1. `docs: fix storage/build.rs drift` — `core-rust/README.md` rusqlite-only + drop `build.rs` line; `docs/PRIVACY.md:38` redb→rusqlite.
2. `chore: pin kotlin version` — one version in `libs.versions.toml` + README + QA report.
3. `feat(core): LayoutSpec registry + per-layout suggest` — new `layout.rs`, `t9-9` built-in frozen, registry, `suggest(..., layout_id)` + back-compat default, FST per layout, tests.
4. `feat(packs): layouts/*.json source of truth` — export JSON, `build_pack.py --layout`, parity test replacing the hand port.
5. `feat(android): layout-driven PadView 9/12/16` — one component + three constructors, service wires `layoutId`, DI for `Predictor`.
6. `test: KSPW + mis-press harness` — shared fixtures, 9/12/16 comparison table in docs.
7. `chore: pack cat-case + personal cap decision` — lowercase cats + case-insensitive boost; 10k-vs-20k ruling.
8. `ci: minimal green pipeline` — cargo test/clippy, pack validate, docs-check, size budget; Android assemble as follow-up job.
