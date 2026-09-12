package com.kb.sync

import android.content.Context
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.syncStore by preferencesDataStore(name = "kb_sync")

/** Opt-in gate for all sync/export/import. Default OFF. */
object SyncPreferences {
    private val OPT_IN = booleanPreferencesKey("sync_opt_in")

    fun optedIn(context: Context): Flow<Boolean> =
        context.syncStore.data.map { it[OPT_IN] ?: false }

    suspend fun setOptedIn(context: Context, value: Boolean) {
        context.syncStore.edit { it[OPT_IN] = value }
    }
}
