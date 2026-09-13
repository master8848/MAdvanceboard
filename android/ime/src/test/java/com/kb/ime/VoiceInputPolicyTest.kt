package com.kb.ime

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit-provable half of plan/15: mic visibility, learn gate, airplane
 * retry without re-speak, zero residue on close.
 *
 * The `SpeechRecognizer` service wiring is NOT covered here (no device,
 * no API stubs) — Friend-ready #6 stays BLOCKED until that lands.
 */
class VoiceInputPolicyTest {

    @Test
    fun micHidden_password_numbers_phone_datetime() {
        assertTrue(VoiceInputPolicy.shouldShowMic(VoiceInputPolicy.FieldKind.TEXT))
        assertFalse(VoiceInputPolicy.shouldShowMic(VoiceInputPolicy.FieldKind.PASSWORD))
        assertFalse(VoiceInputPolicy.shouldShowMic(VoiceInputPolicy.FieldKind.NUMBER))
        assertFalse(VoiceInputPolicy.shouldShowMic(VoiceInputPolicy.FieldKind.PHONE))
        assertFalse(VoiceInputPolicy.shouldShowMic(VoiceInputPolicy.FieldKind.DATETIME))
    }

    @Test
    fun learnGate_passwordAndIncognitoNeverLearn() {
        assertTrue(VoiceInputPolicy.learnAllowed(password = false, incognito = false))
        assertFalse(VoiceInputPolicy.learnAllowed(password = true, incognito = false))
        assertFalse(VoiceInputPolicy.learnAllowed(password = false, incognito = true))
        assertFalse(VoiceInputPolicy.learnAllowed(password = true, incognito = true))
    }

    @Test
    fun airplaneRetry_reusesText_noRespeak() {
        val s = VoiceInputPolicy.VoiceSession()
        s.onPartial("hello wo")
        s.onFinal("hello world")
        val action = VoiceInputPolicy.onFailure(s, VoiceInputPolicy.Failure.NETWORK)
        assertTrue(action is VoiceInputPolicy.RetryAction.RetryOffline)
        assertEquals("hello world", (action as VoiceInputPolicy.RetryAction.RetryOffline).keptText)
        // Partial-only session still retries without re-speaking from zero.
        val p = VoiceInputPolicy.VoiceSession()
        p.onPartial("namas")
        val retry = VoiceInputPolicy.onFailure(p, VoiceInputPolicy.Failure.NETWORK)
        assertEquals("namas", (retry as VoiceInputPolicy.RetryAction.RetryOffline).keptText)
    }

    @Test
    fun noMatch_keepsTail_busy_recreates() {
        val s = VoiceInputPolicy.VoiceSession()
        s.onPartial("hel")
        assertTrue(
            VoiceInputPolicy.onFailure(s, VoiceInputPolicy.Failure.NO_MATCH)
                is VoiceInputPolicy.RetryAction.RespeakTail
        )
        val busy = VoiceInputPolicy.onFailure(s, VoiceInputPolicy.Failure.BUSY)
        assertTrue(busy is VoiceInputPolicy.RetryAction.RecreateThenRetry)
        assertEquals("hel", (busy as VoiceInputPolicy.RetryAction.RecreateThenRetry).keptText)
    }

    @Test
    fun close_wipesZeroResidue() {
        val s = VoiceInputPolicy.VoiceSession()
        s.onPartial("partial")
        s.onFinal("final")
        s.onRetry()
        s.close()
        assertEquals("", s.retryText())
        assertEquals(0, s.retryCount)
    }
}
