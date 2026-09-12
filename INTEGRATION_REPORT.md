# Integration Report — Rust core + packs

Date: 2026-09-12. Scope: `core-rust/`, `packs/`, root only.
Untouched: `android/`, `docs/` (read-only checks for ID reconciliation).
No rename of `core-rust/` (deferred, see root `README.md`).

## 1. Test results

`cargo test` in `core-rust/`, after all edits below: **green**.

- `Running unittests src/lib.rs`: **33 passed, 0 failed**, 0 ignored.
- Doc-tests: 0 passed, 0 failed.
- Coverage: mapping (incl. `hello`->`43556`, Devanagari collapse, 1-edit
  neighbors), ranking (exact > neighbor, personal boost, reject penalty,
  recency, freq ordering), stack (exact/fuzzy/category-boost/priority/
  personal promote+block/FST prefix), personal (learn, password-mode bypass,
  tombstones, LFU cap, redb + JSONL roundtrips), session (export/import,
  garbage rejection), pack loader (encode, `seq` override, invalid reject),
  predictor roundtrip.

Flake note: one intermediate run showed
`personal::tests::lfu_cap_evicts_coldest` FAILED, then passed on immediate
retry (final state 33/33 green). Root cause is pre-existing test timing, not
this integration: all four `learn` calls land in the same wall-clock second,
so the LFU victim tie-break on `(count, last_seen)` falls through to
`HashMap` iteration order and can evict the just-added word. Untested fix
idea (not applied — kept changes small): use unique timestamps or
`with_cap` + explicit `last_seen` control in that test.

Packs validation via the existing loader (transient integration test
`tests/tmp_pack_load.rs`, run then deleted — no temp files left in repo):
**8/8 showcase packs load** through `pack::load_pack`, every word encoding to
a non-empty `seq`:

| File | `id` | Words | Entries |
|---|---|---|---|
| `words_en.json` | `en` | 50 | 50 |
| `nepali.json` | `ne` | 30 | 30 |
| `numbers.json` | `numbers` | 10 | 10 |
| `code_js.json` | `js` | 30 | 30 |
| `code_rust.json` | `rust` | 32 | 32 |
| `code_html.json` | `html` | 33 | 33 |
| `emoji.json` | `emoji` | 20 | 20 |
| `math.json` | `math` | 20 | 20 |

Total 225 words. `cargo test` (33 unit tests) still passes after every edit
in this report.

## 2. UniFFI decision: bumped 0.29 -> 0.32 (kept, build green)

- Before: `uniffi = { version = "0.29", features = ["build"] }`
  (locked 0.29.5).
- Trial: bumped `Cargo.toml` to `0.32`, ran `cargo check -p kbcore` → success
  (resolves `uniffi 0.32.1`), then `cargo test` → 33 passed, 0 failed.
- Decision: **bump kept** (safe now, per the "bump only if check passes"
  rule). `Cargo.toml` + `Cargo.lock` updated; two stale version strings
  updated with the bump (`core-rust/README.md` layout line,
  `predictor.rs` module doc). No API changes needed: the crate uses the
  UDL-less proc-macro surface (`uniffi::setup_scaffolding!("kbcore")`,
  `#[derive(uniffi::Object)]`, `#[uniffi::export]`,
  `#[derive(uniffi::Record)]` on `Suggestion`), which is unchanged
  0.29 -> 0.32, and there is no `build.rs`/`generate_scaffolding` call.
- Note for the Android owner: regenerate the Kotlin/JNI bindings with
  `uniffi-bindgen 0.32` — bindgen version must match the crate version or
  the generated symbols will mismatch `libkbcore.so`.

## 3. ID mapping (report only — no `android/` edits)

Recommended canonical pack/category IDs (also in `packs/README.md`):

`words, ne, js, rust, html, emoji, numbers, math`

