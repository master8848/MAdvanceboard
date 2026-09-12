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
import com.kb.ime.CustomPackStore
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
        Text("Packs: General (words), NE (ne), numbers, js, rust, html, emoji, math")
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
        LayoutSettingsSection()
        DictStackSection()
        CustomCategorySection()
        GestureTuningSection()
        ClipboardSettingsSection()
    }
}

/**
 * Settings → Layout (plan 02): global default (t9-9 / t9-12 / t9-16) plus
 * the per-tab override list (e.g. General→t9-9, NE→t9-16). Persisted by
 * [LayoutStore] (same prefs file the IME service resolves per
 * `active_tab`); changing a layout takes effect on the next tab switch
 * or field start — labels re-encode from the newly loaded spec.
 * Validation failures surface as an explicit Toast, never a silent keep.
 */
@Composable
fun LayoutSettingsSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }

    fun readGlobal(): String = try {
        LayoutStore.global(context)
    } catch (e: Exception) {
        error = "Layout prefs unreadable, showing default: ${e.message}"
        com.kb.ime.DEFAULT_LAYOUT_ID
    }

    fun readOverrides(): Map<String, String> = try {
        LayoutStore.overrides(context)
    } catch (e: Exception) {
        error = "Layout prefs unreadable: ${e.message}"
        emptyMap()
    }

    var global by remember(savedTick) { mutableStateOf(readGlobal()) }
    var overrides by remember(savedTick) { mutableStateOf(readOverrides()) }

    Text(
        "Layout", style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier.padding(top = 24.dp)
    )
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    Text("Global default", style = MaterialTheme.typography.titleMedium)
    Row(modifier = Modifier.fillMaxWidth()) {
        SUPPORTED_LAYOUT_IDS.forEach { id ->
            val short = id.removePrefix("t9-")
            TextButton(onClick = {
                try {
                    LayoutStore.setGlobal(context, id)
                    global = id
                    error = null
                } catch (e: Exception) {
                    error = "Save failed: ${e.message}"
                    Toast.makeText(context, "Layout not saved: ${e.message}", Toast.LENGTH_LONG).show()
                }
            }) { Text(if (id == global) "[$short]" else short) }
        }
    }
    Text(
        "Applies to every tab without an override below. " +
            "NE defaults to t9-16 (finer splits) until proven otherwise.",
        style = MaterialTheme.typography.labelSmall
    )

    Text("Per-tab overrides", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp))
    LayoutStore.KNOWN_CATS.forEach { (cat, label) ->
        val current = overrides[cat]
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                "$label${if (cat != label) " ($cat)" else ""}",
                modifier = Modifier
                    .weight(1f)
                    .padding(vertical = 12.dp)
            )
            TextButton(onClick = {
                try {
                    LayoutStore.clearCatLayout(context, cat)
                    overrides = overrides - cat
                    error = null
                } catch (e: Exception) {
                    error = "Clear failed: ${e.message}"
                }
            }) { Text(if (current == null) "[auto]" else "auto") }
            SUPPORTED_LAYOUT_IDS.forEach { id ->
                val short = id.removePrefix("t9-")
                TextButton(onClick = {
                    try {
                        LayoutStore.setCatLayout(context, cat, id)
                        overrides = overrides + (cat to id)
                        error = null
                    } catch (e: Exception) {
                        error = "Save failed: ${e.message}"
                        Toast.makeText(context, "Layout not saved: ${e.message}", Toast.LENGTH_LONG).show()
                    }
                }) { Text(if (id == current) "[$short]" else short) }
            }
        }
    }
    Button(onClick = {
        // Re-read from disk so external (IME-process) writes become visible.
        savedTick++
        global = readGlobal()
        overrides = readOverrides()
    }) { Text("Refresh layout settings") }
}

/**
 * Settings → Custom categories (user packs, plan/08): create a new tab
 * from a pasted/imported wordlist, toggle it, export/import the set.
 * There is no built-in names starter list (import-only by design — see
 * `docs/CUSTOM_CATEGORIES.md`). Every save/import validates loudly:
 * the exact per-row errors render inline and nothing is stored unless
 * the whole pack validates.
 */
