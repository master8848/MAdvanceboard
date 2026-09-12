# Android Build Log — assembleDebug

- Date (UTC): 2026-09-12
- Command: `cd android && ./gradlew assembleDebug --no-daemon`
- Exit code: `1` (BUILD FAILED)
- Toolchain:
  - `java -version`: `openjdk 17.0.2 2022-01-18` (mise `java 17.0.2`, `JAVA_HOME=/Users/apple/.local/share/mise/installs/java/17.0.2`)
  - System `java` not on PATH (macOS stub: "Unable to locate a Java Runtime"); build used mise JDK via `export JAVA_HOME=...; export PATH="$JAVA_HOME/bin:$PATH"`.
  - Gradle wrapper: `9.6.0` (`android/gradle/wrapper/gradle-wrapper.properties`, distribution downloaded OK on first run)
  - Version catalog (`android/gradle/libs.versions.toml`): AGP `9.4.0`, Kotlin `2.2.20`, KSP `2.2.20-2.0.2`, Compose BOM `2026.09.00`
  - Android SDK: `~/Library/Android/sdk` (`ANDROID_HOME=/Users/apple/Library/Android/sdk`), `android/local.properties` absent (SDK resolved from env)
- APK: none produced (no `app/build/outputs/apk/debug/*.apk`)

## Full output (second run, distribution cached)

```
To honour the JVM settings for this build a single-use Daemon process will be forked. For more on this, please refer to https://docs.gradle.org/9.6.0/userguide/gradle_daemon.html#sec:disabling_the_daemon in the Gradle documentation.
Daemon will be stopped at the end of the build

FAILURE: Build failed with an exception.

* Where:
Build file '/Volumes/New Volume/code/keyboard-project/android/app/build.gradle.kts' line: 1

* What went wrong:
An exception occurred applying plugin request [id: 'org.jetbrains.kotlin.android', version: '2.2.20']
> Failed to apply plugin 'org.jetbrains.kotlin.android'.
   > ⛔ Failed to apply plugin 'com.jetbrains.kotlin.android'
     The 'org.jetbrains.kotlin.android' plugin is no longer required for Kotlin support since AGP 9.0.
     Solution: Remove the 'org.jetbrains.kotlin.android' plugin from this project's build file: app/build.gradle.kts.
     See https://issuetracker.google.com/438678642 for more details.
      > java.lang.Throwable (no error message)

* Try:
> Run with --stacktrace option to get the stack trace.
> Run with --info or --debug option to get more log output.
> Run with --scan to get full insights from a Build Scan (powered by Develocity).
> Get more help at https://help.gradle.org.

BUILD FAILED in 3s
GRADLE_EXIT=1
```

## First-run note

First invocation also downloaded `gradle-9.6.0-bin.zip` (100%, ~2m10s incl. download) then failed identically at `app/build.gradle.kts:1`. Network for Gradle distribution + plugin resolution works.

---

# Fix session — 2026-09-12 (runs 2–7, same command + JDK 17 env as above)

## Changes applied (android/ only)

1. `build.gradle.kts` (root), `app/`, `ime/`, `core-bridge/`, `plugin-api/`,
   `sync/build.gradle.kts`: removed `alias(libs.plugins.kotlin.android)` (AGP 9
   built-in Kotlin); kept `org.jetbrains.kotlin.plugin.compose` in `app`, `ime`.
   `gradle/libs.versions.toml`: removed `kotlin-android` plugin entry.
