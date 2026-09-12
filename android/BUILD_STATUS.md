# Android Build Status — PASS (run 11, receiver fix b8622cd)

- Date (UTC): 2026-09-12T02:04Z
- Result: **PASS — `./gradlew assembleDebug --no-daemon` exits `0`
  (`BUILD SUCCESSFUL in 13s`, 153 tasks: 12 executed, 141 up-to-date).
  Fresh APK produced.**
- Progress since run 10: `b8622cd` (qualify service Context in Compose
  `onLayoutChanged`) cleared the `KbInputMethodService.kt:130:72
  ComposeView vs Context` blocker — `:ime:compileDebugKotlin` now passes.
  No source edits in this pass (build + logs only).

## APK

- Path: `android/app/build/outputs/apk/debug/app-debug.apk` (13 MiB,
  `-rw-r--r-- 13M 12 Sep 07:49`, fresh run-11 build —
  includes `8bf5f13`/`dc965ee`/`5f016a5`/`4172df4`/`b8622cd`)
- Verified (run 11): `aapt dump badging` →
  `package: name='com.kb.app' versionCode='1' versionName='0.1.0'`,
  `compileSdkVersion='36'`, `sdkVersion:'26'`, `targetSdkVersion:'36'`,
  `uses-permission: name='android.permission.BIND_INPUT_METHOD'`,
  `launchable-activity: name='com.kb.app.SettingsActivity'`;
  10 dex files; `KbInputMethodService` present in `classes4.dex`.
- Command: `cd android && ./gradlew assembleDebug --no-daemon` (exit `0`)
- Full log: `android/BUILD_LOG.md` (run 11 section)
- Toolchain (unchanged): mise JDK 17.0.2, Gradle wrapper 9.6.0,
  SDK `~/Library/Android/sdk` (platforms android-34/35/36), network OK.

## Fixes applied (android/ only; core-rust/, packs/, docs/, root files untouched)

1. Removed `alias(libs.plugins.kotlin.android)` from root (apply false), `app`,
   `ime`, `core-bridge`, `plugin-api`, `sync` + catalog entry (AGP 9 built-in
   Kotlin). Kept `org.jetbrains.kotlin.plugin.compose` in `app`, `ime`.
2. `android.kotlinOptions { jvmTarget = "17" }` → top-level
   `kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }` in all five
   modules. `compileOptions` Java 17, `compileSdk`/`targetSdk` 36, `minSdk` 26
   unchanged.
3. Version bumps (required, not optional): `kotlin 2.2.20` → `2.3.20`,
   `ksp 2.2.20-2.0.2` → `2.3.12`, `composeBom 2026.09.00` → `2026.06.01`.
   Kotlin 2.2.20 cannot build under AGP 9.4 in either mode (built-in Kotlin
   rejects the plugin; `android.builtInKotlin=false` crashes KGP 2.2.20 on the
   removed `BaseExtension`; KSP 2.0.x hard-errors under built-in Kotlin).
   BOM 2026.09.00 needs compileSdk 37 (no android-37 platform on host);
   2026.06.01 pins compose 1.11.4 (`minCompileSdk=35`).
4. One source fix (build-demanded, owning agent please review):
   `ime/.../KbInputMethodService.kt` — deleted the
   `shouldOfferSwitchingToNextInputMethod() = true` override; the method is
   `final` in API-36 `android.jar`, so it cannot compile against SDK 36.
   The globe affordance is now platform-controlled.
5. Created empty `consumer-rules.pro` in `core-bridge/`, `ime/`, `plugin-api/`,
   `sync/` (all four declare it; files were missing).
6. No bridge regen needed.

## Known follow-ups (non-blocking)

- Kotlin is now 2.3.20 / KSP 2.3.12 / compose BOM 2026.06.01 (compose 1.11.4).
  Revisit compose 1.12.x + compileSdk 37 once an android-37 platform is
  installed (no `sdkmanager` on this host).
- `android.newDsl` stays default (`true`); no opt-out flags in
  `gradle.properties`.

## Install / enable-IME + 9/12/16 toggle test steps (READY — build green, run on device)

1. `cd android && adb install -r app/build/outputs/apk/debug/app-debug.apk`
2. Settings → System → Languages & input → On-screen keyboard → enable the KB
   IME, then switch to it via the keyboard-switcher icon.
3. Note: Android may restrict third-party IMEs in password/incognito fields —
   verify in a plain text field first.
4. Toggle test: switch pad 9 → 12 → 16 (persisted via PadModeStore);
   verify keys/labels/grid follow `assets/layouts/t9-{9,12,16}.json`,
   predictions still flow through the shared Predictor path with the
   correct `layout_id`, and the mode survives IME restart.
