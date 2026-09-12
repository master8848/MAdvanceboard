package com.kb.sync

import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import androidx.sqlite.db.SupportSQLiteDatabase
import android.content.Context

/**
 * Single durable handle for the personal dictionary (mirrors the core
 * `Store` single-connection discipline in `core-rust/src/store.rs`).
 *
 * Power-friendly pragmas, applied on every open (Room may reopen the file
 * across process restarts, so they live in the callback, not in a
 * one-shot migration):
 *
 * * `journal_mode=WAL` (also enforced via the builder) — readers never
 *   block writers; checkpoints recycle instead of delete+rewrite.
 * * `synchronous=NORMAL` — WAL durability without a full fsync per commit.
 * * `temp_store=MEMORY`, `cache_size=-4096` (4 MiB page cache) — fewer,
 *   larger flash writes.
 * * `journal_size_limit=1048576` (1 MiB) — the WAL can never grow past the
 *   hourly checkpoint budget.
 * * `busy_timeout=5000` — contention surfaces as a timed wait, never a
 *   silent `SQLITE_BUSY` drop.
 *
 * Every failure here throws with the pragma named — Room surfaces it to the
 * caller instead of opening a half-configured database.
 */
@Database(entities = [PersonalWordEntity::class], version = 1, exportSchema = false)
abstract class PersonalDatabase : RoomDatabase() {
    abstract fun words(): PersonalWordDao

    companion object {
        @Volatile
        private var instance: PersonalDatabase? = null

        private val durabilityCallback = object : Callback() {
            override fun onOpen(db: SupportSQLiteDatabase) {
                super.onOpen(db)
                db.execSQL("PRAGMA journal_mode=WAL;")
                db.execSQL("PRAGMA synchronous=NORMAL;")
                db.execSQL("PRAGMA temp_store=MEMORY;")
                db.execSQL("PRAGMA cache_size=-4096;")
                db.execSQL("PRAGMA journal_size_limit=1048576;")
                db.execSQL("PRAGMA busy_timeout=5000;")
            }
        }

        fun get(context: Context): PersonalDatabase =
            instance ?: synchronized(this) {
                instance ?: Room.databaseBuilder(
                    context.applicationContext,
                    PersonalDatabase::class.java,
                    "kb-personal.db"
                )
                    // NOTE: no setJournalMode() call — WAL is Room's default
                    // *and* enforced by the callback below (`PRAGMA
                    // journal_mode=WAL` runs on every open, covering restarts
                    // where the builder flag would not re-apply).
                    .addCallback(durabilityCallback)
                    .build().also { instance = it }
            }
    }
}
