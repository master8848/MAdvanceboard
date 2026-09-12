package com.kb.sync

import androidx.room.Entity
import androidx.room.PrimaryKey

/**
 * Personal-dictionary row with local-only metadata.
 * Tombstones (`deleted = true`) are exempt from LRU eviction.
 */
@Entity(tableName = "personal_words")
data class PersonalWordEntity(
    @PrimaryKey val word: String,
    val lang: String = "en",
    val count: Long = 1L,
    val lastSeen: Long = System.currentTimeMillis(),
    val deleted: Boolean = false,
    val deviceId: String = ""
)
