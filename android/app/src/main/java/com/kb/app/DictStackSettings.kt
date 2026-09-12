package com.kb.app

import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.kb.ime.CustomPackStore
import com.kb.ime.DictStackOrder
import com.kb.ime.PackOrderStore
import com.kb.ime.StackEngineCaps
import com.kb.ime.StackSlot
import kotlin.math.abs

/**
 * Settings → Dictionary stack (plan/08): the suggest priority order as a
 * reorderable list — top = higher priority within 10-90. Base (0) and
 * ★personal (100) render as fixed pins (info only, never moved or
 * toggled). Every movable row shows enable toggle, priority, layout, and
 * learn badges.
 *
 * Reorder = reassign priorities ([DictStackOrder.respace]) + engine
 * `rebuild_index()` at the next init: there is no live reorder FFI (see
 * [StackEngineCaps]), so every save names the restart requirement
 * explicitly instead of pretending the running keyboard re-sorted.
 * Reorder two ways: long-press-drag the `≡` handle, or the Up/Down
 * buttons (the accessible path — identical operation).
 */
@Composable
fun DictStackSection() {
    val context = LocalContext.current
    var error by remember { mutableStateOf<String?>(null) }
    var notice by remember { mutableStateOf<String?>(null) }
    var tick by remember { mutableStateOf(0) }

    fun load(): List<StackSlot> = try {
        DictStackOrder.assembleSlots(context)
    } catch (e: Exception) {
        error = "Dictionary stack unreadable: ${e.message}"
        emptyList()
    }

    val slots = remember(tick) { load() }
    val byId = remember(slots) { slots.associateBy { it.id } }
    val customIds = remember(slots) { slots.filter { it.custom }.map { it.id }.toSet() }
    val orderIds = remember(slots) {
        mutableStateListOf<String>().apply {
            addAll(DictStackOrder.sortMovable(slots).map { it.id })
        }
    }
    val rowHeights = remember(slots) { mutableStateMapOf<String, Int>() }

    val capsNote = remember(tick) {
        try {
            if (StackEngineCaps.setCatEnabledExported) {
                "Engine note: live stack FFI present."
            } else {
                "Engine note: live toggle/reorder FFI is not exported " +
                    "(Predictor.set_cat_enabled missing from the UniFFI bindings) — " +
                    "changes below apply on keyboard restart, when packs " +
                    "reinstall in the new order."
            }
        } catch (e: Exception) {
            "Engine bindings check failed: ${e.message}"
        }
    }

    fun persistOrder(ids: List<String>) {
        try {
            DictStackOrder.persistPriorities(context, DictStackOrder.respace(ids), customIds)
            notice = "Order saved (priorities reassigned ${DictStackOrder.PRIORITY_MAX}…${DictStackOrder.PRIORITY_MIN}). " +
                "Engine applies it on keyboard restart."
            error = null
            tick++
        } catch (e: Exception) {
            error = "Reorder failed: ${e.message}"
        }
    }

    fun toggle(slot: StackSlot, enabled: Boolean) {
        try {
            if (slot.custom) CustomPackStore.setEnabled(context, slot.id, enabled)
            else PackOrderStore.setEnabled(context, slot.id, enabled)
            notice = if (slot.enabled == enabled) {
                "No change."
            } else {
                "Saved \"${slot.id}\" ${if (enabled) "enabled" else "disabled"}. " +
                    StackEngineCaps.liveToggleError(slot.id)
            }
            error = null
            tick++
        } catch (e: Exception) {
            error = "Toggle failed: ${e.message}"
        }
    }

    Text(
        "Dictionary stack", style = MaterialTheme.typography.headlineSmall,
        modifier = Modifier.padding(top = 24.dp)
    )
    Text(
        "Suggest priority order (top = wins ties). Drag the ≡ handle or " +
            "use Up/Down — both reassign priorities 10-90. Export/import " +
            "(Custom categories below) round-trips this order via the " +
            "`stack` section.",
        style = MaterialTheme.typography.labelSmall
    )
    Text(capsNote, style = MaterialTheme.typography.labelSmall)
    error?.let {
        Text(it, color = MaterialTheme.colorScheme.error)
    }
    notice?.let {
        Text(it, style = MaterialTheme.typography.labelSmall)
    }

    // Fixed pins: base bottom + personal top (info only).
    slots.filter { it.fixed }.forEach { slot ->
        Row(modifier = Modifier.fillMaxWidth()) {
            Text(
                "${slot.title} — prio ${slot.priority}, fixed pin",
                style = MaterialTheme.typography.labelMedium,
                modifier = Modifier
                    .weight(1f)
                    .padding(vertical = 8.dp)
            )
        }
    }

    LazyColumn(modifier = Modifier.fillMaxWidth()) {
        items(orderIds.toList(), key = { it }) { id ->
            val slot = byId[id] ?: return@items
            StackOrderRow(
                slot = slot,
                rowHeightPx = { rowHeights[id] ?: 0 },
                onHeight = { rowHeights[id] = it },
                onDragStep = { step ->
                    val idx = orderIds.indexOf(id)
                    val to = idx + step
                    if (idx >= 0 && to in orderIds.indices) {
                        orderIds.removeAt(idx)
                        orderIds.add(to, id)
                        true
                    } else {
                        false
                    }
                },
                onDragEnd = { persistOrder(orderIds.toList()) },
                onMoveUp = {
                    persistOrder(DictStackOrder.moveOrder(orderIds.toList(), id, -1))
                },
                onMoveDown = {
                    persistOrder(DictStackOrder.moveOrder(orderIds.toList(), id, 1))
                },
                onToggle = { toggle(slot, !slot.enabled) }
            )
        }
    }

    Button(onClick = {
        tick++
        notice = null
        error = null
    }) { Text("Refresh stack order") }
}

