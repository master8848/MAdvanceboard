package com.kb.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.kb.sync.SyncPreferences
import com.kb.sync.SyncWorker
import kotlinx.coroutines.launch

/** Host settings: tuning placeholder, pack list, sync opt-in. */
class SettingsActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                SettingsScreen()
            }
        }
    }
}

@Composable
fun SettingsScreen() {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val optedIn by SyncPreferences.optedIn(context).collectAsState(initial = false)

    Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
        Text("9-Key Keyboard Settings", style = MaterialTheme.typography.headlineSmall)
        Text("Tuning (advanced weights TBD)", modifier = Modifier.padding(top = 16.dp))
        Text("Packs: en-base, ne-base, numbers, js, rust, html, emoji, math")
        Switch(
            checked = optedIn,
            onCheckedChange = { value ->
                scope.launch {
                    SyncPreferences.setOptedIn(context, value)
                    if (value) SyncWorker.schedule(context) else SyncWorker.cancel(context)
                }
            }
        )
        Text(if (optedIn) "Sync: ON (opt-in)" else "Sync: OFF")
        Button(onClick = { context.startActivity(OnboardingActivity.intent(context)) }) {
            Text("Open enable-IME wizard")
        }
    }
}
