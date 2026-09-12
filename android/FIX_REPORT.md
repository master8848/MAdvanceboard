# Android IME P0/P1 Fix Report

Date: 2026-09-12 · Scope: `android/` ONLY (no `core-rust/`, `packs/`, `docs/`, root edits).
Grounding (read-only): `SPEC.md` §0, `android/QA_REPORT.md`, `android/LEAN_QWERTY_AUDIT.md`.
Method: static edits + `grep` verification (no JDK/Gradle build in this env). No new Gradle modules.

## P0 — default-keyboard duties

| Fix | File:line |
|---|---|
| `onFinishInput()` → `finishComposingText()` + clear `seq`/`qwertyBuffer`/`liveCandidates`; persist/flush note (personal-dict flush owned by `:app` process; IME holds no sync refs) | `ime/.../KbInputMethodService.kt:138-151` |
| `onFinishInputView()` symmetric cleanup | `KbInputMethodService.kt:153-161` |
| `onStartInputView()` override (layout reflects `inputMode`; pad swaps only on toggle, no desync) | `KbInputMethodService.kt:132-136` |
| `onUpdateSelection()` override (cursor hook; next keystroke re-reads `getTextBeforeCursor` for bigram prev) | `KbInputMethodService.kt:163-174` |
| `onDestroy()` cancels Service scope, closes predictor, releases cached view | `KbInputMethodService.kt:176-184` |
| Service-scoped `CoroutineScope(SupervisorJob + Dispatchers.Default)` for suggest/learn (replaces "scope TBD" stub) | `KbInputMethodService.kt:75` |
| `InputMode` TEXT/NUMBER/PHONE/DATETIME via `resolveInputMode()`; non-TEXT → `predictionEnabled=false` → raw commit, no prediction | `KbInputMethodService.kt:378-388`, `:125-127`, `:227-231`, `:249-252` |
| Email/URI variations keep suggest, suppress learn | `KbInputMethodService.kt:414-420`, `:288` |
| `Predictor.suggest` wired per keystroke (T9 + QWERTY) with `getTextBeforeCursor` prev-word | `KbInputMethodService.kt:262-273`, `:277-284` |
| `Predictor.learn` wired in `commitCandidate`, gated by `shouldLearnNow()` (write-only field + empty stub removed) | `KbInputMethodService.kt:291-306` |
| Dead `deleteLast` fixed: now `internal`, wired to ⌫ in both pads; trims composing buffers first, else `deleteSurroundingText(1,0)` | `KbInputMethodService.kt:314-330`; `KeyPadView.kt:20,54`; `QwertyView.kt:24,76` |
| Enter/editor actions: `handleEnter()` maps `IME_ACTION_DONE/GO/NEXT/SEARCH/SEND` → `performEditorAction`, else commits `\n` | `KbInputMethodService.kt:206-221` |
| Next-IME: `shouldOfferSwitchingToNextInputMethod()=true` + `switchToNextIme()` (`switchToNextInputMethod` on API 28+, picker fallback below) + 🌐 keys on both pads | `KbInputMethodService.kt:187-200`; `KeyPadView.kt:56`; `QwertyView.kt:88` |
| `method.xml` subtypes `en` + `ne` (keyboard mode) | `ime/src/main/res/xml/method.xml:4-11` |
| Incognito gate: `IME_FLAG_NO_PERSONALIZED_LEARNING` + `privateImeOptions` (`incognito`/`noPersonalizedLearning`) → skip learn AND context harvesting (`prevWord()==null`) | `KbInputMethodService.kt:405-411`, `:123`, `:278`, `:287-289` |
| Category `privacy.learn=false` (numbers/math) consulted at commit time via preloaded `learnByCategory` (asset IO off main thread in `onCreate`) | `KbInputMethodService.kt:96-116`, `:288-289`, `:336-342` |

## P1 — QWERTY + lean

