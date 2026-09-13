package com.kb.app

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import androidx.activity.compose.setContent
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.kb.ime.FriendDefaults
import com.kb.ime.KbImeTheme
import com.kb.ime.ThemeStore

/**
 * Friend-ready onboarding wizard (plan/21): 5 steps, each deep-linking to
 * its section. Trust pre-frame first (the system warning scares friends),
 * then switch + board choice (QWERTY default), trial, tabs, graduation +
 * switch-back escape hatch.
 *
 * - Enable→switch in <2 min: two system taps + auto-advance (returning
 *   from system settings re-checks state and steps forward — no
 *   "did it work?" limbo).
 * - Skippable: every step has Skip; the in-keyboard coach covers tips
 *   progressively (emoji after 10 commits, voice after 20).
 */
class OnboardingActivity : AppCompatActivity() {

    companion object {
        fun intent(context: Context): Intent =
            Intent(context, OnboardingActivity::class.java)
    }

    /** Bumped in [onResume] so the screen auto-advances past done steps. */
    private var resumeTick = mutableIntStateOf(0)

    override fun onCreate(savedInstanceState: Bundle?) {
        applyStoredNightMode(this)
        super.onCreate(savedInstanceState)
        setContent {
            val storedMode = ThemeStore.mode(LocalContext.current)
            KbImeTheme(
                darkTheme = when (storedMode) {
                    ThemeStore.DARK -> true
                    ThemeStore.LIGHT -> false
                    else -> isSystemInDarkTheme()
                }
            ) {
                OnboardingScreen(resumeTick = resumeTick.intValue)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        resumeTick.intValue++
    }
}

/** True when our IME appears in the system's enabled list. */
private fun isOurImeEnabled(context: Context): Boolean = try {
    Settings.Secure.getString(context.contentResolver, Settings.Secure.ENABLED_INPUT_METHODS)
        ?.contains(context.packageName) == true
} catch (_: Exception) {
    false
}

/** True when our IME is the current input method. */
private fun isOurImeCurrent(context: Context): Boolean = try {
    Settings.Secure.getString(context.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
        ?.startsWith(context.packageName) == true
} catch (_: Exception) {
    false
}

private const val TOTAL_STEPS = 5

@Composable
fun OnboardingScreen(resumeTick: Int = 0) {
    val context = LocalContext.current
    var step by remember { mutableIntStateOf(0) }
    var done by remember { mutableStateOf(false) }

    // Auto-advance: returning from system settings steps past whatever the
    // system now reports (enable → switch → onward). Never steps back.
    LaunchedEffect(resumeTick) {
        if (done) return@LaunchedEffect
        if (isOurImeCurrent(context)) {
            if (step < 2) step = 2
        } else if (isOurImeEnabled(context)) {
            if (step < 1) step = 1
        }
    }

    fun finish() {
        (context as? OnboardingActivity)?.finish()
    }

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically
        ) {
            Text(
                if (done) "You're set 🎉" else "Get started (step ${step + 1} of $TOTAL_STEPS)",
                style = MaterialTheme.typography.titleMedium
            )
            if (!done) {
                TextButton(onClick = { done = true }) { Text("Skip") }
            }
        }
        if (!done) {
            LinearProgressIndicator(
                progress = { (step + 1) / TOTAL_STEPS.toFloat() },
                modifier = Modifier.fillMaxWidth(),
            )
        }
        when {
            done -> GraduationBody(onFinish = ::finish)
            step == 0 -> EnableBody(onOpenedSettings = { /* auto-advance on resume */ })
            step == 1 -> SwitchBody()
            step == 2 -> TrialBody()
            step == 3 -> TabsBody()
            else -> GraduationBody(onFinish = ::finish)
        }
        if (!done && step in 0..3) {
            Row(modifier = Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                TextButton(onClick = { done = true }) { Text("Skip setup") }
                // Manual override: auto-advance covers the normal path
                // (resume re-checks system state); this covers locked-down
                // devices where the secure-settings read fails.
                Button(onClick = { step++ }) { Text(if (step == 0) "I enabled it →" else "Next") }
            }
        }
    }
}

/**
 * Step 1 — Enable (deep-link `ACTION_INPUT_METHOD_SETTINGS`).
 * Pre-frames the scary system warning first: Android shows it for ALL
 * keyboards; ours has no network permission (verify in Settings→Apps).
 */
@Composable
private fun EnableBody(onOpenedSettings: () -> Unit) {
    val context = LocalContext.current
    Text("First, the scary part — explained honestly:")
    Text(
        "Android will warn this keyboard \"may collect everything you type\". " +
            "It says that for EVERY keyboard. Ours has no network permission, " +
            "so your typing cannot leave the phone — verify in Settings → Apps.",
        style = MaterialTheme.typography.bodyMedium
    )
    Button(onClick = {
        context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
        onOpenedSettings()
    }) { Text("Open input-method settings") }
    TextButton(onClick = {
        try {
            context.startActivity(
                Intent(
                    Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
                    android.net.Uri.parse("package:${context.packageName}")
                )
            )
        } catch (_: Exception) {
        }
    }) { Text("Verify permissions (no network)") }
    Text(
        "Tick the keyboard ON, then come back here — this screen advances by itself.",
        style = MaterialTheme.typography.labelSmall
    )
}

/**
 * Step 2 — Switch (`showInputMethodPicker`) + board choice. Friends get
 * QWERTY first (plan/24 §1); 9-key is one toggle away, not the greeting.
 */
@Composable
private fun SwitchBody() {
    val context = LocalContext.current
    var qwerty by remember { mutableStateOf(FriendDefaults.qwertyDefault(context)) }
    Text("Now switch to it — and pick your board:")
    Button(onClick = {
        (context as? OnboardingActivity)?.window?.decorView?.let {
            (context.getSystemService(Context.INPUT_METHOD_SERVICE)
                as android.view.inputmethod.InputMethodManager)
                .showInputMethodPicker()
        }
    }) { Text("Show picker — pick this keyboard") }
    Row(modifier = Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text("Start with familiar QWERTY")
            Text(
                "OFF = 9-key pad first (research opt-in). Switch anytime with ⌨.",
                style = MaterialTheme.typography.labelSmall
            )
        }
        Switch(
            checked = qwerty,
            onCheckedChange = {
                try {
                    FriendDefaults.setQwertyDefault(context, it)
                    qwerty = it
                } catch (_: Exception) {
                }
            }
        )
    }
}

