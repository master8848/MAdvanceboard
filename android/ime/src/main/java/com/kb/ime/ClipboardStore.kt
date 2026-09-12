package com.kb.ime

import android.content.Context
import android.content.SharedPreferences

/**
 * Clipboard persistence + retention prefs (plan/11 §3).
 *
 * Same SharedPreferences discipline as [GestureTuningStore]: the `:ime`
 * process stays lean (no DataStore/Room/Work). The service keeps history
 * RAM-first ([KbInputMethodService.liveClipboard]) and calls [load]/[save]
 * at lifecycle points; [ClipboardHistory.sweep] runs on every load and
 * every save, so TTL is enforced without a background worker (no
 * WorkManager dependency in `:ime` — every keyboard start and every
 * capture is a sweep point).
 *
 * Privacy: MODE_PRIVATE file, cleared on uninstall with the app. Clipboard
 * items are NEVER written to sync/export files (sync owns
 * personal+bigrams+session only — see `docs/SYNC.md`).
 *
 * Every I/O failure throws [IllegalStateException] with the cause — the
 * service surfaces it on the status line and falls back to empty, never a
 * half-loaded history.
 */
object ClipboardStore {
    const val PREFS_NAME = "kb_clipboard_prefs"
    const val KEY_ITEMS_JSON = "items_json"
    const val KEY_TTL_HOURS = "ttl_hours"

    private fun prefs(context: Context): SharedPreferences {
        try {
            return context.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        } catch (e: Exception) {
            throw IllegalStateException("Clipboard prefs unreadable: ${e.message}", e)
        }
    }

    /** Retention in hours; default 7d ([ClipboardHistory.DEFAULT_TTL_HOURS]). */
    fun ttlHours(context: Context): Long {
        try {
            val v = prefs(context).getLong(KEY_TTL_HOURS, ClipboardHistory.DEFAULT_TTL_HOURS)
            return try {
                ClipboardHistory.validateTtlHours(v)
                v
            } catch (_: IllegalArgumentException) {
                ClipboardHistory.DEFAULT_TTL_HOURS
            }
        } catch (e: IllegalStateException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Clipboard retention unreadable: ${e.message}", e)
        }
    }

    fun setTtlHours(context: Context, hours: Long) {
        ClipboardHistory.validateTtlHours(hours)
        try {
            prefs(context).edit().putLong(KEY_TTL_HOURS, hours).apply()
        } catch (e: Exception) {
            throw IllegalStateException("Could not save clipboard retention: ${e.message}", e)
        }
    }

    /**
     * Loads history swept to [nowMs]. Corrupt JSON surfaces loudly AND is
     * quarantined (rewritten empty) so the next start doesn't re-fail —
     * the failure stays visible via the thrown error, not a silent wipe:
     * callers catch, show the status line, and keep running empty.
     */
    fun load(context: Context, nowMs: Long = System.currentTimeMillis()): List<ClipboardItem> {
        val raw = try {
            prefs(context).getString(KEY_ITEMS_JSON, null)
        } catch (e: Exception) {
            throw IllegalStateException("Clipboard history unreadable: ${e.message}", e)
        }
        if (raw.isNullOrEmpty()) return emptyList()
        val parsed = ClipboardHistory.fromJson(raw)
        val swept = ClipboardHistory.sweep(parsed, ttlHours(context), nowMs)
        if (swept.size != parsed.size) save(context, swept)
        return ClipboardHistory.ordered(swept)
    }

    /** Sweeps then persists (pins survive; see [ClipboardHistory.sweep]). */
    fun save(context: Context, items: List<ClipboardItem>, nowMs: Long = System.currentTimeMillis()) {
        try {
            val swept = ClipboardHistory.sweep(items, ttlHours(context), nowMs)
            prefs(context).edit()
                .putString(KEY_ITEMS_JSON, ClipboardHistory.toJson(swept))
                .apply()
        } catch (e: IllegalStateException) {
            throw e
        } catch (e: Exception) {
            throw IllegalStateException("Could not save clipboard history: ${e.message}", e)
        }
    }

    /**
     * Settings → Clear: wipes unpinned, keeps pins unless [includePins]
     * (the confirm dialog's "Clear pins too?"). Returns the kept list.
     */
    fun clear(context: Context, items: List<ClipboardItem>, includePins: Boolean): List<ClipboardItem> {
        val kept = ClipboardHistory.clear(items, includePins)
        save(context, kept)
        return kept
    }
}
