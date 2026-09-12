package com.kb.app

import android.widget.Toast
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.kb.ime.ClipboardHistory
import com.kb.ime.ClipboardStore

/**
 * Settings → Clipboard (plan/11 §3): retention presets + custom hours,
 * clear (pins-aware). Pills never enter: pins exempt, clear-all asks.
 *
 * Retention applies on the next sweep (every keyboard start + every
 * capture re-sweeps, so no background worker): tightening 7d→24h drops
 * expired unpinned on next open, pins untouched. Wired into
 * [SettingsScreen] as `ClipboardSettingsSection()`.
 */
@Composable
fun ClipboardSettingsSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var savedTick by remember { mutableStateOf(0) }
    var customHours by remember { mutableStateOf("") }

    fun readTtl(): Long = try {
        ClipboardStore.ttlHours(context)
    } catch (e: Exception) {
        error = "Clipboard prefs unreadable, showing default: ${e.message}"
        ClipboardHistory.DEFAULT_TTL_HOURS
    }

    var ttl by remember(savedTick) { mutableStateOf(readTtl()) }

    fun presetLabel(h: Long): String = when (h) {
        1L -> "1h"
        24L -> "24h"
        168L -> "7d"
        720L -> "30d"
        else -> "${h}h"
    }

    fun saveTtl(hours: Long) {
        try {
            ClipboardStore.setTtlHours(context, hours)
            ttl = hours
            error = null
            Toast.makeText(
                context,
                "Clipboard retention: ${presetLabel(hours)} (applies on next open)",
                Toast.LENGTH_LONG
            ).show()
        } catch (e: Exception) {
            error = "Save failed: ${e.message}"
            Toast.makeText(context, "Retention not saved: ${e.message}", Toast.LENGTH_LONG).show()
        }
    }

    fun clear(includePins: Boolean) {
        try {
            val items = ClipboardStore.load(context)
            ClipboardStore.clear(context, items, includePins)
            error = if (includePins) "Clipboard cleared (pins included)"
            else "Clipboard cleared (pins kept)"
        } catch (e: Exception) {
            error = "Clear failed: ${e.message}"
        }
    }

    Text(
        "Clipboard", style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier.padding(top = 24.dp)
    )
    Text(
        "History lives on this device only (never synced, never learned " +
            "into suggestions). Unpinned items auto-delete past retention; " +
            "pinned items persist until unpinned or deleted.",
        style = MaterialTheme.typography.labelSmall
    )
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }

    Text(
        "Retention (current: ${presetLabel(ttl)})",
        style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp)
    )
    Row(modifier = Modifier.fillMaxWidth()) {
        ClipboardHistory.TTL_PRESETS_HOURS.forEach { h ->
            TextButton(onClick = { saveTtl(h) }) {
                Text(if (h == ttl) "[${presetLabel(h)}]" else presetLabel(h))
            }
        }
    }
    Row(modifier = Modifier.fillMaxWidth()) {
        OutlinedTextField(
            value = customHours,
            onValueChange = { customHours = it },
            label = { Text("custom hours (1-720)") },
            singleLine = true,
            modifier = Modifier.weight(1f)
        )
        Button(
            onClick = {
                val hours = customHours.trim().toLongOrNull()
                if (hours == null) {
                    error = "Save failed: \"$customHours\" is not a number of hours (1-720)"
                } else {
                    saveTtl(hours)
                }
            },
            modifier = Modifier.padding(start = 8.dp, top = 8.dp)
        ) { Text("Set") }
    }

    Text("Clear", style = MaterialTheme.typography.titleMedium,
        modifier = Modifier.padding(top = 8.dp))
    Row(modifier = Modifier.fillMaxWidth()) {
        Button(onClick = { clear(includePins = false) }) { Text("Clear (keep pins)") }
        TextButton(onClick = { clear(includePins = true) }) { Text("Clear everything") }
    }
    TextButton(onClick = {
        savedTick++
        ttl = readTtl()
        error = null
    }) { Text("Refresh clipboard settings") }
}