/**
 * Step 3 — Trial (`43556→hello`, live trial row). Tap-once-per-letter,
 * max one screen, keep the keyboard View for latency (this is a plain
 * text field — the real trial happens in any app).
 */
@Composable
private fun TrialBody() {
    var trial by remember { mutableStateOf("") }
    Text("Try it: tap once per letter, the strip guesses the word.")
    Text(
        "On the 9-key pad: 4-3-5-5-6 suggests \"hello\". This is NOT " +
            "multi-tap — never press repeatedly, never wait.",
        style = MaterialTheme.typography.bodyMedium
    )
    OutlinedTextField(
        value = trial,
        onValueChange = { trial = it },
        label = { Text("Trial row — type here") },
        modifier = Modifier.fillMaxWidth()
    )
}

/**
 * Step 4 — Tabs = dictionaries. Friend-defaults: only EN/ने/😀/123
 * visible; code tabs hide behind "Show code tabs" (plan/19, 20).
 */
@Composable
private fun TabsBody() {
    val context = LocalContext.current
    Text("Tabs are dictionaries:")
    Text(
        "E = English, ने = Nepali, 🙂 = emoji, 123 = numbers. Swipe the " +
            "tab row to switch; long-press a tab for its pack info. Code " +
            "tabs stay hidden until you ask (Settings → Dictionaries).",
        style = MaterialTheme.typography.bodyMedium
    )
    TextButton(onClick = {
        context.startActivity(
            SettingsActivity.intent(context, Sections.DICTIONARIES)
        )
    }) { Text("Open Dictionaries") }
}

/**
 * Step 5 — Graduation + switch-back escape hatch. Personal stays
 * on-device (one sentence) → WhatsApp/Viber graduation ("send 3
 * messages") → explicit switch-back so stuck reports don't become
 * support calls. Also the one-tap "report problem / switch back" path
 * (plan/24 §10).
 */
@Composable
private fun GraduationBody(onFinish: () -> Unit) {
    val context = LocalContext.current
    Text("Graduation: open a chat and send 3 messages 🎓")
    Text(
        "Your words stay on this phone — learning is local, nothing uploads. " +
            "WhatsApp, Viber, and Messenger all work the same way.",
        style = MaterialTheme.typography.bodyMedium
    )
    Text(
        "Stuck? Switch back anytime: open any text field → 🌐/picker → " +
            "pick Gboard (or your old keyboard). No uninstall needed.",
        style = MaterialTheme.typography.bodyMedium
    )
    Button(onClick = { onFinish() }) { Text("Finish") }
    TextButton(onClick = {
        context.startActivity(SettingsActivity.intent(context, Sections.HOME))
    }) { Text("Report a problem (opens Settings)") }
}
