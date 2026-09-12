package com.kb.sync

import androidx.room.Database
import androidx.room.Room
import androidx.room.RoomDatabase
import android.content.Context

@Database(entities = [PersonalWordEntity::class], version = 1, exportSchema = false)
abstract class PersonalDatabase : RoomDatabase() {
    abstract fun words(): PersonalWordDao

    companion object {
        @Volatile
        private var instance: PersonalDatabase? = null

        fun get(context: Context): PersonalDatabase =
            instance ?: synchronized(this) {
                instance ?: Room.databaseBuilder(
                    context.applicationContext,
                    PersonalDatabase::class.java,
                    "kb-personal.db"
                ).build().also { instance = it }
            }
    }
}
