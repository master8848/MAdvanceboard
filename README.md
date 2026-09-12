# 9-Key Predictive Keyboard (`kb9`)

Research keyboard: 9-key T9 pad (3×3) with a shared prediction engine,
category-filtered dictionaries, QWERTY fallback that teaches the personal
dict, and open sideloadable vocab packs. See `SPEC.md` (normative),
`docs/INTENT.md` (research framing), `docs/USER.md` (behavior).

How it works in one paragraph: each keypress emits a digit `2–9`; the
encoder produces a `seq` (e.g. `hello`→`43556`) looked up across the
dictionary stack with 1-edit fuzzy match; the ranking formula scores
candidates using base/personal frequency, personal bigrams, recency,
category boost, key-fit, and reject penalty; the top-3 go to the
suggestion bar, top-30 to expand-all. QWERTY mode shares the same engine —
only the key→sequence encoder changes (`SPEC.md` §0).

## Monorepo map

| Path | Contents |
|---|---|
| `SPEC.md` | Normative spec (layout, T9 map, ranking, packs, sync, milestones). Read-only history. |
| `android/` | Kotlin IME (`ime/`), Settings `app/`, `plugin-api/`, `core-bridge/`, `sync/`. See `android/README.md`. |
| `core-rust/` | `kbcore` engine crate (mapping, FST stack, rank, personal, session, pack loader, UniFFI `Predictor`). See `core-rust/README.md`. |
| `packs/` | 8 showcase vocab sources (`words_en`, `nepali`, `numbers`, `code_js`, `code_rust`, `code_html`, `emoji`, `math`). See `packs/README.md`. |
| `docs/` | `INTENT / USER / PRIVACY / EXTENSIONS / SYNC / TUNING / TEST_REPORT` + `ENGINEERING.md` + `BUILD.md` (this set). |
| `kbpack.schema.json` | JSON Schema for `.kbpack` manifests (`kbpack.json`). |
| `mise.toml` / `justfile` | Pinned toolchains + task recipes. |
| `INTEGRATION_REPORT.md` / `FINAL_RECONCILE.md` | Engine + cross-module status reports (read-only history). |
| `android/QA_REPORT.md`, `android/FIX_REPORT.md`, `android/LEAN_QWERTY_AUDIT.md` | Android static QA / fixes / lean audit (read-only history). |
| `scripts/build_pack.py` | Pack tooling: build / add-word / expand-pack / validate / stats for `packs/*.json`. |

Note: crate dir is `core-rust/` (crate `kbcore`); `SPEC.md` §6 sketches it
as `core/` — rename deferred.

## Canonical category IDs

`words, ne, js, rust, html, emoji, numbers, math`

Tab display labels stay `EN` / `NE` / `123` per `SPEC.md` §0; engine and
manifest ids are lowercase. Reconciled 2026-09-12 (`FINAL_RECONCILE.md` §1):
`packs/words_en.json` id `en`→`words`; `android/…/assets/categories/nepali.json`
→ `ne.json` id `ne` (compat mapping for legacy `EN`/`NE`/`nepali` kept in
`KbInputMethodService.onCategoryChanged`). `packs/README.md` §Canonical IDs
is the pack-side reference. Known deferred drift: `nepali.json` rows still
carry display `cat "NE"` and `stack.rs` / `predictor.rs` default to `"EN"` —
normalize together later (`cat_boost` is exact-match).

## Quickstart

```sh
mise install          # or: just bootstrap  (mise trust + install)
just test-core        # cargo test -p kbcore (33 unit tests, engine green)
just validate-packs   # json.load all packs/*.json + kbpack.schema.json
just android-assemble # cd android && ./gradlew assembleDebug (needs JDK 17)
just check-core       # cargo check + clippy (warnings non-blocking)
just docs-check       # fence balance + link surface, manual review
```

Toolchains are pinned in `mise.toml` (rust stable, java 17, gradle 9.6.0,
python 3.12, just, bun). Full prerequisites + troubleshooting: `docs/BUILD.md`.
Architecture + contracts: `docs/ENGINEERING.md`.

## Dictionary add / expand

- Per-word lookup: each row is `{w, freq?, cat?, lang?, seq?}`; `cat`
  defaults to pack id, `lang` to `"en"`, `freq` to `100`; `seq` override is
  required only for non-encodable words (emoji, `\alpha`). Loader:
  `core-rust/src/pack.rs` (`pack::load_pack`), map: `core-rust/src/mapping.rs`.
- Showcase examples: `fun`→`386` (js), `hea`→`432` (emoji), `alp`→`257`
  (math) — see `packs/README.md` §§Sequence examples, Canonical IDs.
- Authoring a real `.kbpack` (zip with `kbpack.json` + `dict.jsonl.gz` +
  optional WASM tokenizer) and sideload policy (SHA-256 shown, unsigned
  allowed with warning, `paid` informational-only): `docs/EXTENSIONS.md`,
  schema `kbpack.schema.json`.
- Bulk add/expand: `python3 scripts/build_pack.py add-word --pack
  packs/words_en.json --word serendipity`, `expand-pack --pack <pack> --in
  <wordlist>`, then `validate` / `stats` (see script header for all
  subcommands).
- Scoring/weights/thresholds when adding words: `docs/TUNING.md`
  (hide `S < -0.5`, OOV ≥2 accepts/7d → personal, delete-within-5s → reject).

## Sync / privacy one-liner

No accounts, no server, no `INTERNET` in the IME: sync is opt-in file-based
JSONL (`personal.jsonl`, `bigrams.jsonl`, `snapshot-<ts>.gz`, `wal.log`,
LWW + max-merge, tombstone `del` wins on newest `ts`) and nothing typed in
password fields or under `privacy.learn: false` packs (`numbers`, `math`)
ever touches learning — see `docs/SYNC.md`, `docs/PRIVACY.md`.

## Status + reports index

- Engine: `cargo test` 33/33 green; 8/8 packs load, 225/225 words encode
  (`INTEGRATION_REPORT.md` §§1, 6; `FINAL_RECONCILE.md` §4).
- Suggest-path latency ~19µs avg (debug, full stack), ~500x headroom under
  the 10 ms p95 budget (`INTEGRATION_REPORT.md` §6.1).
- Android: static QA PASS, P0/P1 fixes applied, lean audit done — all static
  only, no JDK build in CI env
  (`android/QA_REPORT.md`, `android/FIX_REPORT.md`, `android/LEAN_QWERTY_AUDIT.md`).
- Still owed: core `sync.rs` (snapshot/WAL/merge/recovery, `reject(word)`
  UniFFI export), `uniffi-bindgen 0.32` regen replacing `StubPredictor`,
  on-device assemble + smoke test (`FINAL_RECONCILE.md` §5).

## JDK / APK note

`just android-assemble` requires JDK 17 (`JAVA_HOME` set, e.g. via
`mise install`); without it Gradle does not run — expected in no-JDK envs,
use `just test-core` / `just validate-packs` instead. The IME runs without
`libkbcore.so` via `StubPredictor` (`KbCore.loaded == false`); the real
engine needs the `uniffi-bindgen 0.32` binding swap first. APK output lands
under `android/app/build/outputs/apk/debug/` and
`android/ime/build/outputs/aar/` (standard Gradle locations, not checked in).

## License

MAdvanceboard — 9-key predictive keyboard (just like Gboard).
Copyright (C) 2026 Saurav Sanjel.

Licensed under **GPL-3.0-only** — see [`LICENSE`](LICENSE). No warranty.
Copying and selling as closed-source is not permitted: any distribution
must share full source under the same license.