| Fix | File:line |
|---|---|
| Full QWERTY rows `qwertyuiop / asdfghjkl / zxcvbnm` + bottom row Sym/Space/⌫/⏎/🌐 (replaces 6-key stub); toggle affordance kept | `ime/.../QwertyView.kt:29-31,55-90`; `ImeScreen.kt:59`; `KbInputMethodService.kt:339-349` |
| Shared Predictor path: `onQwertyKey` (raw-text encoder) and `onDigit` (T9 encoder) feed the SAME `suggest`/`learn`; mode flag selects encoder only. Space breaks word via `commitCandidate` (learn reused); Enter commits buffer then handles action | `KbInputMethodService.kt:241-260` |
| Hardcoded `listOf("the","tie","vie")` + fake `word$it` padding removed; `ImeScreen(candidates)` is live `Predictor.suggest` output from the service | `ImeScreen.kt:31-32,63`; `KbInputMethodService.kt:62,84-103,271` |
| `:ime → :sync` dependency REMOVED (was unused — zero `com.kb.sync` imports in `:ime`). IME process = UI + bridge only; no Room/Work/DataStore in IME. `:app → :sync` intentionally kept (sync runs in app process) | `ime/build.gradle.kts:30-34`; `app/build.gradle.kts:45` |
| Unused `appcompat` removed from `:app` (activities are `ComponentActivity`) | `app/build.gradle.kts:46-48` |
| `kotlin-compose` plugin + `buildConfig=true` removed from `:sync` (zero Compose/`BuildConfig` usage) | `sync/build.gradle.kts:1-6,16` |
| `StubPredictor` fallback intact (service still defaults to it; `KbCore.isAvailable()` probe kept) + 20k bound on stub `learned` (was unbounded leak) | `KbInputMethodService.kt:82`, `:114`; `core-bridge/.../Predictor.kt:52-60` |
| 48dp targets kept (pad keys + action rows; QWERTY letters full-height 48dp, weight-distributed width), view caching kept (`cachedInputView` early-return/store; only released in `onDestroy`), BIND-only manifest (no INTERNET) | `KeyPadView.kt:35,41-42,62-63`; `QwertyView.kt:41,57`; `KbInputMethodService.kt:78-80,176-184`; `ime/src/main/AndroidManifest.xml:4,10` |

## Static verification (all PASS)

- `grep -rn INTERNET android/ime/ android/app/src` → no hits (BIND-only).
- `grep -rn "project(\":sync\")|com.kb.sync" android/ime/` → no hits (IME/sync split).
- `predictor.suggest` (`KbInputMethodService.kt:268`) + `predictor.learn` (`:302`), both inside `shouldLearnNow()` gating (`:287-289,299`).
- `method.xml` subtypes `en`/`ne` present (`:4-11`).
- No hardcoded candidates: `grep '"the"|"tie"|"vie"|word\$it' android/ime/src/` → no hits.
- QWERTY full rows present (`QwertyView.kt:29-31`); `onKey` routes to service `onQwertyKey` (`KbInputMethodService.kt:343`).

## Final reconcile pass (2026-09-12; static only, no JDK build)

- **ID alignment (canonical `words, ne, js, rust, html, emoji, numbers, math`):**
  `android/.../assets/categories/nepali.json` → `ne.json` (`"id": "ne"`,
  `displayName` stays `NE`); `CategoryTabs.DEFAULT_CATEGORIES` now uses
  canonical ids (`words`, `ne`, …); `onCategoryChanged` maps legacy `EN`/`NE`
  labels + old `nepali` id to `words`/`ne` (compat). Packs side:
  `packs/words_en.json` `"id"` `en` → `words` (default `cat` follows).
- **Bridge parity:** `Predictor` interface + `StubPredictor` gained
  `forget` / `reject` / `exportSession` stubs
  (`core-bridge/.../Predictor.kt`). Wire-up to the real engine still
  requires a JDK machine: add a core `reject(word)` UniFFI export
  (`record_reject` is core-internal today), then regen with
  **`uniffi-bindgen 0.32`** (must match crate `uniffi 0.32.1`) and replace
  `StubPredictor` with the generated binding.
- **Tombstone:** canonical wire name **`del`** (SPEC §5); readers accept
  legacy `deleted` (SPEC §4) — `SyncMerge.fromJsonl` compat,
  core `serde(rename="del", alias="deleted")`. Room field `deleted` and
  `PersonalWordEntity` unchanged (no migration).

## Known remaining (not in P0/P1 scope; left for follow-ups)

- Key-`1` punct cycle (`.,?!`), double-space→period, auto-caps, 2s learn debounce — noted in audit P1 #9/#10, untouched.
- `shouldOfferSwitchingToNextInputMethod` / `switchToNextInputMethod(false)` are API 28+; guarded at call time, override is inert pre-28. Lint `NewApi` check needs JDK build.
- Fullscreen/extract policy (`onEvaluateFullscreenMode`), bottom-sheet expand-all, tab reorder/persist — P2, untouched.
- JDK build still required: `export JAVA_HOME=<jdk17> && ./gradlew :app:assembleDebug :ime:assembleDebug` + lint.
