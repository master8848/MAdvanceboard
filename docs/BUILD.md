# Build — Toolchains, Recipes, Troubleshooting

## 1. Prerequisites

| Tool | Version | Source |
|---|---|---|
| `mise` | any recent | `mise.toml` header: `just bootstrap` = `mise trust` + `mise install` |
| Rust | stable | `mise.toml` `rust = "stable"`; `core-rust` edition 2021, crate `kbcore` |
| JDK | 17 | `mise.toml` `java = "17"`; AGP/Gradle toolchain requires 17; `JAVA_HOME` comes from mise's java backend |
| Gradle | 9.6.0 | `mise.toml` + `android/gradle/wrapper/gradle-wrapper.properties`; `./gradlew` is the fallback |
| Android NDK | 28 | required for `libkbcore.so` cross-compilation (UniFFI cdylib → JNI); install via SDK manager, then point `ANDROID_NDK_HOME` at it |
| Python | 3.12 | `mise.toml`; used by `just validate-packs` (stdlib `json` only) and `scripts/build_pack.py`-style pack tooling |
| `just` | latest | `mise.toml`; recipes in `justfile` (`just --list`) |
| `bun` | latest | convenience only — no JS in repo yet |

Android stack (verified statically in `android/QA_REPORT.md` §1): AGP 9.4.0,
Kotlin 2.2.20 (KSP 2.2.20-2.0.2), compileSdk/targetSdk 36, minSdk 26, Compose
BOM 2026.09.00, Java 17 in all 5 modules.

## 2. Just recipes

| Recipe | Command | What it does |
|---|---|---|
| `just bootstrap` | `mise trust` + `mise install` | install all pinned toolchains |
| `just test-core` | `cargo test -p kbcore --manifest-path core-rust/Cargo.toml` | Rust engine tests (33 green) |
| `just check-core` | `cargo check` + `cargo clippy` | check + lint; clippy warnings non-blocking |
| `just validate-packs` | `python3 -c "import glob, json; …"` | `json.load` every `packs/*.json` + `kbpack.schema.json`; prints `VALID N files` |
| `just docs-check` | fence balance + link surface | odd-fence fail; lists `](…)` link targets for manual review |
| `just android-assemble` | `cd android && ./gradlew assembleDebug` | debug APK/AAR build; **needs JDK 17** (`JAVA_HOME` set) |
| `just sync-report` | `git status --short` | read-only snapshot; makes no changes |

`mise.toml` also exposes `mise run test-core` / `mise run validate-packs`
shortcuts for `just test-core` / `just validate-packs`.

## 3. Android assemble + APK path

```sh
export JAVA_HOME=<jdk17>   # via mise: mise install, re-exec shell with mise activated
cd android && ./gradlew assembleDebug
# or: just android-assemble
```

Useful targets: `./gradlew :app:assembleDebug :ime:assembleDebug` (+ `:sync:assembleDebug`
for the sync module), `./gradlew lint` (esp. `NewApi` on the API-28+
`switchToNextInputMethod` call), `:sync:kspDebugKotlin` for the Room/KSP
check (`android/QA_REPORT.md` §Remaining; `android/FIX_REPORT.md`
§Known remaining). Outputs (standard Gradle locations, gitignored): 
`android/app/build/outputs/apk/debug/app-debug.apk`,
`android/ime/build/outputs/aar/`. The IME runs without `libkbcore.so`
(`KbCore.loaded == false` → `StubPredictor`); shipping the real engine
requires building the Rust cdylib for Android targets and the bindgen swap
(§4). Enable the IME on-device via the Settings/onboarding wizard, then type
in plain vs password fields to confirm the learn gate.

## 4. Core tests

```sh
just test-core
# or: cd core-rust && cargo test
```

