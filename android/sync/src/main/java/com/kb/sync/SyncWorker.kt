package com.kb.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import kotlinx.coroutines.flow.first
import java.util.concurrent.TimeUnit

/**
 * Periodic placeholder sync. Runs every 24h (within the 12-24h budget) and
 * only when the user has opted in via [SyncPreferences]. Real transport TBD.
 */
class SyncWorker(appContext: Context, params: WorkerParameters) :
    CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result {
        if (!SyncPreferences.optedIn(applicationContext).first()) {
            return Result.success()
        }
        // Stub: read local rows so the worker does real I/O without network.
        val db = PersonalDatabase.get(applicationContext)
        val rows = db.words().all().filter { !it.deleted }
        SyncMerge.toJsonl(rows)
        return Result.success()
    }

    companion object {
        const val UNIQUE_NAME = "kb-personal-sync"

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<SyncWorker>(24, TimeUnit.HOURS)
                .addTag(UNIQUE_NAME)
                .build()
            WorkManager.getInstance(context.applicationContext).enqueueUniquePeriodicWork(
                UNIQUE_NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                request
            )
        }

        fun cancel(context: Context) {
            WorkManager.getInstance(context.applicationContext)
                .cancelUniqueWork(UNIQUE_NAME)
        }
    }
}