@Composable
fun CustomCategorySection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }
    var id by remember { mutableStateOf("") }
    var title by remember { mutableStateOf("") }
    var priorityText by remember { mutableStateOf("50") }
    var wordlist by remember { mutableStateOf("") }
    var importText by remember { mutableStateOf("") }
    var exportText by remember { mutableStateOf<String?>(null) }

    fun readPacks() = try {
        CustomPackStore.list(context)
    } catch (e: Exception) {
        error = "Custom packs unreadable: ${e.message}"
        emptyList()
    }

    var packs by remember(savedTick) { mutableStateOf(readPacks()) }

    Text(
        "Custom categories", style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier.padding(top = 24.dp)
    )
    Text(
        "New tab from your own wordlist (e.g. a names list). " +
            "One `word [freq]` per line, `#` comments allowed. " +
            "New tabs isolate to their own words; learned words overlay " +
            "under the tab. Takes effect in the engine after the keyboard restarts.",
        style = MaterialTheme.typography.labelSmall
    )
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    packs.forEach { pack ->
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                "${pack.title} (${pack.id}, prio ${pack.priority})",
                modifier = Modifier
                    .weight(1f)
                    .padding(vertical = 12.dp)
            )
            Switch(
                checked = pack.enabled,
                onCheckedChange = { enabled ->
                    try {
                        CustomPackStore.setEnabled(context, pack.id, enabled)
                        error = null
                        savedTick++
                        packs = readPacks()
                    } catch (e: Exception) {
                        error = "Toggle failed: ${e.message}"
                    }
                }
            )
            TextButton(onClick = {
                try {
                    CustomPackStore.remove(context, pack.id)
                    error = null
                    savedTick++
                    packs = readPacks()
                } catch (e: Exception) {
                    error = "Delete failed: ${e.message}"
                }
            }) { Text("Delete") }
        }
    }

    Text("New category", style = MaterialTheme.typography.titleMedium)
    androidx.compose.material3.OutlinedTextField(
        value = id, onValueChange = { id = it },
        label = { Text("id (e.g. names)") },
        modifier = Modifier.fillMaxWidth()
    )
    androidx.compose.material3.OutlinedTextField(
        value = title, onValueChange = { title = it },
        label = { Text("title (e.g. Names)") },
        modifier = Modifier.fillMaxWidth()
    )
    androidx.compose.material3.OutlinedTextField(
        value = priorityText, onValueChange = { priorityText = it },
        label = { Text("priority 10-90") },
        modifier = Modifier.fillMaxWidth()
    )
    androidx.compose.material3.OutlinedTextField(
        value = wordlist, onValueChange = { wordlist = it },
        label = { Text("wordlist (word [freq] per line)") },
        minLines = 4,
        modifier = Modifier.fillMaxWidth()
    )
    Button(onClick = {
        try {
            val priority = priorityText.trim().toIntOrNull()
                ?: throw IllegalArgumentException("priority \"$priorityText\" is not a number (10-90)")
            CustomPackStore.save(context, id.trim(), title.trim(), wordlist, priority)
            error = null
            id = ""
            title = ""
            wordlist = ""
            savedTick++
            packs = readPacks()
        } catch (e: Exception) {
            error = "Save failed: ${e.message}"
        }
    }) { Text("Save category") }

    Text("Export / import", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp))
    Row(modifier = Modifier.fillMaxWidth()) {
        Button(onClick = {
            try {
                exportText = CustomPackStore.exportJson(context)
                error = null
            } catch (e: Exception) {
                error = "Export failed: ${e.message}"
            }
        }) { Text("Export") }
        TextButton(onClick = {
            try {
                val imported = CustomPackStore.importJson(context, importText)
                error = null
                importText = ""
                savedTick++
                packs = readPacks()
                Toast.makeText(context, "Imported ${imported.size} pack(s)", Toast.LENGTH_LONG).show()
            } catch (e: Exception) {
                error = "Import failed: ${e.message}"
            }
        }) { Text("Import pasted JSON") }
    }
    exportText?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
    androidx.compose.material3.OutlinedTextField(
        value = importText, onValueChange = { importText = it },
        label = { Text("paste export JSON to import") },
        minLines = 2,
        modifier = Modifier.fillMaxWidth()
    )
    Button(onClick = {
        savedTick++
        packs = readPacks()
        error = null
    }) { Text("Refresh custom categories") }
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
            error = "T9 coach will replay on next input view"
        } catch (e: Exception) {
            error = "Coach replay failed: ${e.message}"
        }
    }) { Text("Replay T9 coach") }

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
