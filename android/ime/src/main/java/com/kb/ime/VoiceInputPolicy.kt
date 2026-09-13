package com.kb.ime

/**
 * Voice typing policy contract — system-engine-first (plan/15 §1-2, §5).
 *
 * Pure JVM logic (no `android.speech` imports) so it stays unit-provable.
 * The `SpeechRecognizer` service wiring inside the IME service is NOT built
 * yet — see the follow-up note at the bottom. This contract pins the
 * behavior the wiring must obey when it lands.
 *
 * Rules:
 * - Mic is an action, not typing; same privacy as keys.
 * - Mic HIDDEN in password + number/phone/datetime fields (never
 *   voice-digits into PIN/OTP); visible in plain text only.
 * - Incognito: mic visible but transcript inserts without learning
 *   (`learnNow = !password && !incognito`, same gate as keys).
 * - System path owns no audio (OS streams): no-re-speak = keep last
 *   partial/final text so offline retry reuses text and the user only
 *   re-speaks the missing tail. Nothing to spill to disk.
 * - Zero residue: [close] wipes text; the service must also sweep any
 *   spill files + destroy() the recognizer on finish (plan/15 §4).
 *
 * FOLLOW-UP (service wiring, NOT built): `SpeechRecognizer` create /
 * listener / stopListening-cancel-before-restart / destroy() in
 * `onFinishInputView/onDestroy`, `isRecognitionAvailable()` gate,
 * `EXTRA_LANGUAGE` from active tab, `EXTRA_PARTIAL_RESULTS=true`,
 * `EXTRA_PREFER_OFFLINE` retry, `RECORD_AUDIO` via settings Activity only.
 * Friend-ready #6 stays BLOCKED until that wiring + a device run land.
 */
object VoiceInputPolicy {

    /** Field class driving mic visibility. Mirrors plan/15 §5. */
    enum class FieldKind { TEXT, PASSWORD, NUMBER, PHONE, DATETIME }

    /** Recoverable failure classes the error row must distinguish. */
    enum class Failure { NETWORK, NO_MATCH, BUSY, UNSUPPORTED, OTHER }

    /** What the error row offers. Offline retry keeps text (no re-speak). */
    sealed interface RetryAction {
        /** Keep [keptText]; retry with PREFER_OFFLINE=true, no re-speak. */
        data class RetryOffline(val keptText: String) : RetryAction
        /** Keep last partial visible; user re-speaks the missing tail only. */
        data class RespeakTail(val keptText: String) : RetryAction
        /** Recreate recognizer first (BUSY/TOO_MANY), text kept. */
        data class RecreateThenRetry(val keptText: String) : RetryAction
        /** No retry offered (e.g. permission denied → rationale path). */
        data object NoRetry : RetryAction
    }

    /** Per-utterance text holder. Text only — never audio (system path). */
    class VoiceSession {
        var lastPartial: String = ""
            private set
        var lastFinal: String = ""
            private set
        var retryCount: Int = 0
            private set

        fun onPartial(text: String) {
            lastPartial = text
        }

        fun onFinal(text: String) {
            lastFinal = text
            lastPartial = text
        }

        fun onRetry() {
            retryCount++
        }

        /** Zero-residue close: wipe everything the session held. */
        fun close() {
            lastPartial = ""
            lastFinal = ""
            retryCount = 0
        }

        /** Text an offline retry reuses without re-speaking. */
        fun retryText(): String = lastFinal.ifEmpty { lastPartial }
    }

    /** Mic visible only in plain-text fields. */
    fun shouldShowMic(kind: FieldKind): Boolean = kind == FieldKind.TEXT

    /** Learning gate shared with keys: password/incognito never learn. */
    fun learnAllowed(password: Boolean, incognito: Boolean): Boolean =
        !password && !incognito

    /** Map a recognizer failure to the error-row action (text always kept). */
    fun onFailure(session: VoiceSession, failure: Failure): RetryAction {
        val kept = session.retryText()
        return when (failure) {
            Failure.NETWORK -> RetryAction.RetryOffline(kept)
            Failure.NO_MATCH -> RetryAction.RespeakTail(kept)
            Failure.BUSY -> RetryAction.RecreateThenRetry(kept)
            Failure.UNSUPPORTED -> RetryAction.NoRetry
            Failure.OTHER -> RetryAction.RespeakTail(kept)
        }
    }
}