Covers mapping (`hello`→`43556`, Devanagari collapse, 1-edit neighbors),
ranking (exact > neighbor, personal boost, reject penalty, recency, freq
ordering), stack (exact/fuzzy/category-boost/priority/personal
promote+block/FST prefix), personal (learn, password-mode bypass, tombstones,
LFU cap, redb + JSONL roundtrips), session (export/import, garbage
rejection), pack loader (encode, `seq` override, invalid reject), predictor
roundtrip — 33 passed, 0 failed (`INTEGRATION_REPORT.md` §1). Suggest-path
spot check from the lean pass: ~19µs avg (debug, all 8 packs, 2000×
`suggest("", "43556", "EN", 5)`) vs the 10 ms p95 budget.

## 5. Pack validation

```sh
just validate-packs
```

`json.load`s all 8 `packs/*.json` (`words_en`, `nepali`, `numbers`,
`code_js`, `code_rust`, `code_html`, `emoji`, `math` — 225 words total) plus
`kbpack.schema.json`. Deeper check (loader, not just JSON): `pack::load_pack`
transient test used in `INTEGRATION_REPORT.md` §§1, 6 — entries == words per
pack, every entry encodes to non-empty `seq`. Manifest schema
(`kbpack.schema.json`) validates `.kbpack` `kbpack.json` files (12 SPEC §3
fields), **not** the dev-time `packs/*.json` wordlists
(`{id, title, version, words[]}`) — different format, no gap. Authoring guide:
`docs/EXTENSIONS.md`; counts + seq examples: `packs/README.md`.

## 6. Troubleshooting

- **No-JDK env (this sandbox).** `android-assemble` fails without JDK 17 —
  expected. Verify `JAVA_HOME` is set (`mise install`, re-exec shell with mise
  activated); until then use `just test-core` + `just validate-packs` +
  static `grep` checks (`grep -rn INTERNET android/ime/ android/app/src` →
  no hits for BIND-only). All Android reports to date are static-only for this
  reason (`android/QA_REPORT.md` header; `android/FIX_REPORT.md` §Known remaining).
- **`uniffi-bindgen 0.32` regen (LANDED).** Bindgen version must match the
  crate (`uniffi 0.32.1` in `core-rust/Cargo.toml` + `Cargo.lock`).
  Regen ran with `uniffi-bindgen 0.32.1`
  (`uniffi-bindgen generate --library target/debug/libkbcore.dylib
  --language kotlin` from `core-rust/`); output vendored at
  `android/core-bridge/src/main/java/uniffi/kbcore/kbcore.kt`.
  Reconciliation: `reject(word)` / `reject_with_shown` ARE UniFFI-exported
  (`predictor.rs:314,320`); `record_reject` is the core-internal personal
  op they delegate to — no core change was needed, only this doc line.
  `UniFfiPredictor` (backed by the generated `uniffi.kbcore.Predictor`) is
  the default path; `StubPredictor` is degraded-only with a user-visible
  message. Active-tab ids + limit 3/30 + base wordlist JSON wired via
  `PredictorFactory` (`FINAL_RECONCILE.md` §§2, 5). Re-regen after ANY
  `#[uniffi::export]` change, or symbols mismatch `libkbcore.so`.
- **Stable-shim note.** The repo pins `mise.toml` `rust = "stable"` while
  `core-rust` documents `uniffi 0.32 build feature` compatibility
  (`core-rust/README.md` §Layout). If a future stable toolchain breaks the
  0.32 proc-macro surface, pin an exact `rust = "1.x"` in `mise.toml` and
  re-run `cargo check --locked` (green at ~0.15–0.18s wall per
  `INTEGRATION_REPORT.md` §6.1) before touching `Cargo.toml`.
- **`stable-shim` / cdylib size.** Debug `target/debug/libkbcore.dylib` is
  ~1.7M on macOS; the Linux `libkbcore.so` for Android is still unmeasured —
  measure at bindgen time (`INTEGRATION_REPORT.md` §6.2).
- **Corrupt personal dict on-device.** Recovery order: restore newest
  `snapshot-<ts>.gz`, replay `wal.log`; if both corrupt, start empty personal
  (base packs keep working) + notify user (`docs/SYNC.md` §5).