/**
 * One movable stack row: drag handle, title + badges, priority, toggle,
 * Up/Down. Long-press-drag accumulates vertical travel in units of this
 * row's measured height and live-moves the row; release persists.
 */
@Composable
private fun StackOrderRow(
    slot: StackSlot,
    rowHeightPx: () -> Int,
    onHeight: (Int) -> Unit,
    /** Move this row one [step] (±1) in the live list; true when moved. */
    onDragStep: (step: Int) -> Boolean,
    onDragEnd: () -> Unit,
    onMoveUp: () -> Unit,
    onMoveDown: () -> Unit,
    onToggle: () -> Unit
) {
    var dragAccum by remember(slot.id) { mutableFloatStateOf(0f) }
    var moved by remember(slot.id) { mutableStateOf(false) }
    val badges = buildString {
        append("prio ${slot.priority}")
        slot.layoutId?.let { append(" · $it") }
        append(if (slot.learns) " · learns" else " · no-learn")
        if (slot.custom) append(" · custom")
    }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .onSizeChanged { onHeight(it.height) }
    ) {
        Text(
            text = "≡",
            modifier = Modifier
                .padding(horizontal = 8.dp, vertical = 12.dp)
                .pointerInput(slot.id) {
                    detectDragGesturesAfterLongPress(
                        onDragStart = {
                            dragAccum = 0f
                            moved = false
                        },
                        onDragEnd = {
                            if (moved) onDragEnd()
                        },
                        onDragCancel = { dragAccum = 0f },
                        onDrag = { change, dragAmount ->
                            change.consume()
                            val h = rowHeightPx()
                            if (h <= 0) return@detectDragGesturesAfterLongPress
                            dragAccum += dragAmount.y
                            while (abs(dragAccum) >= h) {
                                val step = if (dragAccum > 0) 1 else -1
                                if (!onDragStep(step)) break
                                dragAccum -= step * h
                                moved = true
                            }
                        }
                    )
                }
        )
        Column(modifier = Modifier.weight(1f)) {
            Text(slot.title.ifEmpty { slot.id })
            Text(slot.id, style = MaterialTheme.typography.labelSmall)
            Text(badges, style = MaterialTheme.typography.labelSmall)
        }
        TextButton(onClick = onMoveUp) { Text("Up") }
        TextButton(onClick = onMoveDown) { Text("Down") }
        Switch(checked = slot.enabled, onCheckedChange = { onToggle() })
    }
}
