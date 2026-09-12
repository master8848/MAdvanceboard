# Engineering — `kb9` Architecture

Normative spec: `SPEC.md`. Research framing: `docs/INTENT.md`. Build/run:
`docs/BUILD.md`. Pack authoring: `docs/EXTENSIONS.md`. Sync file format:
`docs/SYNC.md`. Weights: `docs/TUNING.md`. Privacy: `docs/PRIVACY.md`.

## 1. Architecture

- **Rust core (`core-rust/`, crate `kbcore`).** Android-free engine:
  `src/mapping.rs` (layout-driven encode via `LayoutSpec`, t9-9 default;
  Devanagari collapse, 1-edit adjacency),
  `src/stack.rs` (dictionary stack + `fst` sequence-index `SetBuilder`/`Set`
  with prefix stream), `src/rank.rs` (scoring + `RankWeights`), `src/personal.rs`
  (personal overlay: counts, `last_seen`, LFU 10k cap, `redb` persistence via
  `save_to_redb` / `load_from_redb`, JSONL roundtrips), `src/session.rs`
  (accepted/rejected event log, `export_jsonl` / `import_jsonl`), `src/pack.rs`
  (MVP `packs/*.json` loader), `src/predictor.rs` (UniFFI `Predictor` object).
  Deps (`core-rust/Cargo.toml`): `fst 0.4`, `serde` / `serde_json`, `redb 2`,
  `uniffi 0.32` — minimal surface, no `lru`/`postcard`/`memmap2`
  (`INTEGRATION_REPORT.md` §6.2).