| Canonical | `packs/<file>.json` (`id`) | `android/.../assets/categories/<file>.json` (`id`) | Status |
|---|---|---|---|
| `words` | `words_en.json` (`en`) | `words.json` (`words`, display `EN`) | **MISMATCH** packs side; rename pack id `en`->`words` (coordinated; affects `pack.rs` default `cat`) |
| `ne` | `nepali.json` (`ne`) | `nepali.json` (`nepali`, display `NE`, `langs:[ne]`) | **MISMATCH** Android side; rename manifest to `ne.json` / `"id":"ne"`, keep `displayName: NE` |
| `js` | `code_js.json` (`js`) | `js.json` (`js`) | agree |
| `rust` | `code_rust.json` (`rust`) | `rust.json` (`rust`) | agree |
| `html` | `code_html.json` (`html`) | `html.json` (`html`) | agree |
| `emoji` | `emoji.json` (`emoji`) | `emoji.json` (`emoji`) | agree |
| `numbers` | `numbers.json` (`numbers`) | `numbers.json` (`numbers`, display `123`) | agree |
| `math` | `math.json` (`math`) | `math.json` (`math`) | agree |

SPEC deltas (informational, in `packs/README.md` too):

- Personal cap: code `personal::PERSONAL_CAP` = **10k** (LFU, tombstones
  exempt) vs SPEC section 4 = **20k** LRU. Decide: bump code or amend SPEC.
- Hide threshold: code `RankWeights::hide_threshold` = **-0.5**, matching
  SPEC section 2. No action.

## 4. Remaining wiring

### A. core-bridge `Predictor` parity (Android owner)

`android/core-bridge/.../Predictor.kt` is still a stub (`StubPredictor`,
in-memory learned list, `suggest` returns learned words with score 0.0,
category `"words"`). To reach parity with the Rust UniFFI `Predictor`
(`new(base_json)`, `suggest(ctx, digits, active_tab, limit)`,
`learn(word, category)`, `forget(word)`, `export_session()`):

1. Generate real bindings from `kbcore` with `uniffi-bindgen 0.32` (Kotlin)
   and replace `StubPredictor` with the generated `Predictor`.
2. Signature gaps to close: stub `suggest(seq, prev, limit)` drops
   `active_tab` (category boost) and returns no `seq`/`score`; stub
   `learn(word, prev)` conflates ctx-prev with category; stub has no
   `forget` (block tombstone), no `reject` path (`record_reject` exists in
   core but is not UniFFI-exposed), no `placement()` (long-press popup),
   no `export/import` wiring.
3. Then wire: base wordlist JSON into `Predictor.new`, active-tab string
   from `CategoryTabs` (using the canonical IDs above), `limit` 3/30
   (bar vs expand-all), `learn`/`reject` on accept/delete-within-5s,
   `Dispatchers.Default` already correct.

### B. File-sync snapshot/WAL (SPEC section 5; core side)

Present in core: `redb` persistence for personal entries + bigrams
(`save_to_redb` / `load_from_redb`), session event log with JSONL
export/import (`export_jsonl` / `import_jsonl`), personal JSONL
(`to_jsonl` / `from_jsonl`).

Still missing vs SPEC:

1. Sync file set: `personal.jsonl`, `bigrams.jsonl`, `snapshot-<ts>.gz`,
   `wal.log` — no snapshot/WAL writer, compactor, or reader yet.
2. Merge rule: last-write-wins per word-id with element-wise max-merge of
   monotonic counters + tombstone-wins-on-newest-`ts` + single-device HLC
   (`ts` + deviceId tiebreak) — not implemented.
3. Auto-recovery: corrupt-`personal.jsonl` -> newest snapshot + replay WAL
   -> empty personal with base functional + notify — not implemented.
4. Write path discipline: append-to-WAL then hourly batch-compact; 2s
   background flush thread (SPEC puts the logging thread in core, Android
   only schedules via WorkManager) — not implemented.

Suggested next core tasks: `sync.rs` (snapshot read/write `.gz`, WAL
append/replay, max-merge, recovery) + UniFFI surface for
`export/import` of personal + bigrams + a `reject(word)` export.

## 5. Files changed/added (this report)

- Edited: `core-rust/Cargo.toml` (uniffi 0.29->0.32), `core-rust/Cargo.lock`
  (0.32.1 tree), `core-rust/README.md` + `core-rust/src/predictor.rs`
  (version strings only).
