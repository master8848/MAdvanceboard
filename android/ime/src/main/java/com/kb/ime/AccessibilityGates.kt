package com.kb.ime

import android.accessibilityservice.AccessibilityServiceInfo
import android.content.Context
import android.provider.Settings
import android.view.accessibility.AccessibilityManager

/**
 * Accessibility gate (plan 01 §Discoverability + accessibility).
 *
 * When TalkBack / ExploreByTouch (touch exploration), SwitchAccess, or
 * VoiceAccess is active, Pad flings are disabled: [isFlingsAllowed] returns
 * false and the IME must show the focusable 48dp fallback button cluster
 * (Delete, Space, Accept-#1, Next-Category, QWERTY, Sym) instead.
 *
 * Detection failures fail OPEN for taps (gestures stay enabled) and report
 * the cause via the returned [GateResult.reason], so the Tuning screen can
 * show why the fallback bar appeared instead of silently hiding gestures.
 */
object AccessibilityGates {

    data class GateResult(
        val flingsAllowed: Boolean,
        val touchExploration: Boolean,
        val switchOrVoiceAccess: Boolean,
        val reason: String? = null
    )

    fun evaluate(context: Context): GateResult {
        try {
            val am = context.getSystemService(Context.ACCESSIBILITY_SERVICE)
                as? AccessibilityManager
                ?: return GateResult(true, false, false, "no AccessibilityManager")
            val touch = try {
                am.isTouchExplorationEnabled
            } catch (e: Exception) {
                return GateResult(true, false, false, "touch-exploration probe failed: ${e.message}")
            }
            val switchVoice = try {
                isSwitchOrVoiceAccessOn(context, am)
            } catch (e: Exception) {
                return GateResult(!touch, touch, false, "switch/voice probe failed: ${e.message}")
            }
            val allowed = !touch && !switchVoice
            val reason = when {
                touch -> "TalkBack/ExploreByTouch on: Pad flings disabled, fallback buttons shown"
                switchVoice -> "SwitchAccess/VoiceAccess on: Pad flings disabled, fallback buttons shown"
                else -> null
            }
            return GateResult(allowed, touch, switchVoice, reason)
        } catch (e: Exception) {
            return GateResult(true, false, false, "accessibility probe failed: ${e.message}")
        }
    }

    private fun isSwitchOrVoiceAccessOn(
        context: Context,
        am: AccessibilityManager
    ): Boolean {
        // Enabled-service list check (SwitchAccess / VoiceAccess package hints).
        val enabled = try {
            am.getEnabledAccessibilityServiceList(AccessibilityServiceInfo.FEEDBACK_ALL_MASK)
        } catch (_: Exception) {
            emptyList()
        }
        if (enabled.any { svc ->
            val id = (svc.resolveInfo?.serviceInfo?.packageName ?: "").lowercase()
            "switchaccess" in id || "voiceaccess" in id
        }) return true
        // System settings switch-access tokens as a second signal.
        return try {
            val services = Settings.Secure.getString(
                context.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES
            )?.lowercase().orEmpty()
            "switchaccess" in services || "voiceaccess" in services
        } catch (_: Exception) {
            false
        }
    }
}
