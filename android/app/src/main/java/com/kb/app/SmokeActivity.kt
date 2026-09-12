package com.kb.app

import android.app.Activity
import android.os.Bundle
import android.view.inputmethod.InputMethodManager
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView

/**
 * ADB smoke fixture (NOT in the launcher): a full-screen [EditText] that
 * takes focus on start so the active IME binds without manual taps.
 *
 * Launch: `adb shell am start -n com.kb.app/.SmokeActivity`
 * (package is the `:app` applicationId — see `android/app/build.gradle.kts`).
 * Then select this keyboard via
 * `adb shell ime set com.kb.ime/.KbInputMethodService` and tap the field.
 * Typing `43556` on the 9-key pad must suggest `hello` — the stub engine
 * (no learned words on a fresh install) can never produce it, so `hello`
 * proves the Rust engine path end to end.
 */
class SmokeActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(32, 64, 32, 32)
        }
        val hint = TextView(this).apply {
            setText("Smoke field — tap here, then type 43556 on the 9-key pad. Expect: hello.")
            textSize = 18f
        }
        val field = EditText(this).apply {
            setHint("type here")
            textSize = 24f
            minLines = 4
            isFocusable = true
            isFocusableInTouchMode = true
        }
        root.addView(hint)
        root.addView(field)
        setContentView(root)
        field.requestFocus()
        try {
            (getSystemService(INPUT_METHOD_SERVICE) as? InputMethodManager)
                ?.showSoftInput(field, InputMethodManager.SHOW_IMPLICIT)
        } catch (_: Exception) {
            // IME shows on field tap regardless; this is best-effort.
        }
    }
}