- Added: `kbpack.schema.json`, `packs/README.md`, `README.md`,
  `.gitignore` (`target/`, `sessions/`, `*.gz`, `.gradle/`),
  `INTEGRATION_REPORT.md` (this file).

---

## 6. Test-Lean pass (append-only, 2026-09-12; scope `core-rust/`, `packs/`, root)

### 6.1 Test results (33/33 green, flake fixed)

- `cargo test` in `core-rust/`: **33 passed, 0 failed** (5 consecutive full
  runs green; unit-test exec ~0.18–0.23s,_incremental wall ~0.34s).
- `cargo check --locked` (UniFFI 0.32.1 tree): **green** (~0.15–0.18s wall).
- Flaky `personal::tests::lfu_cap_evicts_coldest`: **fixed in
  `core-rust/src/personal.rs`** (only production-code change this pass).
  Root cause as previously noted: all `learn` calls in the test land in the
  same wall-clock second, so the LFU victim tie-break on
  `(count, last_seen)` fell through to `HashMap` iteration order and could
  evict the just-added word `d` itself. Fix: `evict_if_needed` now takes the
  just-learned key as an exclusion (a new word never evicts itself to make
  room for itself) and breaks residual ties deterministically by key instead
  of hash order. Verified: targeted test 30/30 green + 5x full-suite 33/33.
- Packs validation through the real loader (transient integration test
  `tests/tmp_lean_check.rs`, run then deleted — no temp files left):
  **8/8 packs load** via `pack::load_pack`, entries == words for every pack,
  every entry encodes to a non-empty `seq` (explicit `seq` overrides in
  emoji/math, `mapping::encode_word` elsewhere). Total **225/225**.
- Suggest-path latency (same transient test, debug build, all 8 packs
  loaded, 2000x `suggest("", "43556", "EN", 5)`): total 37ms,
  **avg ~19µs/suggest** — ~500x headroom under the 10ms p95 budget.

### 6.2 Lean audit

- **Deps (`core-rust/Cargo.toml`)**: `fst 0.4` (used: `stack.rs`
  sequence-index `SetBuilder`/`Set` + prefix stream), `serde` /
  `serde_json` (used: pack/personal/session ser/de), `redb 2`
  (used: `personal.rs` `save_to_redb` / `load_from_redb`),
  `uniffi 0.32` (used: `predictor.rs` proc-macro export surface).
  `lru`, `postcard`, `memmap2` are **not depended on and not used** —
  no removal needed; dependency surface is already minimal. Full
  transitive tree is uniffi-build tooling + `fst`/`redb`/`serde` only
  (see `Cargo.lock`: no `lru`/`postcard`/`memmap` entries).
- **cdylib size** (debug, macOS): `target/debug/libkbcore.dylib` **1.7M**,
  `libkbcore.rlib` 7.2M. No release-size note taken (no `release` profile
  build run; Linux `.so` size for the Android `libkbcore.so` ship vehicle
  still to be measured by the Android owner at bindgen time).
- **mmap vs heap (fst packs)**: current `rebuild_index` builds the FST
  set in heap memory (`fst::SetBuilder::memory`) on every
  `new`/`add_pack`. Fine at 225-word showcase scale (µs suggest path),
  but index rebuild is O(entries) per pack add and the bytes are
  heap-resident, not mmapped. If packs grow toward 10k+ words: build the
  FST once from a sorted `map` dump and serve it via `fst::Set::new(mmap)`
  (`memmap2`, currently *not* a dep — add only then), keeping the
  in-heap builder as the small-pack default.
- **redb single-writer + cache**: current persistence opens a fresh
  `redb::Database` per `save_`/`load_` call (whole-table JSON blob under
  keys `"entries"`/`"bigrams"`); there is no long-lived handle, no
  single-writer discipline, no 512–2048-entry LRU in front, and no
  background WAL flush thread (2s or otherwise) — correctly listed as
  missing under §4B. Suggested shape when `sync.rs` lands: one
  long-lived `Database` handle behind the existing `Mutex<Inner>`
  (single writer = the core logging thread), an in-memory LRU
  (512–2048 personal entries) in front of reads, WAL-append + 2s
  background flush with hourly batch-compact per SPEC §5.
