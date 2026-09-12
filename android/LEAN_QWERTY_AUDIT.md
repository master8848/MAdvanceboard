# Android IME — Completeness + Leanness Audit (QWERTY focus)

Scope: `android/` only. `SPEC.md`, `android/README.md`, `android/QA_REPORT.md` read-only grounding. No Rust/packs/docs/root edits.
Date: 2026-09-12. Method: static read of all `android/**/*.kt`, `*.kts`, `*.toml`, manifests, assets. No build (no JDK here).

## Verdict

Skeleton passes static QA (SDK versions, manifest wiring, password-learn gate, view cache, StubPredictor fallback, no INTERNET). It is **not a default-driver keyboard yet**: input lifecycle, EditorInfo handling, cursor/editor-action, subtype/switching, fullscreen, and incognito gating are missing; QWERTY and 9-key chrome are stubs with no shared Predictor path; and the IME process drags the full sync stack (Room/DataStore/Work) via a wrong-direction dependency.

---

## 1. Default-keyboard duties — PASS / GAP

| Duty | Status | Evidence |
|---|---|---|
| `onCreate` calls super | PASS | `ime/.../KbInputMethodService.kt:23-25` |
| `onCreateInputView` caching (early-return + store) | PASS | `KbInputMethodService.kt:16,27-28,50` |
| `onStartInput` sets password flag + clears seq | PASS | `KbInputMethodService.kt:54-58` |
| Password variants suppress learning | PASS | `KbInputMethodService.kt:109-115` covers TEXT_PASSWORD, VISIBLE_PASSWORD, WEB_PASSWORD, NUMBER_PASSWORD; gated at `:71-79` |
| `commitText` / `setComposingText` / `deleteSurroundingText` present | PASS | `KbInputMethodService.kt:68,81-83,87` |
| Service manifest: BIND_INPUT_METHOD + InputMethod filter + method meta | PASS | `ime/src/main/AndroidManifest.xml:4,7-18`; `ime/src/main/res/xml/method.xml:1-3` |
| No INTERNET in IME (or app) manifests | PASS | `grep -rn INTERNET android/*/src` → no hits |
| `StubPredictor` fallback when `libkbcore.so` missing | PASS | `core-bridge/.../Predictor.kt:7-22` try/catch → `loaded=false`; service defaults to `StubPredictor` (`KbInputMethodService.kt:21`); `README.md:28-29` |
| core-bridge: no android imports, Dispatchers.Default, no Context in interface | PASS | `Predictor.kt:1-4,35-39,46,53` |
| `onFinishInput` / `onFinishInputView` / `onStartInputView` cleanup | **GAP (P0)** | No overrides anywhere in `KbInputMethodService.kt`. `seq` cleared only in `onStartInput`; composing text never `finishComposingText()`'d → stale composing leaks across fields |
| EditorInfo `inputType` handling (TEXT/NUMBER/PHONE/DATETIME) | **GAP (P0)** | Only `isPasswordField()` inspected. No `TYPE_CLASS_NUMBER/PHONE/DATETIME` branch, no per-class autocorrect/prediction disable, no email/URI variation handling |
| `InputConnection` cursor/context ops | **GAP (P0)** | No `onUpdateSelection`, no `getTextBeforeCursor`/`getTextAfterCursor` (bigram `prev` never collected), no `finishComposingText`. `Predictor.suggest/learn` **never called** — `predictor` field is write-only; `commitCandidate:75-78` learn body is an empty comment, no CoroutineScope |
| Editor actions / Enter handling | **GAP (P0)** | No `onEditorAction`, no `EditorInfo.imeOptions` / `IME_ACTION_*` handling. `deleteLast()` (`:86-88`) is `private`, `@Suppress("unused")`, never wired — no delete key path at all |
| subtype / `method.xml` | **GAP (P0)** | `method.xml:1-3` has zero `<subtype>` entries (single subtype-less IME). No locale/layout subtypes for EN/NE |
| Switch to next IME | **GAP (P0)** | No `switchToNextInputMethod`, no `shouldOfferSwitchingToNextInputMethod`, no globe-key affordance |
| Fullscreen / extract handling | **GAP (P1)** | No `onEvaluateFullscreenMode`, no extract `EditText` config, no candidates-view for landscape fullscreen |
| Incognito / no-learn gating beyond password | **GAP (P0)** | No `EditorInfo.imeOptions FLAG_NO_PERSONALIZED_LEARNING` check. Category `privacy.learn=false` (numbers/math) never consulted at commit time. `CommitContext.isPasswordField` (`plugin-api/.../Models.kt:21-26`) exists but is unused by the service |

