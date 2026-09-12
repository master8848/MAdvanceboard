package com.kb.sync

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query

@Dao
interface PersonalWordDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun upsert(entry: PersonalWordEntity)

    @Query("SELECT * FROM personal_words WHERE word = :word LIMIT 1")
    suspend fun get(word: String): PersonalWordEntity?

    @Query("SELECT * FROM personal_words ORDER BY lastSeen DESC LIMIT :limit")
    suspend fun recent(limit: Int = 200): List<PersonalWordEntity>

    @Query("SELECT * FROM personal_words")
    suspend fun all(): List<PersonalWordEntity>

    @Query("DELETE FROM personal_words WHERE word = :word")
    suspend fun delete(word: String)
}
