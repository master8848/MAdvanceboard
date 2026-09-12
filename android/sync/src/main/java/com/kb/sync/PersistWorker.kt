package com.kb.sync

import android.content.Context
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import java.util.concurrent.TimeUnit

/**
 * Coalesced durability flush for the personal dictionary (the WorkManager
 * half of plan `06-persistence-power.md`; the core half is the 2 s
 * dirty-set flush in `core-rust/src/store.rs`).
 *
 * Keystrokes only dirty RAM/Room rows. Call [schedule] after a mutation;
 * the 2 s initial delay plus `KEEP` dedupe coalesces a typing burst into a
 * single run that (a) prunes cold rows past the 10k LFU cap and (b)
 * checkpoints the WAL (`TRUNCATE`), so standby pages stay flat.
 *
 * Failures are explicit: an exception returns [Result.retry] (visible with
 * backoff in WorkManager diagnostics) instead of a silent success, and the
 * pruned-row count is returned as output data for power tracing.
 */
class PersistWorker(appContext: Context, params: WorkerParameters) :
    CoroutineWorker(appContext, params) {

    override suspend fun doWork(): Result {
        val db = PersonalDatabase.get(applicationContext)
        return try {
            val pruned = db.words().evictCold(PersonalWordDao.KEEP_PERSONAL)
            db.openHelper.writableDatabase
                .query("PRAGMA wal_checkpoint(TRUNCATE);")
                .close()
            Result.success(workDataOf(OUTPUT_PRUNED to pruned))
        } catch (e: Exception) {
            // Loud retry: the flush did NOT happen; WorkManager backs off
            // and the next keystroke re-schedules. Never success-with-loss.
            Result.retry()
        }
    }

    companion object {
        const val UNIQUE_NAME = "kb-personal-persist"
        const val OUTPUT_PRUNED = "pruned_rows"

        /**
         * Request a coalesced flush in 2 s. `KEEP` dedupes: if a flush is
         * already enqueued from an earlier keystroke in the burst, it is
         * kept and no second job is added — O(1) disk wakes per burst.
         */
        fun schedule(context: Context) {
            val request = OneTimeWorkRequestBuilder<PersistWorker>()
                .setInitialDelay(2, TimeUnit.SECONDS)
                .addTag(UNIQUE_NAME)
                .build()
            WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(
                UNIQUE_NAME,
                ExistingWorkPolicy.KEEP,
                request
            )
        }

        fun cancel(context: Context) {
            WorkManager.getInstance(context.applicationContext)
                .cancelUniqueWork(UNIQUE_NAME)
        }
    }
}
