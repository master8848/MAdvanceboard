# Android IME Shell — QA Report

Date: 2026-09-12 · Scope: `android/` only (`core-rust/`, `packs/`, `docs/`, `SPEC.md` read-only grounding, not modified).
Method: static validation only — no JDK/Gradle build (no Java runtime in this environment). No new modules, no Rust code added.

## 1. Gradle / SDK validation — PASS

| Check | Expected | Actual | Result |
|---|---|---|---|
| AGP | 9.4.0 | `gradle/libs.versions.toml:2` `agp = "9.4.0"` | PASS |
| Kotlin | 2.2.20 | `gradle/libs.versions.toml:3` `kotlin = "2.2.20"`; KSP `2.2.20-2.0.2` (`:4`) matches | PASS |
| compileSdk | 36 | `app/build.gradle.kts:9`, `ime/build.gradle.kts:9`, `core-bridge/build.gradle.kts:8`, `sync/build.gradle.kts:10`, `plugin-api/build.gradle.kts:8` all `compileSdk = 36` | PASS |
| targetSdk | 36 | `app/build.gradle.kts:14` `targetSdk = 36` (libraries correctly omit `targetSdk`) | PASS |
| minSdk | 26 | `app/build.gradle.kts:13`, `ime/build.gradle.kts:12`, `core-bridge/build.gradle.kts:11`, `sync/build.gradle.kts:13`, `plugin-api/build.gradle.kts:11` all `minSdk = 26` | PASS |
| Compose BOM | present | `gradle/libs.versions.toml:5,20` `composeBom = "2026.09.00"` + `compose-bom`; consumed via `platform(...)` in `app/build.gradle.kts:53-55` and `ime/build.gradle.kts:38-40` | PASS |
| Java 17 | all modules | `sourceCompatibility/targetCompatibility = VERSION_17` + `jvmTarget = "17"` in all 5 `build.gradle.kts` | PASS |
| Modules included | app, ime, plugin-api, core-bridge, sync | `settings.gradle.kts:16-20` includes all five | PASS |
| Repos | FAIL_ON_PROJECT_REPOS + google/mavenCentral | `settings.gradle.kts:8-14` | PASS |

No mismatches. Notes (info only): Gradle wrapper is 9.6.0 (`gradle/wrapper/gradle-wrapper.properties`), compatible with AGP 9.4.0; `sync` module enables `buildConfig = true` (`sync/build.gradle.kts:18`) but has no `BuildConfig` usage yet — harmless.

## 2. Category assets `ime/src/main/assets/categories/*.json` — PASS (after fix)

All 8 files (`words, nepali, js, rust, html, emoji, numbers, math`) are valid JSON. Required keys `id/title/version/source/privacy` now present in all files; `privacy.learn=false` preserved for `numbers`/`math`.

| File | id | title | version | source | privacy.learn | Result |
|---|---|---|---|---|---|---|
| `words.json` | words | EN | 1.0.0 | bundled | true | PASS |
| `nepali.json` | nepali | NE | 1.0.0 | bundled | true | PASS |
| `js.json` | js | js | 1.0.0 | bundled | true | PASS |
| `rust.json` | rust | rust | 1.0.0 | bundled | true | PASS |
| `html.json` | html | html | 1.0.0 | bundled | true | PASS |
| `emoji.json` | emoji | emoji | 1.0.0 | bundled | true | PASS |
| `numbers.json` | numbers | 123 | 1.0.0 | bundled | **false** | PASS |
| `math.json` | math | math | 1.0.0 | bundled | **false** | PASS |

Parser compatibility: `plugin-api/.../CategoryRegistry.kt:41-58` uses `optString/optInt/optJSONObject` and ignores unknown keys, so the added `title/version/source` keys are forward-compatible and do not break `loadOne`/`parse`. Existing keys (`displayName, priority, langs, description, privacy.learn`) were left untouched.

## 3. IME service / bridge / sync — PASS