- **FFI (`predictor.rs`, UniFFI 0.32, narrow).** UDL-less proc-macro surface
  (`setup_scaffolding!`, `#[derive(uniffi::Object)]`, `#[uniffi::export]`,
  `Suggestion` record (`word`, `score`, `seq`, `cat`, `layout_id`).
  Methods: `new(base_json)`, `suggest(ctx, digits, active_tab, limit)`,
  `suggest_with_layout` / `suggest_for_cat`, `learn(word, category)` /
  `learn_with_shown`, `forget(word)`, `reject(word)` / `reject_with_shown`
  (delegating to core-internal `record_reject`), `export_session()`, plus
  layout (`encode` / `decode` / `layouts` / cat-map) and persist
  (`open_persist` / `flush_persist` / `compact_persist`) passthroughs.
  Batch-string discipline: JSON wordlist in, JSONL session out — no per-word
  FFI chatter; one `suggest` call per keystroke. Bumped 0.29→0.32, build
  green; Kotlin bindings regenerated with `uniffi-bindgen 0.32.1`
  (`android/core-bridge/src/main/java/uniffi/kbcore/kbcore.kt`) — bindgen
  version must match the crate or symbols mismatch
  (`INTEGRATION_REPORT.md` §2; `FINAL_RECONCILE.md` §5).
- **Android IME (hybrid Views + Compose).** `android/ime/`:
  `KbInputMethodService` (lifecycle, `InputMode` TEXT/NUMBER/PHONE/DATETIME,
  `shouldLearnNow()` gate, per-keystroke `suggest`, `learn` on commit),
  9-key `KeyPadView` (default board) + full QWERTY `QwertyView` (shared Predictor path, only
  the encoder differs), Compose `SuggestionStrip` (`LazyRow`, 3 inline),
  category `TabRow`, expand-all list. Manifests in
  `ime/src/main/assets/categories/*.json`. Stack: AGP 9.4.0, Gradle 9.6.0,
  JDK 17, Kotlin 2.2.20, compileSdk/targetSdk 36, minSdk 26, Compose BOM
  2026.09.00 (`android/QA_REPORT.md` §1).
- **Module split.** `plugin-api` (`CategoryProvider`, `SuggestionQuery` /
  `SuggestionItem` / `CommitContext`, `CategoryRegistry` JSON loader) —
  no sync deps. `core-bridge` (`System.loadLibrary("kbcore")` probe +
  `Predictor` interface: `new`/`suggest`/`learn`/`forget`/`reject`/`exportSession`
  on `Dispatchers.Default`, no Android `Context`; `StubPredictor` in-memory
  fallback with 20k bound) — no Android imports. `sync` (Room + DataStore
  stubs, `WorkManager` 24h periodic, LWW per-word merge, JSONL export/import,
  opt-in off by default) — runs in `:app` process only. `:ime → :sync`
  dependency removed; `:app → :sync` kept (`android/FIX_REPORT.md` §§P0–P1).
  `app` holds Settings + onboarding enable-IME wizard.
- **Dictionary stack.** `base EN/NE` (prio 0) < extension packs (10–90,
  user-ordered) < personal (100). Lookup unions enabled packs + personal,
  dedupes on `(norm(word), lang)`; display keeps highest-prio pack's `cat`;
  frequencies sum with personal weighted separately (`SPEC.md` §4;
  `docs/TUNING.md` §2; `core-rust/src/stack.rs`).
- **Ranking formula** (`core-rust/src/rank.rs`, weights in
  `RankWeights::default()`, tunable in Settings → Tuning):

```text
S(w) = 1.0*log10(freq_base + freq_personal + 1)
     + 1.2*P_personal(w) + 0.8*Bigram(ctx_prev, w) + 0.5*Recency(w)
     + 0.3*CategoryBoost(w) + 0.2*KeyFit(seq, w) - 1.5*RejectPenalty(w)
```

  `Bigram` is personal-only log-prob with backoff 0; `Recency` is
  `exp(-dt/7d)`; `CategoryBoost` 1.0 on active tab / 0.5 on ★; `KeyFit` 1.0
  exact, 0.9 prefix (core addition — SPEC lists 1.0/0.6), 0.6 neighbor;
  `RejectPenalty` is `rej/(acc+rej+1)` decayed 30d. Hide `S < -0.5` unless
  expand-all; tie-break shorter → lexicographic → pack priority; matches
  capped at 200, top-30 heap (`SPEC.md` §2; `docs/TUNING.md` §§1–2).
- **T9 + fuzzy.** Keys `2–9` Latin T9 preserved; Devanagari initials collapse
  phonetically onto the same keys so one `seq` matches both scripts; key `1`
  is punct-commit (breaks word); `numbers` tab is digit-commit, no prediction;
  long-press inserts the digit. Lookup is exact + 1-edit neighbor (cost 0.4);
  matras normalize to base vowels (`कि`→`29`); code/emoji/math reuse seq
  (`fun`→`386`, `hea`→`432`, `alp`→`257`) (`SPEC.md` §1; `core-rust/README.md`
  §§Key mapping, Extension packs).
- **Learning / WAL (current vs target).** Current: accept bumps
  `freq_personal` + bigram + recency; reject (delete-within-5s or pick-other)
  bumps `rej`; OOV committed ≥2×/7d promotes to personal; `freq_personal`
  decays ×0.98 monthly; persistence is whole-table JSON blobs under redb keys
  `entries`/`bigrams` opened fresh per call. Target per `SPEC.md` §§2,5: 2s
  background batch flush (core logging thread, Android only schedules via
  WorkManager), append-to-WAL + hourly compact. No flush thread, no LRU, no
  long-lived DB handle yet (`INTEGRATION_REPORT.md` §§4B, 6.2).
- **Sync (JSONL LWW).** Row: `{w, lang, cat, seq, freq, acc, rej, ts, del,
  id}` with `id = "<lang>:<normalized-word>"`. Export `session-<date>.jsonl.gz`
  = personal delta + `log.jsonl` events; import replays the log through ranking
  deterministically. Folder layout: `personal.jsonl`, `bigrams.jsonl`,
  `snapshot-<ts>.gz`, `wal.log`. Merge: group by `id`, keep max `ts`,
  element-wise `max` on monotonic counters, tombstone wins iff newest `ts`
  (HLC `ts` + `deviceId`, no server clock). Recovery: corrupt personal →
  newest snapshot + replay WAL → empty personal + base functional + notify
  (`SPEC.md` §5; `docs/SYNC.md`; `core-rust/src/session.rs`).
- **Tombstones: `del` canonical + compat.** SPEC is self-inconsistent (§4
  `{deleted:true}` vs §5 `"del":false`); decision is **`del`** (SPEC §5, all
  of `docs/`). Readers accept legacy `deleted`: core via
  `#[serde(rename = "del", alias = "deleted")]` (Rust field `deleted`, old
  redb blobs still load, new writes emit `del`); Android
  `SyncMerge.fromJsonl` reads `del` else `deleted`, `toJsonl` emits `del`
  (`FINAL_RECONCILE.md` §3).
- **Lean budgets.** Suggest p95 <10 ms core budget (measured ~19µs avg debug,
  full 225-word stack — ~500x headroom); personal cap 10k LFU in code vs 20k
  LRU in SPEC (decision open); debug `libkbcore.dylib` 1.7M; IME keeps 48dp
  targets + view caching + BIND-only manifest; sync WorkManager 24h within the
  12–24h budget; release minify/shrink still off (`INTEGRATION_REPORT.md`
  §6.2; `docs/TUNING.md` §5; `android/LEAN_QWERTY_AUDIT.md` §4).

## 2. Module contracts

| Module | In | Out | Notes |
|---|---|---|---|
| `mapping` | keypress / word | `seq` digit string | `encode_word`, `char_to_digit`, neighbor set; file `core-rust/src/mapping.rs` |
| `pack` | `packs/*.json` path | `Pack` (words + explicit `seq` preserved) | `load_pack`; `cat`→pack id, `lang`→`en`, `freq`→100 defaults; rejects invalid input (`core-rust/src/pack.rs`) |
| `stack` | `seq` + ctx + active tab | ranked `Vec<Cand>` (cap 200 → heap top-30) | owns FST index (`rebuild_index` per pack add, heap-resident); prio + dedupe + category boost (`core-rust/src/stack.rs`) |
| `rank` | candidate + ctx + weights | `S(w)` score | `RankWeights::default()`; hide threshold `-0.5` (`core-rust/src/rank.rs`) |
| `personal` | accept / reject / block | overlay counts + tombstones | LFU 10k, tombstones exempt; redb + JSONL roundtrips (`core-rust/src/personal.rs`) |
| `session` | accept/reject events | `export_jsonl` / `import_jsonl` | deterministic replay (`core-rust/src/session.rs`) |
| `predictor` (UniFFI) | JSON wordlist; `(ctx, digits, active_tab, limit)` | `Suggestion` records; JSONL session | narrow FFI; `reject(word)` / `reject_with_shown` ARE exported (`core-rust/src/predictor.rs:314,320`); `record_reject` is the core-internal personal-overlay op they delegate to. Kotlin regen verified with `uniffi-bindgen 0.32.1` (see `android/core-bridge/src/main/java/uniffi/kbcore/kbcore.kt`) |
| `plugin-api` | category asset JSON | `CategoryProvider` entries | `opt*` parsing, forward-compatible (`android/plugin-api/…/CategoryRegistry.kt`) |
| `core-bridge` | suggest/learn/forget/reject/export calls | candidates / ack | `Dispatchers.Default`, no `Context`; stub until bindgen regen (`android/core-bridge/…/Predictor.kt`) |
| `sync` | local files / Room rows | merged personal table | LWW + tombstone-wins, 24h `WorkManager`, opt-in only (`android/sync/…/SyncWorker.kt`, `SyncMerge.kt`) |
| `kbpack.schema.json` | `.kbpack` manifest | validation verdict | covers all 12 SPEC §3 fields; does NOT validate dev-time `packs/*.json` wordlists (different format) |

## 3. Key decisions + alternatives considered

- **redb over rusqlite/SQLite** (`core-rust/README.md` §Storage choice):
  pure-Rust embedded ACID, no C toolchain → simple NDK cross-compilation;
  typed tables suffice for personal/bigram KV; JSONL stays the interchange
  format. Alternative (SQLite/Room on Android side only) kept for the `sync`
  module's single entity — flagged as disproportionate, keep only if
  FTS/paging proven needed (`android/LEAN_QWERTY_AUDIT.md` §4).
- **fst for the sequence index** (`stack.rs`): prefix-stream lookup fits T9
  `seq` matching; heap-built (`SetBuilder::memory`) at 225-word scale. If
  packs reach 10k+ words: dump sorted map once, serve via `fst::Set::new(mmap)`
  (`memmap2`, not currently a dep), keep heap builder as small-pack default
  (`INTEGRATION_REPORT.md` §6.2).
- **UniFFI 0.32 narrow FFI instead of hand-rolled JNI**: proc-macro surface,
  batch-string calls, generated Kotlin bindings. Cost: bindgen version must
  match crate exactly (0.32 ↔ 0.32.1); the real swap needs a JDK machine
  (`INTEGRATION_REPORT.md` §2).
- **`del` over `deleted`** for tombstones: follows SPEC §5 + all of `docs/`,
  with compat readers for the SPEC §4 legacy name (`FINAL_RECONCILE.md` §3).
- **Canonical ids `words/ne/…` with display labels `EN/NE/123`**: engine ids
  lowercase, tab labels per SPEC §0; legacy-label compat kept in
  `onCategoryChanged` rather than forcing a flag-day (`FINAL_RECONCILE.md` §1).
- **IME/sync process split**: `:ime → :sync` removed so the IME process is UI
  + bridge only; sync (Room/Work/DataStore) lives in `:app`. Biggest single
  lean win (`android/FIX_REPORT.md` §P1).

## 4. Risks / tech debt

1. **No LRU / WAL thread yet.** No long-lived redb handle (open-per-call),
   no 512–2048-entry LRU front, no 2s background flush / hourly compact, no
   `sync.rs` (snapshot `.gz` + WAL append/replay + max-merge + recovery).
   Shape when it lands: one `Database` behind the existing `Mutex<Inner>`
   (single writer = core logging thread) (`INTEGRATION_REPORT.md` §§4B, 6.2).
2. **Bridge regen landed (this change).** `uniffi-bindgen 0.32.1`
   output vendored at `android/core-bridge/.../uniffi/kbcore/kbcore.kt`;
   `Predictor` bridge interface mirrors the full export surface
   (`suggest_with_layout(ctx, digits, layout_id, active_tab, limit)`,
   `learn(word, category)` + `learn_with_shown`, `reject` +
   `reject_with_shown`, `forget`, `export_session`, layout/persist
   passthroughs; `ScoredCandidate` carries `seq`/`score`/`cat`/`layout_id`).
   `UniFfiPredictor` is the default path; `StubPredictor` remains ONLY as the
   explicit degraded path when `libkbcore.so` is absent (`KbCore.isAvailable()
   == false` surfaces in the IME status line, never silently).
   Delete-within-5s → `reject`, explicit block → `forget`, limit 3/30,
   base wordlist JSON via `PredictorFactory` (`INTEGRATION_REPORT.md` §4A;
   `FINAL_RECONCILE.md` §§2, 5). Still open: `placement()` has no core
   UniFFI export (stack-only, `stack.rs:1521`) — long-press popup stays
   local until a core export lands.
3. **JDK build pending.** All Android verification so far is static (no JDK in
   env): `assembleDebug` + lint (`NewApi` on the API-28+ IME-switch call) +
   on-device smoke test still owed (`android/FIX_REPORT.md` §Known remaining).
4. **Cap + cat-case drift.** Personal cap 10k LFU (code) vs 20k LRU (SPEC §4);
   row cats `"NE"`/default `"EN"` vs canonical `ne`/`words` (`cat_boost` is
   exact-match — normalize in one move) (`packs/README.md` §§Display note,
   SPEC deltas; `FINAL_RECONCILE.md` §1).
5. **Flake fixed, pattern noted.** `lfu_cap_evicts_coldest` same-second
   `last_seen` tie fell through to `HashMap` order; fixed by excluding the
   just-learned key + deterministic key tie-break. Same-second-timestamp tests
   remain a smell to watch (`INTEGRATION_REPORT.md` §6.1).
