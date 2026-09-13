# 15 — Voice Typing (System Engine Default + Optional API Engine, No Re-Speak)

Plan only, no code. Mic is an action (like `01`), not typing. Same privacy as keys.

## 1. Engine choice (what Gboard uses, best-practice pattern)

- **Default = in-IME `SpeechRecognizer`, zero config (AOSP/Gboard pattern).** `createSpeechRecognizer(ctx)` → system default `RecognitionService` (Google on-device + cloud); API 31+ prefer `createOnDeviceSpeechRecognizer` when `Prefer on-device` ON. Never `ACTION_RECOGNIZE_SPEECH` intent (needs Activity, kills IME window) and never `VoiceInteractionService` (assistant, not dictation). Never start mic foreground-service from IME (Android 14+ ban) — system service holds mic via Binder.
- Gate every start: `isRecognitionAvailable()` (+ API 33 `checkRecognitionSupport` / `isOnDeviceRecognitionAvailable`); missing service → disabled mic, not crash. `EXTRA_LANGUAGE` from active tab, `EXTRA_PARTIAL_RESULTS=true`, `EXTRA_MAX_RESULTS=1-5`, `EXTRA_PREFER_OFFLINE` per toggle. `triggerModelDownload()` on `LANGUAGE_NOT_SUPPORTED`.
- Lifecycle: `setRecognitionListener` on main thread, `stopListening/cancel` before restart (else `ERROR_RECOGNIZER_BUSY`), `destroy()` in `onFinishInputView/onDestroy`.
- **Optional = user API STT engine.** Settings app → `Voice → Engine: [System default | Custom API]` + endpoint/key in `EncryptedSharedPreferences` (`MasterKey.AES256_GCM`), never plain prefs/log. This path owns mic via `AudioRecord(VOICE_RECOGNITION)` + VAD, so it can buffer/replay. `INTERNET` declared only for this path.
- **`Prefer on-device only` toggle** (default OFF). ON: system `PREFER_OFFLINE=true`, API path hidden. OFF: allow network.

## 2. No re-speak on failure (differs per engine — OS owns mic)

- **System path: no audio replay possible (OS owns mic).** No-re-speak here = keep last `onPartialResults` / `UNSTABLE_TEXT` + `RESULTS_RECOGNITION` text, so Retry-offline reuses text and user only re-speaks the missing tail. Flow: `ERROR_NETWORK/SERVER` → `"Network not available — try with OS default/offline"` + `[Retry offline]` (`PREFER_OFFLINE=true`, same text kept); `NO_MATCH/SPEECH_TIMEOUT` → `"Didn't catch that"` + keep last partial visible.
- **API path (own `AudioRecord`): true buffered replay.** Keep RAM ring + file spill (see §4) so `[Retry same]` / `[Use OS default]` re-POSTs the same PCM without re-speaking.
- Error mapping: `BUSY/TOO_MANY/SERVER_DISCONNECTED` → `destroy()+recreate` before restart; `INSUFFICIENT_PERMISSIONS` → rationale (see §5); `LANGUAGE_NOT_SUPPORTED` → model-download prompt.
- API path always offers one-tap fallback to System; System path offers offline retry.

## 3. Live when available, batch when not (both must work)

- **Capability flag, not two commit paths.** System: assume batch until first `onPartialResults` proves live (server may ignore `EXTRA_PARTIAL_RESULTS`); API: `supportsLive=true` only for WS/gRPC-stream/SSE. Default `false`.
- **Live mode:** render interim grey + confirmed black (`RESULTS_RECOGNITION` + `UNSTABLE_TEXT`, `CONFIDENCE_SCORES`, API 33 `onSegmentResults`). Tap stabilised word = early pick + keep listening. Same final `commitText/setComposingText` + single-undo as batch.
- **Batch fallback:** waveform + elapsed + `Listening…`/`Working…`, transcribe on stop (API: chunked `~100 ms/6400 B` stream, batch POST only as fallback).
- Detection: no partial in first `2 s` → auto-drop to batch UI, no error, no re-speak.

## 4. Buffer till keyboard closed, spill to drive if too long (bounded)

- **System path: no audio kept (OS streams).** Only text partials retained. Nothing to spill.
- **API path (`AudioRecord`): RAM first, cache-file spill when long.** Ring `~10 s (~320 KB)` in RAM; if `>10-15 s` or retry needed, spill to `cacheDir/stt_*.ogg` (`MODE_PRIVATE`, never external/media). Format 16 kHz mono 16-bit PCM (`~32 KB/s`, `~1.9 MB/min`).
- **Caps: 60-120 s or 2-5 MB per utterance, then auto-stop + finalize** (segmented-session restart for continuous dictation, Gboard pattern). Chunked streaming `~100 ms / 6400 B` (OkHttp chunked / gRPC bidi); whole-file POST only as batch fallback.
- **Lifecycle = keyboard session.** `delete()` in `finally` after transcribe + sweep `cacheDir/stt_*` on `onFinishInputView/onDestroy/onCancel`. `allowBackup=false` exclusion so audio never backed up. Never SQLite / session log / sync / learn — same rule as `12` snippet buffer.
- Final commit inserts at cursor, single undo. No background thread after IME close; cancel `SpeechRecognizer` / close WS + release buffer in `onFinishInput`.

## 5. Privacy / gates (reuse `11` + `12`)

- Mic **hidden** in `password` fields. In `incognito`: mic visible but transcript inserts without learning (same `learnNow=!password&&!incognito` gate as keys).
- `numbers` tab: mic hidden (never voice-digits into PIN/OTP).
- Voice commits never enter `personal` / `bigrams` unless `learnNow` passes; voice audio never logged (only `action=voice_accepted/voice_retried` counts, no text/audio in log).
- Permissions: `RECORD_AUDIO` declared, requested from settings `Activity` only (`RequestPermission` contract) — IME service never prompts directly; if denied, mic disabled. `INTERNET` only for API path. Incognito/password signal = `IME_FLAG_NO_PERSONALIZED_LEARNING` + password `inputType` → hide mic (Gboard parity).

## 6. UI (minimal)

- Mic key on toolbar (near QWERTY FAB, per `01` zone rules — not a Pad fling). Tap = start/stop. Waveform + elapsed `0:00/1:30` always; interim text line only in live mode. Error row with retry buttons above. No auto-commit: user taps a partial/final candidate to insert.
- Accessibility: mic is a 48 dp button, TalkBack-labelled; no gesture-only path.

## 7. Budgets + acceptance

- No bundled model, no new engine — OS/API does the work, so keyboard CPU/battery stays at `06` baseline. Only cost is RAM ring (`~320 KB`) + spill file (`≤5 MB`) + streaming POST on API path.
- Accept: (a) airplane-mode API failure → one-tap OS-default retry succeeds without re-speaking; (b) 120 s speech stops cleanly, transcribes, buffer + file freed on keyboard close (Profiler + `cacheDir` sweep shows 0 residual); (c) audit: zero audio/text on disk after close, zero voice learns in incognito/password; (d) live-capable engine shows interim <1 s, batch-only engine still commits correctly with waveform-only UI.
