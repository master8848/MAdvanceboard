package com.kb.sync

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Upsert

/**
 * Personal-dictionary writes are upsert-only: a learned word replaces its
 * row in place (`@Upsert` / `REPLACE`), never `deleteAll + reinsert`, so a
 * keystroke costs O(dirty) flash instead of O(table).
 *
 * The table is bounded like the core dict: [evictCold] keeps the hottest
 * [KEEP_PERSONAL] live rows by `(count DESC, lastSeen DESC)` — the SQL twin
 * of the core 10k-LFU cap (`core-rust/src/personal.rs`, `PERSONAL_CAP`).
 * Block tombstones (`deleted = 1`) are exempt from eviction, exactly as in
 * core. Tombstones are local-only (single-device scope): export/import
 * carries them as rows, but no merge reconciles them across devices.
 */
@Dao
interface PersonalWordDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(entry: PersonalWordEntity)

    /** Batch upsert for flush/import paths — one transaction, no deletes. */
    @Upsert
    suspend fun upsertAll(entries: List<PersonalWordEntity>)

    @Query("SELECT * FROM personal_words WHERE word = :word LIMIT 1")
    suspend fun get(word: String): PersonalWordEntity?

    @Query("SELECT * FROM personal_words ORDER BY lastSeen DESC LIMIT :limit")
    suspend fun recent(limit: Int = 200): List<PersonalWordEntity>

    @Query("SELECT * FROM personal_words")
    suspend fun all(): List<PersonalWordEntity>

    @Query("SELECT COUNT(*) FROM personal_words WHERE deleted = 0")
    suspend fun liveCount(): Long

    /**
     * LFU prune to [keep] live rows. Returns rows deleted so the caller
     * (and power tracing) can observe flash wear per run. Tombstones are
     * exempt; ties break toward newer `lastSeen`, matching core eviction
     * order (`count`, `last_seen`, key).
     */
    @Query(
        "DELETE FROM personal_words WHERE deleted = 0 AND word NOT IN " +
            "(SELECT word FROM personal_words WHERE deleted = 0 " +
            "ORDER BY count DESC, lastSeen DESC LIMIT :keep)"
    )
    suspend fun evictCold(keep: Int = KEEP_PERSONAL): Int

    @Query("DELETE FROM personal_words WHERE word = :word")
    suspend fun delete(word: String)

    companion object {
        /** Live-row cap, mirroring core `PERSONAL_CAP` (10k LFU). */
        const val KEEP_PERSONAL = 10_000
    }
}