| Check | Evidence | Result |
|---|---|---|
| Password variations suppress learning | `ime/.../KbInputMethodService.kt:109-115` `isPasswordField()` covers `TEXT_PASSWORD`, `TEXT_VISIBLE_PASSWORD`, `TEXT_WEB_PASSWORD`, `NUMBER_PASSWORD`; `onStartInput` (`:54-58`) sets `isPasswordField`; `commitCandidate` (`:71-79`) gates learning on `!isPasswordField` | PASS |
| `onCreateInputView` caching | `KbInputMethodService.kt:16` `cachedInputView` field; `:27-28` early-return on cache hit, `:50` stores | PASS |
| `commitText` / `setComposingText` / `deleteSurroundingText` | `:81-83` `commitText(text,1)`; `:68` `setComposingText(seq,1)`; `:87` `deleteSurroundingText(1,0)` | PASS |
| core-bridge has no Android imports | `grep "import android" android/core-bridge` → no matches; `Predictor.kt:1-4` imports only `kotlinx.coroutines` | PASS |
| Predictor uses `Dispatchers.Default` | `core-bridge/.../Predictor.kt:46` (`suggest`) and `:53` (`learn`) both `withContext(Dispatchers.Default)`; no `Context` in `Predictor` interface (`:35-39`) | PASS |
| Sync Worker 12–24h periodic | `sync/.../SyncWorker.kt:34` `PeriodicWorkRequestBuilder<SyncWorker>(24, TimeUnit.HOURS)` with `enqueueUniquePeriodicWork(KEEP)` (`:37-41`); doc comment `:12-15` states 24h budget | PASS |
| Sync opt-in off by default | `sync/.../SyncPreferences.kt:16-17` `it[OPT_IN] ?: false`; `SyncWorker.doWork` (`SyncWorker.kt:20-22`) early-returns success when not opted in; `SettingsActivity.kt:40,46-55` wires Switch → `setOptedIn` + schedule/cancel | PASS |
| IME manifest wiring | `ime/src/main/AndroidManifest.xml:7-18` service with `BIND_INPUT_METHOD`, `android.view.InputMethod` filter, `@xml/method` meta-data; `ime/src/main/res/xml/method.xml:1-3` points at `SettingsActivity` | PASS |

Minor observations (not failures, left as-is per minimal-change rule): `KbInputMethodService.commitCandidate` learn call is a documented stub (`:76-78`, service-scoped `CoroutineScope` TBD); `QwertyView`/`ImeScreen` candidates are stubs (`QwertyView.kt:22`, `ImeScreen.kt:35-38`); `SyncMerge` doc comment mentions `{count, last_seen, …}` while entity fields are `word/lang/count/lastSeen/deleted/deviceId` — code itself (`SyncMerge.kt:11-27,30-61`) is self-consistent LWW + tombstone-wins.

## Fixes applied

1. Added `title` (mirrors `displayName`), `version: "1.0.0"`, `source: "bundled"` to all 8 files in `ime/src/main/assets/categories/`. Rationale: task schema requires `id/title/version/source/privacy`; pre-existing manifests only had `id/displayName/priority/langs/description/privacy`. Values chosen to match sibling `packs/*.json` conventions (`version "1.0.0"`, titles mirror display names) and to keep `CategoryRegistry` parsing intact. `privacy.learn` values unchanged (`false` for `numbers`, `math`; `true` elsewhere). No other files modified.

## Remaining items needing JDK 17 + `assembleDebug`

> Addendum 2026-09-12 (toolchain bump, see `android/BUILD_STATUS.md`):
> the §1 / §Re-test rows below still record Kotlin 2.2.20, KSP
> `2.2.20-2.0.2`, Compose BOM 2026.09.00 as-found. Current pins are
> Kotlin **2.3.20**, KSP **2.3.12**, BOM **2026.06.01**
> (`gradle/libs.versions.toml:3-5`) — 2.2.20 cannot build under AGP 9.4
> and BOM 2026.09.00 needs compileSdk 37 (no android-37 platform on the
> build host). `assembleDebug` is green (run 11). Historical rows kept
> verbatim; treat the pins here as superseding them.