- **Suggest path vs <10ms p95**: measured ~19µs avg in debug on the
  full 225-word stack; path is `HashMap` union → top-200 freq
  pre-truncate → rank → top-N sort, all in-memory with no I/O. Budget
  holds with large margin; revisit only if personal dict approaches the
  10k LFU cap with heavy bigram history.
- **UniFFI narrow FFI**: surface is already narrow —
  `Predictor::new(base_json)`, `suggest(ctx, digits, active_tab, limit)`,
  `learn(word, category)`, `forget(word)`, `export_session()`, plus the
  `Suggestion` record. Strings cross the boundary per call today; keep
  batch-string discipline (JSON wordlist in, JSONL session out) rather
  than per-word FFI chatter, and expose `reject(word)` (existing
  `record_reject` is core-internal, not exported) when the delete path
  is wired (§6.4).

### 6.3 Packs / schema / canonical IDs (report only — no `android/` edits)

- `kbpack.schema.json` **covers all 12 SPEC §3 manifest fields**
  (`id`, `version`, `kind`, `name`, `license`, `paid`, `langs`,
  `categories`, `priority`, `dict`, `tokenizer`, `minCore` — verified
  field-for-field against the SPEC §3 `kbpack.json` example). Note:
  the schema validates `.kbpack` manifests, *not* the dev-time
  `packs/*.json` wordlists (`{id, title, version, words[]}`), which load
  via `pack::load_pack` — different format, no schema gap.
- Canonical IDs confirmed per `packs/README.md`:
  `words, ne, js, rust, html, emoji, numbers, math`.
- Mismatches vs `android/ime/.../assets/categories/` (read-only check,
  **not edited**): `words_en.json` (`id: "en"`) vs `words.json`
  (`id: "words"`, display `EN`) — **MISMATCH, packs side**; `nepali.json`
  (`id: "ne"`) vs `nepali.json` (`id: "nepali"`, display `NE`,
  `langs: [ne]`) — **MISMATCH, Android side**. Remaining six
  (`js`, `rust`, `html`, `emoji`, `numbers`, `math`) agree.
- `en`/`words` default-cat alignment: **not renamed this pass**
  (deliberate). `pack.rs` defaults missing `cat` to the pack id, so
  English-base words carry `cat "en"` while `stack.rs`/`predictor.rs`
  default to `"EN"` and `cat_boost` compares exact strings — the rename
  `en`->`words` also needs the Android tab-label side to move in lockstep
  (other agent owns `android/`). Left as the coordinated rename already
  recorded in §3 / `packs/README.md`; no `pack.rs`-side half-fix applied.

### 6.4 Fixes applied this pass (minimal diff)

- `core-rust/src/personal.rs` only: LFU victim exclusion for the
  just-learned key + deterministic key tie-break (`k.clone()` on the
  insert path to satisfy the borrow). Tests green (33/33 x5, flake 30/30).
- No pack renames, no `core-rust/` rename, no `android/` or `docs/` edits.

### 6.5 Remaining (owners as noted)

1. **core-bridge 0.32 regen** (Android owner): regenerate Kotlin/JNI
   bindings with `uniffi-bindgen 0.32` and replace `StubPredictor`
   (`suggest(seq, prev, limit)` / `learn(word, prev)`) with the real
   UniFFI `Predictor` (§4A signature gaps unchanged).
2. **Snapshot/WAL** (core side): `sync.rs` per §4B — snapshot `.gz` +
   WAL append/replay, max-merge + tombstone-wins + HLC, recovery,
   WAL-append + 2s background flush / hourly compact.
3. **`del` vs `deleted` call** (Android owner, core already ready):
   the IME delete path (user deletes a committed word) has no bridge
   call today — `Predictor`/`StubPredictor` expose no
   `forget`/`del`/`reject` method, while core has `forget(word)`
   (`deleted:true` tombstone) and `record_reject` (not UniFFI-exposed).
   Wire delete-within-5s to a new UniFFI `reject(word)` export +
   `forget` on explicit block, and align the method name (`del` vs
   `forget`) with the bridge interface when it is cut.