## 2. QWERTY mode — PASS / GAP

| Item | Status | Evidence |
|---|---|---|
| `QwertyView` exists | PASS | `ime/.../QwertyView.kt:1-30` |
| Toggle exists + swaps pad | PASS | `ImeScreen.kt:55` `TextButton("QWERTY")`; `KbInputMethodService.kt:18,37-40,90-105` `qwertyFallback` + `refreshPad()` |
| Full QWERTY grid | **GAP (P1)** | `QwertyView.kt:22-27` is one 6-key stub row (`q w e r t y`). No rows 2–3, no Shift/Sym/Space/Delete/Enter |
| Shared Predictor path (same suggest/learn, only encoder differs) | **GAP (P1)** | Neither mode calls `predictor`. `ImeScreen.kt:36-38` hardcodes `listOf("the","tie","vie")`. No T9-digit encoder vs QWERTY-raw-string encoder separation; both paths just `commitText` |
| Auto-caps / punct / space behavior | **GAP (P1)** | No `InputConnection.getTextBeforeCursor` sentence-start check, no auto-cap, no double-space→period, no punct-breaks-word |
| Learning debounce + cap + forget/undo | **GAP (P1)** | No debounce (SPEC §2: 2s WAL batch), no 20k LRU cap, no block/pin/undo UI. `StubPredictor.learn` (`Predictor.kt:52-56`) is an unbounded `mutableListOf` — leaks memory by design |
| Code/math/numbers autocorrect-off | **GAP (P1)** | No category branch. QWERTY commits raw in every tab; `CategoryRegistry.learn` flag never read by IME |
| Toggle placement | **GAP (P2)** | SPEC §0: floating button bottom-right over Pad. Actual: `TextButton` in the suggestion row (`ImeScreen.kt:49-56`) — cramped strip, poor thumb reach, collides with future raw-as-typed chip |

## 3. 9-key + chrome — PASS / GAP

| Item | Status | Evidence |
|---|---|---|
| 3×3 pad, ≥48dp targets | PASS | `KeyPadView.kt:27-44`; `minWidth/minHeight = 48dp` at `:32,38-39` |
| SuggestionStrip `LazyRow`, 3 inline | PASS | `SuggestionStrip.kt:20-21` `items(candidates.take(3))` |
| CategoryTabs order matches SPEC | PASS | `CategoryTabs.kt:13-15` `numbers,EN,NE,js,rust,html,emoji,math,★personal` |
| Expand-all 30-item list | PASS (shape only) | `ImeScreen.kt:59` `.take(30)` |
| Hit-slop beyond 48dp / haptics | **GAP (P1)** | No `TouchDelegate`, no extra touch padding, no `performHapticFeedback`. Stock `Button`s in a 16dp-padding `GridLayout` only |
| Raw-as-typed chip | **GAP (P1)** | Strip renders only stub candidates; no literal-sequence chip (`SuggestionStrip.kt:13-30`) |
| Long-press: pin / block / info placement popup | **GAP (P1)** | No `combinedClickable`, no popup. SPEC §2/§4 placement (`packId • freq • accepts`) absent. Key `1` stub (`KbInputMethodService.kt:61-65`) commits `"."` only — no `.,?!` cycle; no long-press digit insert |
| Tabs reorderable + persisted | **GAP (P2)** | `onSelect` sets index only; state is `remember mutableIntStateOf` (not `rememberSaveable`, lost on rotation); no drag-reorder, no DataStore persistence |
| Expand-all: bottom-sheet grid + paging + search + placement | **GAP (P2)** | `ImeScreen.kt:70-93` is an inline `Column + LazyColumn`, not a bottom sheet/grid, no paging, no search field, `onExpandAll` in service is `/* TBD */` (`KbInputMethodService.kt:36`) |
| Sym/Shift/Space/Delete row or gestures | **GAP (P1)** | No 12-key toggle row (SPEC §0), no swipe-left=delete / swipe-up=space / long-press=symbols gestures on pad |

