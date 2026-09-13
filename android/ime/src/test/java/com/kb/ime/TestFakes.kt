package com.kb.ime

import android.content.SharedPreferences
import android.test.mock.MockContext

/**
 * JVM unit-test fakes for the bug-hunt sweep.
 *
 * The `:ime` stores deliberately use framework `SharedPreferences` (lean
 * process: no DataStore/Room), which historically kept their defaults
 * untested on the JVM. [FakeContext] serves an in-memory [FakePrefs] per
 * file name (plus an injectable service map), so default values, round
 * trips, and validation paths run as plain JUnit4 with no Robolectric.
 *
 * Only `getSharedPreferences` / `getSystemService` are overridden; every
 * other framework call hits the `MockContext` stub and throws, so a test
 * that touches more framework than intended fails loudly instead of
 * passing against a hollow fake.
 */
class FakePrefs : SharedPreferences {
    private val data = mutableMapOf<String, Any?>()

    override fun contains(key: String?): Boolean = data.containsKey(key)

    override fun edit(): SharedPreferences.Editor = FakeEditor()

    override fun getAll(): Map<String, *> = data.toMap()

    @Suppress("UNCHECKED_CAST")
    override fun getBoolean(key: String?, defValue: Boolean): Boolean =
        data[key] as? Boolean ?: defValue

    override fun getFloat(key: String?, defValue: Float): Float =
        (data[key] as? Number)?.toFloat() ?: defValue

    override fun getInt(key: String?, defValue: Int): Int =
        (data[key] as? Number)?.toInt() ?: defValue

    override fun getLong(key: String?, defValue: Long): Long =
        (data[key] as? Number)?.toLong() ?: defValue

    override fun getString(key: String?, defValue: String?): String? =
        data[key] as? String ?: defValue

    @Suppress("UNCHECKED_CAST")
    override fun getStringSet(key: String?, defValues: Set<String>?): Set<String>? =
        (data[key] as? Set<String>)?.toMutableSet() ?: defValues

    override fun registerOnSharedPreferenceChangeListener(
        listener: SharedPreferences.OnSharedPreferenceChangeListener?
    ) = Unit

    override fun unregisterOnSharedPreferenceChangeListener(
        listener: SharedPreferences.OnSharedPreferenceChangeListener?
    ) = Unit

    private inner class FakeEditor : SharedPreferences.Editor {
        private val pending = mutableMapOf<String, Any?>()
        private var clearAll = false

        override fun apply() {
            if (clearAll) data.clear()
            data.putAll(pending)
        }

        override fun clear(): SharedPreferences.Editor {
            clearAll = true
            return this
        }

        override fun commit(): Boolean {
            apply()
            return true
        }

        override fun putBoolean(key: String?, value: Boolean): SharedPreferences.Editor {
            pending[key!!] = value
            return this
        }

        override fun putFloat(key: String?, value: Float): SharedPreferences.Editor {
            pending[key!!] = value
            return this
        }

        override fun putInt(key: String?, value: Int): SharedPreferences.Editor {
            pending[key!!] = value
            return this
        }

        override fun putLong(key: String?, value: Long): SharedPreferences.Editor {
            pending[key!!] = value
            return this
        }

        override fun putString(key: String?, value: String?): SharedPreferences.Editor {
            pending[key!!] = value
            return this
        }

        override fun putStringSet(key: String?, values: Set<String>?): SharedPreferences.Editor {
            pending[key!!] = values?.toMutableSet()
            return this
        }

        override fun remove(key: String?): SharedPreferences.Editor {
            pending.remove(key)
            // Removal must survive apply(): null would shadow on read, so
            // drop from the backing map eagerly and keep the tombstone out.
            data.remove(key)
            return this
        }
    }
}

/**
 * Framework `Context` stand-in: named in-memory prefs files + a service map.
 * A missing service returns null (the production fail-open path in
 * [AccessibilityGates.evaluate]: no manager -> gestures stay enabled with
 * an explicit reason).
 */
class FakeContext(
    private val services: Map<String, Any?> = emptyMap()
) : MockContext() {
    private val files = mutableMapOf<String, FakePrefs>()

    override fun getSharedPreferences(name: String?, mode: Int): SharedPreferences =
        files.getOrPut(name!!) { FakePrefs() }

    override fun getSystemService(name: String?): Any? = services[name]

    /** Direct handle to a prefs file (for seeding corrupt/out-of-range values). */
    fun prefsFile(name: String): FakePrefs =
        files.getOrPut(name) { FakePrefs() }
}
