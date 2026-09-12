package com.kb.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import android.widget.Toast
import com.kb.ime.GestureLog
import com.kb.ime.GestureThresholds
import com.kb.ime.GestureTuningStore
import com.kb.ime.LayoutStore
import com.kb.ime.SUPPORTED_LAYOUT_IDS
import com.kb.sync.SyncPreferences
import com.kb.sync.SyncWorker
import kotlinx.coroutines.launch

/** Host settings: gesture tuning, pack list, sync opt-in. */
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

    Column(
        modifier = Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(16.dp)
    ) {
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
        GestureTuningSection()
    }
}

/**
 * Settings → Gesture Tuning (plan 01 §Detection + §12-key override).
 * Ship defaults: fling 24dp / 200dp/s / 1.4:1 axis, word-step 20dp,
 * long-press 400ms (300–600), slop 8dp, edge 8dp. Out-of-range saves throw
 * ([GestureThresholds.init]) and surface inline instead of clamping.
 */
@Composable
fun GestureTuningSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }

    fun current(): GestureThresholds = try {
        GestureTuningStore.loadThresholds(context)
    } catch (e: Exception) {
        error = "Tuning prefs unreadable, showing defaults: ${e.message}"
        GestureThresholds()
    }

    var flingDp by remember(savedTick) { mutableFloatStateOf(current().flingThresholdDp) }
    var velocity by remember(savedTick) { mutableFloatStateOf(current().flingVelocityDpS) }
    var axis by remember(savedTick) { mutableFloatStateOf(current().axisRatio) }
    var wordStep by remember(savedTick) { mutableFloatStateOf(current().wordStepDp) }
    var longPress by remember(savedTick) { mutableFloatStateOf(current().longPressMs.toFloat()) }
    var allow12 by remember(savedTick) {
        mutableStateOf(GestureTuningStore.allowTwelveKeyFlings(context))
    }
    var terminator by remember(savedTick) {
        mutableStateOf(GestureTuningStore.codeTerminator(context))
    }
    var logStats by remember(savedTick) {
        mutableStateOf(
            GestureLog(context, onError = { error = it }).stats()
        )
    }

    Text("Gesture Tuning", style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier.padding(top = 24.dp))
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    TuningSlider("Fling distance (dp)", flingDp, 12f, 48f) { flingDp = it }
    TuningSlider("Fling velocity (dp/s)", velocity, 100f, 500f) { velocity = it }
    TuningSlider("Dominant-axis ratio", axis, 1.1f, 3f) { axis = it }
    TuningSlider("Word step (dp/word)", wordStep, 10f, 40f) { wordStep = it }
    TuningSlider("Long-press (ms)", longPress, 300f, 600f) { longPress = it }

    Row(modifier = Modifier.fillMaxWidth()) {
        Text("Allow pad flings in 12-key (thresholds +30%)",
            modifier = Modifier.weight(1f))
        Switch(checked = allow12, onCheckedChange = { allow12 = it })
    }
    Text(
        text = "Default OFF: the 12-key bottom row [Sym|Space|⌫] is authoritative. " +
            "←/↑ flings stay disabled; → accept keeps working.",
        style = MaterialTheme.typography.labelSmall
    )
    Row(modifier = Modifier.fillMaxWidth()) {
        Text("Code/math ↑ terminator", modifier = Modifier.weight(1f))
        TextButton(onClick = { terminator = " " }) {
            Text(if (terminator == " ") "[space]" else "space")
        }
        TextButton(onClick = { terminator = ";" }) {
            Text(if (terminator == ";" ) "[;]" else ";")
        }
    }
    Button(onClick = {
        try {
            val base = current()
            GestureTuningStore.saveThresholds(
                context,
                base.copy(
                    flingThresholdDp = flingDp,
                    flingVelocityDpS = velocity,
                    axisRatio = axis,
                    wordStepDp = wordStep,
                    longPressMs = longPress.toLong()
                )
            )
            GestureTuningStore.setAllowTwelveKeyFlings(context, allow12)
            GestureTuningStore.setCodeTerminator(context, terminator)
            error = null
            savedTick++
        } catch (e: Exception) {
            error = "Save failed: ${e.message}"
        }
    }) { Text("Save gesture tuning") }

    Text("Discoverability", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 16.dp))
    Button(onClick = {
        try {
            GestureTuningStore.requestCoachReplay(context)
            error = "Coach will replay on next input view"
        } catch (e: Exception) {
            error = "Coach replay failed: ${e.message}"
        }
    }) { Text("Replay gesture coach") }

    Text("False-trigger log (log.jsonl)", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 16.dp))
    if (logStats.unreadable) {
        Text("Log unreadable: ${logStats.error}",
            color = MaterialTheme.colorScheme.error)
    } else {
        Text("Events: ${logStats.events} · fling-← commits: ${logStats.flingLeftCommits} " +
            "· undone: ${logStats.undoneDeletes}")
        if (logStats.flingLeftCommits > 0) {
            val rate = 100 * logStats.undoneDeletes / logStats.flingLeftCommits
            Text("Undo rate ≈ $rate% (false-trigger estimate)",
                style = MaterialTheme.typography.labelSmall)
        }
    }
    Button(onClick = {
        val ok = GestureLog(context, onError = { error = it }).clear()
        if (ok) logStats = GestureLog(context).stats()
    }) { Text("Clear gesture log") }
}

@Composable
private fun TuningSlider(
    label: String,
    value: Float,
    min: Float,
    max: Float,
    onChange: (Float) -> Unit
) {
    Text("$label: ${"%.1f".format(value)}")
    Slider(
        value = value,
        onValueChange = onChange,
        valueRange = min..max,
        modifier = Modifier.fillMaxWidth()
    )
}