## 4. Lean audit

| Item | Status | Evidence / note |
|---|---|---|
| Heavy deps flagged, not added | n/a — deps **already present** | `gradle/libs.versions.toml:10-12` `room 2.8.2`, `datastore 1.2.0`, `work 2.11.0` |
| Room for single-table personal dict | **TRIM (P2)** | `sync/build.gradle.kts:34-35,38` `room-runtime + room-ktx + room-compiler (KSP)` + `ksp` plugin (`:5`) for one entity (`PersonalWordEntity.kt`), one DAO (`PersonalWordDao.kt`). SPEC §5 says personal dict is JSONL + WAL file — Room+KSP (annotation processing, sqlite native lib) is disproportionate. Keep only if FTS/paging proven needed |
| DataStore for one boolean | **TRIM (P2)** | `sync/build.gradle.kts:36`; `SyncPreferences.kt:10,16-17` uses it for a single `sync_opt_in` flag. Adds protobuf + coroutines machinery for what `SharedPreferences`/single-key store does |
| WorkManager for 24h no-op stub | **TRIM (P2)** | `sync/build.gradle.kts:37`; `SyncWorker.kt:19-28` early-returns when opted out, else re-serializes DB rows and drops the result. Keep the API (right primitive for 12–24h budget) but defer the dep until transport exists, or gate behind a flavor |
| **`:ime` → `:sync` dependency drags Room/Work/DataStore into the IME process** | **FIX (P1)** | `ime/build.gradle.kts:32` `implementation(project(":sync"))`. IME process should be UI + bridge only. Move sync DB/work behind `app` process or a lean interface; IME needs at most a `learnEnabled: Boolean` + fire-and-forget learn call |
| `appcompat` unused | **TRIM (P2)** | `app/build.gradle.kts:47`. Both activities are `ComponentActivity` (`SettingsActivity.kt:25`, `OnboardingActivity.kt:26`) — no AppCompat usage. Remove |
| `kotlin-compose` plugin + `buildConfig=true` on `:sync` with zero Compose usage | **TRIM (P2)** | `sync/build.gradle.kts:4,18`. Remove plugin flag + `buildConfig` (QA already notes no `BuildConfig` usage — `QA_REPORT.md:20`) |
| `lifecycle-runtime`, `coroutines-android` in IME | INFO | `ime/build.gradle.kts:34-36`. Small; `coroutines-android` only needed if touching main thread — audit call sites before trimming |
| Full Compose Material3 in IME | INFO | `ime/build.gradle.kts:38-44` BOM + ui/foundation/material3/runtime for a strip + tabs + stub list. Fine for MVP velocity; largest APK contributor — revisit with R8/minify (`app/build.gradle.kts:20` has `isMinifyEnabled=false`) + `isShrinkResources` before release |
| View-caching / recomposition | PASS with note | `cachedInputView` reuse is correct; `ComposeView.setContent` runs once (`KbInputMethodService.kt:27-50`). Note: toggle state is duplicated (service `qwertyFallback` vs Compose `selectedTab/expanded`) and pad swap (`refreshPad:90-105`) bypasses Compose state — future source of desync; unify into one state holder |
| Preload on bg thread | **GAP (P2)** | `onCreate` is empty (`KbInputMethodService.kt:23-25`). `CategoryRegistry.loadManifests` does asset IO on caller thread (`CategoryRegistry.kt:18-29`); `KbCore` static init `loadLibrary` runs on class-load thread (`Predictor.kt:12-19`). Preload manifests + warm predictor on `Dispatchers.IO/Default` at `onCreate` to avoid first-input jank |
| IME-process INTERNET avoidance | PASS | No INTERNET permission in any manifest; sync transport is a stub (`SyncWorker.kt:23-26` local I/O only). Keep it that way: when transport lands, run it in `:app` process, never `:ime` |
| APK size / memory notes | INFO | Single APK (`app` depends on `:ime`, `:ime` on `:sync` — `app/build.gradle.kts:43-45`). No minify/shrink; Room+Work+DataStore+Compose-M3 transitive graph dominates size. `StubPredictor.learned` unbounded list is the only in-IME memory leak vector today |

