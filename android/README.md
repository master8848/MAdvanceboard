# Android IME shell

Kotlin Android shell for the 9-key predictive keyboard. Toolchain: AGP 9.4.0,
Gradle 9.6.0, JDK 17, Kotlin 2.2.20, compileSdk/targetSdk 36, minSdk 26,
Compose BOM 2026.09.00.

## Modules

- `app` — Settings Activity + onboarding enable-IME wizard.
- `ime` — `InputMethodService` skeleton, 9-key `KeyPadView`, Compose suggestion
  strip (`LazyRow`), category `TabRow`, expand-all fullscreen `LazyColumn`,
  QWERTY fallback toggle. Manifests in `ime/src/main/assets/categories/`.
- `plugin-api` — `CategoryProvider` interface, `SuggestionQuery` /
  `SuggestionItem` / `CommitContext` models, `CategoryRegistry` JSON loader.
- `core-bridge` — `System.loadLibrary("kbcore")` stub + `Predictor` interface
  (`new` / `suggest` / `learn` / `forget` / `reject` / `exportSession` on
  `Dispatchers.Default`, no Android Context; `forget`/`exportSession` mirror
  the Rust UniFFI surface, `reject` awaits a core export + `uniffi-bindgen`
  regen — `StubPredictor` covers all six in memory until then).
- `sync` — Room + DataStore stubs, `WorkManager` 24h periodic work (within the
  12–24h budget), LWW per-word merge, JSONL export/import, opt-in only.

## Build

```sh
cd android
./gradlew assembleDebug
```

Requires JDK 17. The Gradle wrapper points at Gradle 9.6.0. The Rust
`libkbcore.so` is NOT required: `KbCore.loaded` is false without it and the
`StubPredictor` keeps the IME running.

## Privacy notes

- The IME declares **only** `BIND_INPUT_METHOD`. No `INTERNET` permission in
  the shell; sync transport is a stub.
- No learning in password fields: `KbInputMethodService` inspects
  `EditorInfo.inputType` (`isPasswordField()`) and skips `Predictor.learn`
  for password / visible-password / web-password variations.
- Category packs opt out via `privacy.learn: false` (`numbers`, `math`).
- Personal-dictionary sync/export is **opt-in only** (`SyncPreferences`, off by
  default); tombstones (`del: true` on the wire, legacy `deleted` accepted on
 read) replicate and are never resurrected.