- [ ] `export JAVA_HOME=<jdk17> && ./gradlew :app:assembleDebug :ime:assembleDebug :sync:assembleDebug` (no Java runtime here; static checks only).
- [ ] `./gradlew lint` / Android Lint on `ime` (Compose, `PrimaryScrollableTabRow` in `CategoryTabs.kt:28`, `LazyRow`/`LazyColumn` usage).
- [ ] Instrumented check: enable IME via `OnboardingActivity`, type into password vs. plain fields, confirm no learning on password fields and `cachedInputView` reuse across `onCreateInputView` calls.
- [ ] Room/KSP verify: `:sync:kspDebugKotlin` (compiler `room-compiler` wired at `sync/build.gradle.kts:38`).
- [ ] WorkManager verify: toggle sync opt-in in `SettingsActivity`, confirm 24h periodic work enqueued/cancelled (`adb shell dumpsys jobscheduler`).
- [ ] Confirm `CategoryRegistry.loadManifests` reads the amended manifests on-device (new keys ignored, `learn` flags respected).

## Re-test 2026-09-12 — PASS (static only, no JDK/Gradle build)

Scope: `android/` only. No files modified (no JSON fixes needed — prior fixes intact). Method: file reads + `grep`-based checks, `python3 json.load` validation.

| # | Check | Evidence | Result |
|---|---|---|---|
| 1 | AGP 9.4.0 / Kotlin 2.2.20 / KSP 2.2.20-2.0.2 | `gradle/libs.versions.toml:2-4` | PASS |
| 2 | Compose BOM 2026.09.00 + `platform(...)` consumption | `gradle/libs.versions.toml:5,20`; `app/build.gradle.kts:53-55`, `ime/build.gradle.kts:38-40` | PASS |
| 3 | compileSdk 36 all modules; targetSdk 36 app only; minSdk 26 all; Java 17 all | `app/build.gradle.kts:9,13-14,34-38`; `ime/build.gradle.kts:9,12,21-26`; `core-bridge/build.gradle.kts:8,11,16-21`; `sync/build.gradle.kts:10,13,22-27`; `plugin-api/build.gradle.kts:8,11,16-21` | PASS |
| 4 | Modules + repos | `settings.gradle.kts:8-14` FAIL_ON_PROJECT_REPOS + google/mavenCentral; `:16-20` all five modules | PASS |
| 5 | IME manifest: BIND_INPUT_METHOD + InputMethod filter + method meta-data | `ime/src/main/AndroidManifest.xml:4,7-17` | PASS |
| 6 | method.xml points at SettingsActivity (no `<subtype>` entries — single subtype-less IME, acceptable for skeleton) | `ime/src/main/res/xml/method.xml:1-3` | PASS (info) |
| 7 | Password gating: 4 variations, `onStartInput` sets flag, learn gated | `ime/.../KbInputMethodService.kt:54-58`, `:71-79`, `:109-115` | PASS |
| 8 | View caching: `cachedInputView` early-return + store | `KbInputMethodService.kt:16,27-28,50` | PASS |
| 9 | Commit paths: `commitText(text,1)` / `setComposingText(seq,1)` / `deleteSurroundingText(1,0)` | `KbInputMethodService.kt:68,81-83,87` | PASS |
| 10 | No INTERNET permission in `ime/` | `grep -rn INTERNET ime/` → no matches (only hit repo-wide is `android/README.md:33` doc mention) | PASS |
| 11 | No learn call outside password guard | `grep -rn learn --include=*.kt` → only `Predictor.kt:37,52` (interface + stub impl) and `CategoryRegistry.kt:47` comment; `KbInputMethodService.commitCandidate:75-78` stub sits inside `if (!isPasswordField)` | PASS |
| 12 | core-bridge: no android imports, Dispatchers.Default, no Context in interface | `grep import android core-bridge/` → none; `Predictor.kt:1-4,35-39,46,53` | PASS |
| 13 | Sync: 24h periodic + KEEP + opt-in default false + doWork early-return | `SyncWorker.kt:20-22,34,37-41`; `SyncPreferences.kt:16-17` `?: false` | PASS |
| 14 | CategoryRegistry uses opt* parsing | `CategoryRegistry.kt:43,45,48-50,52-54,57` `optJSONArray/optString/optJSONObject/optBoolean/optInt` | PASS |
| 15 | 8 category JSONs valid, title/version/source present, learn=false for numbers/math | `python3 json.load` all 8 VALID; `numbers.json` + `math.json` `privacy.learn=false`, others `true`; keys include `id/title/version/source/privacy` | PASS |

JSON fixes this round: none (all 8 files already carry `title`/`version:"1.0.0"`/`source:"bundled"` from prior fix; verified intact).