2. All five modules: `android.kotlinOptions { jvmTarget = "17" }` →
   top-level `kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }`
   (per https://developer.android.com/build/migrate-to-built-in-kotlin).
3. `gradle/libs.versions.toml`: `kotlin 2.2.20` → `2.3.20`
   (compose compiler plugin follows via `version.ref`),
   `ksp 2.2.20-2.0.2` → `2.3.12`, `composeBom 2026.09.00` → `2026.06.01`.
   Rationale: KGP 2.2.20 is incompatible with AGP 9.4 in BOTH modes —
   built-in Kotlin rejects the plugin, and with `android.builtInKotlin=false`
   the plugin crashes casting the new-DSL extension to the removed
   `BaseExtension`; KSP 2.0.x hard-errors under built-in Kotlin
   (google/ksp#2729, fix only in KSP 2.3.x, which needs Kotlin 2.3.x).
   BOM 2026.09.00 pins compose 1.12.1 (`minCompileSdk=37`, no android-37
   platform installed and no `sdkmanager` on this host); 2026.06.01 pins
   compose 1.11.4 (`minCompileSdk=35`, verified from AAR metadata).
   compileSdk/targetSdk stay 36, minSdk 26.
4. `ime/src/main/java/com/kb/ime/KbInputMethodService.kt:187-188`: deleted
   `override fun shouldOfferSwitchingToNextInputMethod() = true` — the method
   is `final` in API-36 `android.jar` (verified via `javap`), so the override
   cannot compile against compileSdk 36. Only source change; no refactor.
5. Created empty `consumer-rules.pro` in `core-bridge/`, `ime/`, `plugin-api/`,
   `sync/` — all four declare `consumerProguardFiles("consumer-rules.pro")`
   but the files were absent (`:core-bridge:mergeDebugConsumerProguardFiles`
   failed on the missing file).

## Run 2 — remove kotlin-android only (Kotlin 2.2.20): FAIL (config)

```
> Configure project :app
e: .../android/app/build.gradle.kts:36:5: Unresolved reference 'kotlinOptions'.
FAILURE: ... Script compilation errors: Line 36: kotlinOptions { ^ Unresolved reference
```

## Run 3 — + kotlinOptions→compilerOptions, KSP→2.2.20-2.0.4: FAIL (config :sync)

```
> Using kotlin.sourceSets DSL to add Kotlin sources is not allowed with built-in Kotlin.
  Kotlin source set 'debug' contains: [.../sync/build/generated/ksp/debug/kotlin, .../sync/build/generated/ksp/debug/java]
```

## Run 4 — + android.builtInKotlin=false, kotlin-android restored: FAIL (config :app)

```
WARNING: The option setting 'android.builtInKotlin=false' is deprecated. ... removed in version 10.0
An exception occurred applying plugin request [id: 'org.jetbrains.kotlin.android', version: '2.2.20']
> Failed to apply plugin 'org.jetbrains.kotlin.android'.
   > class ...ApplicationExtensionImpl$AgpDecorated_Decorated cannot be cast to class com.android.build.gradle.BaseExtension
```

Flag reverted; Kotlin 2.2.20 proven unbuildable with AGP 9.4 either way.

## Run 5 — built-in Kotlin + Kotlin 2.3.20 + KSP 2.3.12: FAIL (compile :ime)

```
> Task :ime:compileDebugKotlin FAILED
e: .../KbInputMethodService.kt:188:5 'shouldOfferSwitchingToNextInputMethod' in 'InputMethodService' is final and cannot be overridden.
```

## Run 6 — + override deleted: FAIL (AAR metadata, :app)

Nine `androidx.compose.*:1.12.1` deps (BOM 2026.09.00)
`require ... compile against version 37 or later ... :app is currently compiled against android-36.`
(`:app:packageDebug` still emitted a 13 MiB APK in this run, but the build
exited 1 — not counted as success.)

## Run 7 — + BOM→2026.06.01: FAIL (only :core-bridge task)

```
Execution failed for task ':core-bridge:mergeDebugConsumerProguardFiles'
> Supplied consumer proguard configuration does not exist: .../android/core-bridge/consumer-rules.pro
```

## Run 8 — + consumer-rules.pro x4: SUCCESS

```
BUILD SUCCESSFUL in 15s
153 actionable tasks: 39 executed, 114 up-to-date
GRADLE_EXIT=0
```

APK: `app/build/outputs/apk/debug/app-debug.apk` (13 MiB).
`aapt dump badging`: `package: name='com.kb.app' versionCode='1'
versionName='0.1.0' ... compileSdkVersion='36'`, `sdkVersion:'26'`,
`targetSdkVersion:'36'`, `BIND_INPUT_METHOD` present.
`KbInputMethodService` confirmed present in `classes4.dex`.
No bridge regen was needed (failure was pure Gradle/plugin/SDK wiring +
one final-method override); `core-rust/`, `packs/`, `docs/`, root files
untouched.

---

# Run 9 — 2026-09-12: rebuild after multi-layout pads landing — FAIL (compile :ime)

- Command: `cd android && ./gradlew assembleDebug --no-daemon` (exit `1`,
  `BUILD FAILED in 9s`, 137 tasks: 5 executed, 132 up-to-date)
- Toolchain (unchanged): mise JDK 17.0.2
  (`JAVA_HOME=/Users/apple/.local/share/mise/installs/java/17.0.2`),
  Gradle wrapper 9.0, SDK `~/Library/Android/sdk`
- APK: none produced; existing
  `app/build/outputs/apk/debug/app-debug.apk` (13 MiB, dated 07:40) is
  STALE — predates the pads landing (`8bf5f13`/`dc965ee`/`5f016a5`).
  `aapt dump badging` on the stale APK still reads
  `package: name='com.kb.app' versionCode='1' versionName='0.1.0'`,
  `sdkVersion:'26'`, `targetSdkVersion:'36'`, `BIND_INPUT_METHOD` present.
  Do NOT install it as a pads build.
- NO source edits made in this pass (build + logs only, per scope).

## Blocker (exact, read-only diagnosis)

`ime/src/main/java/com/kb/ime/PadSpec.kt:132:1 Syntax error: Unclosed comment.`

Root cause: Kotlin block comments nest. The KDoc opener `/**` on line 6
is re-entered by the literal `/*` inside `` `layouts/*.json` `` on line 7,
so the `*/` on line 14 only closes the inner level and the comment swallows
the rest of the file to EOF (raw counts: 5× `/*` vs 4× `*/`). Every
declaration after line 14 (`DEFAULT_LAYOUT_ID`, `SUPPORTED_LAYOUT_IDS`,
`loadLayoutSpec`/`parseLayoutSpec`, `LayoutSpecUi`, `LayoutKeyUi`,
`PadRoles`) is therefore invisible, producing all 46 `e:` errors below.
Owning agent fix (one line, e.g. reword to `` `layouts/` JSON `` or
`` `layouts&#47;*.json` ``); other pads files have no stray `/*`.

## Log excerpt (`:ime:compileDebugKotlin`, 46 errors)

```
> Task :ime:compileDebugKotlin
e: .../ime/.../KbInputMethodService.kt:58:43 Unresolved reference 'DEFAULT_LAYOUT_ID'.
e: .../ime/.../KbInputMethodService.kt:60:29 Unresolved reference 'LayoutSpecUi'.
e: .../ime/.../KbInputMethodService.kt:75:30 Property delegate must have a 'getValue(...)' method. ...
e: .../ime/.../KbInputMethodService.kt:75:33 Cannot infer type for type parameter 'T'. ...
e: .../ime/.../KbInputMethodService.kt:75:48 Unresolved reference 'DEFAULT_LAYOUT_ID'.
e: .../ime/.../KbInputMethodService.kt:92:22 Unresolved reference 'loadLayoutSpec'.
e: .../ime/.../KbInputMethodService.kt:130:72 Argument type mismatch: actual type is 'ComposeView', but 'Context' was expected.
e: .../ime/.../KbInputMethodService.kt:247:31 Unresolved reference 'keyByCode'.
e: .../ime/.../KbInputMethodService.kt:314:26 Unresolved reference 'SUPPORTED_LAYOUT_IDS'.
e: .../ime/.../KbInputMethodService.kt:317:22 Unresolved reference 'loadLayoutSpec'.
e: .../ime/.../KeyPadView.kt:17:29 Unresolved reference 'loadLayoutSpec'.
e: .../ime/.../PadModeStore.kt:24:53 Unresolved reference 'DEFAULT_LAYOUT_ID'.
e: .../ime/.../PadModeStore.kt:26:13 Unresolved reference 'DEFAULT_LAYOUT_ID'.
e: .../ime/.../PadModeStore.kt:28:30 Unresolved reference 'SUPPORTED_LAYOUT_IDS'.
e: .../ime/.../PadModeStore.kt:28:66 Unresolved reference 'DEFAULT_LAYOUT_ID'.
e: .../ime/.../PadModeStore.kt:33:26 Unresolved reference 'SUPPORTED_LAYOUT_IDS'.
e: .../ime/.../PadSpec.kt:132:1 Syntax error: Unclosed comment.
e: .../ime/.../PadView.kt:32:15 Unresolved reference 'LayoutSpecUi'.
e: .../ime/.../PadView.kt:32:30 Unresolved reference 'loadLayoutSpec'.
e: .../ime/.../PadView.kt:32:54 Unresolved reference 'DEFAULT_LAYOUT_ID'.
[... 20 further cascading unresolved-reference/type-mismatch errors in
...  PadView.kt (id/rows/cols/keys/role/symbols/PadRoles/LayoutKeyUi) and
...  PredictorLayout.kt (SUPPORTED_LAYOUT_IDS x2) ...]
> Task :ime:compileDebugKotlin FAILED
FAILURE: Build failed with an exception.
* What went wrong:
Execution failed for task ':ime:compileDebugKotlin' ...
> Compilation error. See log for more details
BUILD FAILED in 9s
GRADLE_EXIT=1
```

Full raw output retained at
`/private/var/folders/cg/fj7yz97x5dx49jm6q66x4vqr0000gn/T/opencode/build9.log`
(46 `^e: ` lines).

---

# Run 10 — 2026-09-12: rebuild after PadSpec KDoc fix (4172df4) — FAIL (compile :ime, 1 error)

- Command: `cd android && ./gradlew assembleDebug --no-daemon` (exit `1`,
  `BUILD FAILED in 9s`, 137 tasks: 2 executed, 135 up-to-date)
- Toolchain (unchanged): mise JDK 17.0.2
  (`JAVA_HOME=/Users/apple/.local/share/mise/installs/java/17.0.2`),
  Gradle wrapper 9.6.0, SDK `~/Library/Android/sdk`
  (`ANDROID_HOME=/Users/apple/Library/Android/sdk`)
- Fix verified: `4172df4` cleared the `PadSpec.kt:132 Unclosed comment`
  blocker — the 46-error cascade is gone; `:ime:compileDebugKotlin` now
  reports exactly 1 error (below). No `PadSpec`/`DEFAULT_LAYOUT_ID`/
  `SUPPORTED_LAYOUT_IDS`/`loadLayoutSpec` unresolved references remain.
- APK: none produced; existing
  `app/build/outputs/apk/debug/app-debug.apk` (13 MiB, dated Sep 12 07:40)
  is STALE — predates the pads landing (`8bf5f13`/`dc965ee`/`5f016a5`) and
  the KDoc fix (`4172df4`). `aapt dump badging` on the stale APK still reads
  `package: name='com.kb.app' versionCode='1' versionName='0.1.0'`,
  `compileSdkVersion='36'`, `sdkVersion:'26'`, `targetSdkVersion:'36'`,
  `uses-permission: name='android.permission.BIND_INPUT_METHOD'`.
  Do NOT install it as a pads build.
- NO source edits made in this pass (build + logs only, per scope).

## Blocker (exact, read-only diagnosis)

```
> Task :ime:compileDebugKotlin FAILED
e: file:///Volumes/New%20Volume/code/keyboard-project/android/ime/src/main/java/com/kb/ime/KbInputMethodService.kt:130:72 Argument type mismatch: actual type is 'ComposeView', but 'Context' was expected.
```

`PadModeStore.save(context: Context, layoutId: String)` (object, line 32)
is called at `KbInputMethodService.kt:130` as `PadModeStore.save(this, it)`
from inside nested lambdas:

```kotlin
val strip = ComposeView(this).apply {          // line 118
    setContent {                               // line 119
        ImeScreen(
            ...
            onLayoutChanged = { setPadLayout(PadModeStore.save(this, it)) }  // line 130
        )
    }
}
```

The unqualified `this` at line 130 resolves to the enclosing `ComposeView`
receiver (via `apply`/`setContent`), not the `KbInputMethodService`
`Context`, hence `ComposeView` vs `Context` mismatch. Owning-agent fix
(one line, e.g. qualify the service receiver or hoist a `Context` val —
`this@KbInputMethodService`, `root.context`, `applicationContext` — then
also check the line-126 `refreshPad(root)` call, which already passes the
`LinearLayout` explicitly).

## Log excerpt

```
> Task :ime:compileDebugKotlin FAILED
e: file:///Volumes/New%20Volume/code/keyboard-project/android/ime/src/main/java/com/kb/ime/KbInputMethodService.kt:130:72 Argument type mismatch: actual type is 'ComposeView', but 'Context' was expected.
FAILURE: Build failed with an exception.
* What went wrong:
Execution failed for task ':ime:compileDebugKotlin' (registered by plugin 'com.android.internal.library').
> A failure occurred while executing org.jetbrains.kotlin.compilerRunner.btapi.BuildToolsApiCompilationWork
   > Compilation error. See log for more details
BUILD FAILED in 9s
GRADLE_EXIT=1
```

Full raw output retained at
`/private/var/folders/cg/fj7yz97x5dx49jm6q66x4vqr0000gn/T/opencode/build10.log`
(1 `^e: ` line).

---

# Run 11 — 2026-09-12: rebuild after receiver fix (b8622cd) — SUCCESS

- Date (UTC): 2026-09-12T02:04Z
- Command: `cd android && ./gradlew assembleDebug --no-daemon` (exit `0`,
  `BUILD SUCCESSFUL in 13s`, 153 tasks: 12 executed, 141 up-to-date)
- Toolchain (unchanged): mise JDK 17.0.2
  (`JAVA_HOME=/Users/apple/.local/share/mise/installs/java/17.0.2`),
  Gradle wrapper 9.6.0, SDK `~/Library/Android/sdk`
  (`ANDROID_HOME=/Users/apple/Library/Android/sdk`)
- Fix verified: `b8622cd fix(ime): qualify service Context in Compose
  onLayoutChanged` cleared the run-10 blocker
  (`KbInputMethodService.kt:130:72 ComposeView vs Context mismatch`) —
  `:ime:compileDebugKotlin` now passes, no source edits in this pass
  (build + logs only, per scope).
- APK: `app/build/outputs/apk/debug/app-debug.apk` (13 MiB,
  `-rw-r--r-- 13M 12 Sep 07:49`, fresh — includes pads landing
  `8bf5f13`/`dc965ee`/`5f016a5` + KDoc fix `4172df4` + receiver fix
  `b8622cd`).
- Verify: `aapt dump badging` →
  `package: name='com.kb.app' versionCode='1' versionName='0.1.0'`,
  `compileSdkVersion='36'`, `sdkVersion:'26'`, `targetSdkVersion:'36'`,
  `uses-permission: name='android.permission.BIND_INPUT_METHOD'`,
  `launchable-activity: name='com.kb.app.SettingsActivity'`;
  10 dex files; `KbInputMethodService` present in `classes4.dex`.

## Log excerpt

```
> Task :ime:compileDebugKotlin
> Task :app:packageDebug
> Task :app:assembleDebug
BUILD SUCCESSFUL in 13s
153 actionable tasks: 12 executed, 141 up-to-date
```

## Install / enable-IME + 9/12/16 toggle test steps (READY — build green)

1. `cd android && adb install -r app/build/outputs/apk/debug/app-debug.apk`
2. Settings → System → Languages & input → On-screen keyboard → enable the
   KB IME, then switch to it via the keyboard-switcher icon.
3. Note: Android may restrict third-party IMEs in password/incognito fields —
   verify in a plain text field first.
4. Toggle test: switch pad 9 → 12 → 16 (persisted via PadModeStore);
   verify keys/labels/grid follow `assets/layouts/t9-{9,12,16}.json`,
   predictions still flow through the shared Predictor path with the
   correct `layout_id`, and the mode survives IME restart.