---

## Prioritized fixes (minimal diffs, `android/` only)

### P0 — missing default-keyboard behavior (must fix before dogfood)
1. **Finish input lifecycle**: add `onFinishInput()`/`onFinishInputView()` → `finishComposingText()` + `seq.clear()`; symmetric with `onStartInput` (`KbInputMethodService.kt:54-58`).
2. **Wire the Predictor**: give service a `Service`-scoped `CoroutineScope` (cancel in `onDestroy`); call `predictor.suggest(seq, prev)` per digit with `getTextBeforeCursor` prev-word, and `predictor.learn` in `commitCandidate` (already gated `!isPasswordField`). Delete the empty-stub comment at `:75-78`.
3. **Incognito gate**: `onStartInput` also checks `attribute.imeOptions and FLAG_NO_PERSONALIZED_LEARNING`; combine with `isPasswordField` and category `learn==false` into one `shouldLearn` flag.
4. **Editor actions + delete**: implement `onEditorAction`/`sendDefaultEditorAction`, wire a Delete path (today `deleteLast:86-88` is dead code), handle `IME_ACTION_DONE/GO/NEXT/SEARCH`.
5. **Next-IME affordance**: `shouldOfferSwitchingToNextInputMethod()=true` + globe/long-press-space hook to `switchToNextInputMethod`.
6. **Subtypes**: add `<subtype android:imeSubtypeMode="keyboard" android:imeSubtypeLocale="en"/>` (+ `ne`) entries to `method.xml`.

### P1 — QWERTY + correction gaps
7. **Real QWERTY grid**: 3 rows + Space/Delete/Enter/Sym rows in `QwertyView.kt`; route keys through one `onCommit`-vs-`onSuggest` split (raw commit for code/math/numbers, suggest+learn otherwise).
8. **Shared engine, split encoders**: introduce `T9Encoder.digits(key)` vs `QwertyEncoder.raw(text)` feeding the same `suggest(seq,ctx,tab,limit)`; replace `ImeScreen.kt:36-38` stubs with live `predictor.suggest`.
9. **Debounce + cap**: 2s batch learn (SPEC §2), 20k LRU cap with tombstone exemption, reject path (delete-within-5s → `rejects+=1`); bound `StubPredictor.learned`.
10. **Auto-caps/punct/space**: sentence-start cap via `getTextBeforeCursor`, double-space→period, key-`1` punct cycle.
11. **Break `:ime → :sync`**: move sync behind `:app` or an interface module so the IME process drops Room/Work/DataStore. Biggest single lean win.
12. **Gestures or 12-key row**: swipe-left delete / swipe-up space / long-press symbols, or the SPEC §0 `[Sym|Shift|Space|⌫/⏎]` row; add `performHapticFeedback` + `TouchDelegate` hit-slop; add raw-as-typed chip + long-press pin/block/info popup.

### P2 — lean trims + chrome polish
13. Remove `appcompat` from `app/build.gradle.kts:47`; remove `kotlin-compose` plugin + `buildConfig=true` from `sync/build.gradle.kts:4,18` (no Compose/BuildConfig usage there).
14. Justify-or-drop Room/DataStore/Work until the sync transport exists; at minimum split into a `:sync-impl` flavor so the base IME APK stays thin. Enable `isMinifyEnabled + isShrinkResources` in release (`app/build.gradle.kts:19-27`).
15. Preload on bg thread: warm `CategoryRegistry` + predictor in `onCreate` via `Dispatchers.IO`; keep `KbCore.loadLibrary` off the main thread.
16. Persist + reorder tabs (`rememberSaveable` + drag-reorder + DataStore/SyncPreferences entry); promote expand-all to bottom-sheet grid with paging + search + placement popup; move QWERTY toggle to floating bottom-right per SPEC §0.
17. Fullscreen/extract: define `onEvaluateFullscreenMode` policy + extract-view configuration; add `ne`/`en` subtypes.

## Files touched by this audit
- Created: `android/LEAN_QWERTY_AUDIT.md` (this file). No other files modified.
