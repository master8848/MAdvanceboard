package com.kb.app

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

/** 3-step onboarding wizard: enable IME → select IME → done. */
class OnboardingActivity : ComponentActivity() {

    companion object {
        fun intent(context: Context): Intent =
            Intent(context, OnboardingActivity::class.java)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                OnboardingScreen()
            }
        }
    }
}

@Composable
fun OnboardingScreen() {
    val context = LocalContext.current
    var step by remember { mutableIntStateOf(0) }

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp)
    ) {
        Text("Enable 9-Key Keyboard (step ${step + 1} of 3)")
        when (step) {
            0 -> {
                Text("1. Tap below, then enable “9-Key Keyboard” in system settings.")
                Button(onClick = {
                    context.startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                    step = 1
                }) { Text("Open input-method settings") }
            }
            1 -> {
                Text("2. Show the input-method picker and select “9-Key Keyboard”.")
                Button(onClick = {
                    (context as? OnboardingActivity)?.window?.decorView?.let {
                        (context.getSystemService(Context.INPUT_METHOD_SERVICE)
                            as android.view.inputmethod.InputMethodManager)
                            .showInputMethodPicker()
                    }
                    step = 2
                }) { Text("Show picker") }
            }
            else -> {
                Text("3. Done! Long-press candidates for pin / block / info.")
                Button(onClick = { (context as? ComponentActivity)?.finish() }) {
                    Text("Finish")
                }
            }
        }
    }
}
