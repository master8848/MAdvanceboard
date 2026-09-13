package com.kb.ime

import android.content.Context
import android.content.SharedPreferences
import android.content.*
import android.content.pm.*
import android.content.res.*
import android.database.*
import android.database.sqlite.*
import android.graphics.*
import android.graphics.drawable.*
import android.net.*
import android.os.*
import android.view.*
import java.io.*
import java.util.concurrent.*

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
 * other framework call hits a throwing stub below and fails loudly, so a test
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
 *
 * Extends [Context] directly (`android.test.mock.MockContext` is absent
 * from `android.jar`, so it cannot be used in JVM unit tests). The 100+
 * uninvolved framework methods throw [UnsupportedOperationException], so a
 * test that touches more framework than intended fails loudly instead of
 * passing against a hollow fake.
 */
class FakeContext(
    private val services: Map<String, Any?> = emptyMap()
) : Context() {
    private val files = mutableMapOf<String, FakePrefs>()

    override fun getSharedPreferences(name: String?, mode: Int): SharedPreferences =
        files.getOrPut(name!!) { FakePrefs() }


    /** Direct handle to a prefs file (for seeding corrupt/out-of-range values). */
    fun prefsFile(name: String): FakePrefs =
        files.getOrPut(name) { FakePrefs() }

    override fun checkCallingOrSelfPermission(p0: String): Int = throw UnsupportedOperationException("checkCallingOrSelfPermission")
    override fun checkCallingOrSelfUriPermission(p0: android.net.Uri?, p1: Int): Int = throw UnsupportedOperationException("checkCallingOrSelfUriPermission")
    override fun checkCallingPermission(p0: String): Int = throw UnsupportedOperationException("checkCallingPermission")
    override fun checkCallingUriPermission(p0: android.net.Uri?, p1: Int): Int = throw UnsupportedOperationException("checkCallingUriPermission")
    override fun checkPermission(p0: String, p1: Int, p2: Int): Int = throw UnsupportedOperationException("checkPermission")
    override fun checkSelfPermission(p0: String): Int = throw UnsupportedOperationException("checkSelfPermission")
    override fun clearWallpaper(): Unit = throw UnsupportedOperationException("clearWallpaper")
    override fun createContextForSplit(p0: String): android.content.Context? = throw UnsupportedOperationException("createContextForSplit")
    override fun createDeviceProtectedStorageContext(): android.content.Context? = throw UnsupportedOperationException("createDeviceProtectedStorageContext")
    override fun createPackageContext(p0: String, p1: Int): android.content.Context? = throw UnsupportedOperationException("createPackageContext")
    override fun databaseList(): Array<String?>? = throw UnsupportedOperationException("databaseList")
    override fun deleteDatabase(p0: String): Boolean = throw UnsupportedOperationException("deleteDatabase")
    override fun deleteFile(p0: String): Boolean = throw UnsupportedOperationException("deleteFile")
    override fun deleteSharedPreferences(p0: String): Boolean = throw UnsupportedOperationException("deleteSharedPreferences")
    override fun enforceCallingOrSelfUriPermission(p0: android.net.Uri?, p1: Int, p2: String): Unit = throw UnsupportedOperationException("enforceCallingOrSelfUriPermission")
    override fun enforceCallingUriPermission(p0: android.net.Uri?, p1: Int, p2: String): Unit = throw UnsupportedOperationException("enforceCallingUriPermission")
    override fun fileList(): Array<String?>? = throw UnsupportedOperationException("fileList")
    override fun getApplicationContext(): android.content.Context? = throw UnsupportedOperationException("getApplicationContext")
    override fun getApplicationInfo(): android.content.pm.ApplicationInfo? = throw UnsupportedOperationException("getApplicationInfo")
    override fun getAssets(): android.content.res.AssetManager? = throw UnsupportedOperationException("getAssets")
    override fun getCacheDir(): java.io.File? = throw UnsupportedOperationException("getCacheDir")
    override fun getClassLoader(): java.lang.ClassLoader? = throw UnsupportedOperationException("getClassLoader")
    override fun getCodeCacheDir(): java.io.File? = throw UnsupportedOperationException("getCodeCacheDir")
    override fun getContentResolver(): android.content.ContentResolver? = throw UnsupportedOperationException("getContentResolver")
    override fun getDataDir(): java.io.File? = throw UnsupportedOperationException("getDataDir")
    override fun getDatabasePath(p0: String): java.io.File? = throw UnsupportedOperationException("getDatabasePath")
    override fun getDir(p0: String, p1: Int): java.io.File? = throw UnsupportedOperationException("getDir")
    override fun getExternalCacheDir(): java.io.File? = throw UnsupportedOperationException("getExternalCacheDir")
    override fun getExternalCacheDirs(): Array<java.io.File?>? = throw UnsupportedOperationException("getExternalCacheDirs")
    override fun getExternalFilesDirs(p0: String): Array<java.io.File?>? = throw UnsupportedOperationException("getExternalFilesDirs")
    override fun getExternalMediaDirs(): Array<java.io.File?>? = throw UnsupportedOperationException("getExternalMediaDirs")
    override fun getFileStreamPath(p0: String): java.io.File? = throw UnsupportedOperationException("getFileStreamPath")
    override fun getFilesDir(): java.io.File? = throw UnsupportedOperationException("getFilesDir")
    override fun getMainLooper(): android.os.Looper? = throw UnsupportedOperationException("getMainLooper")
    override fun getNoBackupFilesDir(): java.io.File? = throw UnsupportedOperationException("getNoBackupFilesDir")
    override fun getObbDir(): java.io.File? = throw UnsupportedOperationException("getObbDir")
    override fun getObbDirs(): Array<java.io.File?>? = throw UnsupportedOperationException("getObbDirs")
    override fun getPackageCodePath(): String = throw UnsupportedOperationException("getPackageCodePath")
    override fun getPackageManager(): android.content.pm.PackageManager? = throw UnsupportedOperationException("getPackageManager")
    override fun getPackageName(): String = throw UnsupportedOperationException("getPackageName")
    override fun getPackageResourcePath(): String = throw UnsupportedOperationException("getPackageResourcePath")
    override fun getResources(): android.content.res.Resources? = throw UnsupportedOperationException("getResources")
    override fun getSystemServiceName(p0: Class<*>): String = throw UnsupportedOperationException("getSystemServiceName")
    override fun getTheme(): android.content.res.Resources.Theme? = throw UnsupportedOperationException("getTheme")
    override fun getWallpaper(): android.graphics.drawable.Drawable? = throw UnsupportedOperationException("getWallpaper")
    override fun getWallpaperDesiredMinimumHeight(): Int = throw UnsupportedOperationException("getWallpaperDesiredMinimumHeight")
    override fun getWallpaperDesiredMinimumWidth(): Int = throw UnsupportedOperationException("getWallpaperDesiredMinimumWidth")
    override fun grantUriPermission(p0: String, p1: android.net.Uri?, p2: Int): Unit = throw UnsupportedOperationException("grantUriPermission")
    override fun isDeviceProtectedStorage(): Boolean = throw UnsupportedOperationException("isDeviceProtectedStorage")
    override fun moveDatabaseFrom(p0: android.content.Context?, p1: String): Boolean = throw UnsupportedOperationException("moveDatabaseFrom")
    override fun moveSharedPreferencesFrom(p0: android.content.Context?, p1: String): Boolean = throw UnsupportedOperationException("moveSharedPreferencesFrom")
    override fun openFileInput(p0: String): java.io.FileInputStream? = throw UnsupportedOperationException("openFileInput")
    override fun openFileOutput(p0: String, p1: Int): java.io.FileOutputStream? = throw UnsupportedOperationException("openFileOutput")
    override fun openOrCreateDatabase(p0: String, p1: Int, p2: android.database.sqlite.SQLiteDatabase.CursorFactory?): android.database.sqlite.SQLiteDatabase? = throw UnsupportedOperationException("openOrCreateDatabase")
    override fun openOrCreateDatabase(p0: String, p1: Int, p2: android.database.sqlite.SQLiteDatabase.CursorFactory?, p3: android.database.DatabaseErrorHandler?): android.database.sqlite.SQLiteDatabase? = throw UnsupportedOperationException("openOrCreateDatabase")
    override fun peekWallpaper(): android.graphics.drawable.Drawable? = throw UnsupportedOperationException("peekWallpaper")
    override fun removeStickyBroadcast(p0: android.content.Intent?): Unit = throw UnsupportedOperationException("removeStickyBroadcast")
    override fun removeStickyBroadcastAsUser(p0: android.content.Intent?, p1: android.os.UserHandle?): Unit = throw UnsupportedOperationException("removeStickyBroadcastAsUser")
    override fun revokeUriPermission(p0: android.net.Uri?, p1: Int): Unit = throw UnsupportedOperationException("revokeUriPermission")
    override fun revokeUriPermission(p0: String, p1: android.net.Uri?, p2: Int): Unit = throw UnsupportedOperationException("revokeUriPermission")
    override fun sendStickyBroadcast(p0: android.content.Intent?): Unit = throw UnsupportedOperationException("sendStickyBroadcast")
    override fun sendStickyBroadcastAsUser(p0: android.content.Intent?, p1: android.os.UserHandle?): Unit = throw UnsupportedOperationException("sendStickyBroadcastAsUser")
    override fun setTheme(p0: Int): Unit = throw UnsupportedOperationException("setTheme")
    override fun setWallpaper(p0: android.graphics.Bitmap?): Unit = throw UnsupportedOperationException("setWallpaper")
    override fun setWallpaper(p0: java.io.InputStream?): Unit = throw UnsupportedOperationException("setWallpaper")
    override fun startActivities(p0: Array<android.content.Intent?>?): Unit = throw UnsupportedOperationException("startActivities")
    override fun startActivities(p0: Array<android.content.Intent?>?, p1: android.os.Bundle?): Unit = throw UnsupportedOperationException("startActivities")
    override fun startActivity(p0: android.content.Intent?): Unit = throw UnsupportedOperationException("startActivity")
    override fun startActivity(p0: android.content.Intent?, p1: android.os.Bundle?): Unit = throw UnsupportedOperationException("startActivity")
    override fun startForegroundService(p0: android.content.Intent?): android.content.ComponentName? = throw UnsupportedOperationException("startForegroundService")
    override fun startIntentSender(p0: android.content.IntentSender?, p1: android.content.Intent?, p2: Int, p3: Int, p4: Int): Unit = throw UnsupportedOperationException("startIntentSender")
    override fun startIntentSender(p0: android.content.IntentSender?, p1: android.content.Intent?, p2: Int, p3: Int, p4: Int, p5: android.os.Bundle?): Unit = throw UnsupportedOperationException("startIntentSender")
    override fun startService(p0: android.content.Intent?): android.content.ComponentName? = throw UnsupportedOperationException("startService")
    override fun stopService(p0: android.content.Intent?): Boolean = throw UnsupportedOperationException("stopService")
    override fun unregisterReceiver(p0: android.content.BroadcastReceiver?): Unit = throw UnsupportedOperationException("unregisterReceiver")
    override fun getSystemService(p0: String): Any? = services[p0]
    override fun bindService(p0: Intent, p1: Context.BindServiceFlags, p2: Executor, p3: ServiceConnection): Boolean = throw UnsupportedOperationException("bindService")
    override fun bindService(p0: Intent, p1: ServiceConnection, p2: Context.BindServiceFlags): Boolean = throw UnsupportedOperationException("bindService")
    override fun bindService(p0: Intent, p1: ServiceConnection, p2: Int): Boolean = throw UnsupportedOperationException("bindService")
    override fun bindService(p0: Intent, p1: Int, p2: Executor, p3: ServiceConnection): Boolean = throw UnsupportedOperationException("bindService")
    override fun checkUriPermission(p0: Uri?, p1: String?, p2: String?, p3: Int, p4: Int, p5: Int): Int = throw UnsupportedOperationException("checkUriPermission")
    override fun createConfigurationContext(p0: Configuration): Context = throw UnsupportedOperationException("createConfigurationContext")
    override fun createDisplayContext(p0: Display): Context = throw UnsupportedOperationException("createDisplayContext")
    override fun enforceCallingOrSelfPermission(p0: String, p1: String?): Unit = throw UnsupportedOperationException("enforceCallingOrSelfPermission")
    override fun enforceCallingPermission(p0: String, p1: String?): Unit = throw UnsupportedOperationException("enforceCallingPermission")
    override fun enforcePermission(p0: String, p1: Int, p2: Int, p3: String?): Unit = throw UnsupportedOperationException("enforcePermission")
    override fun enforceUriPermission(p0: Uri?, p1: String?, p2: String?, p3: Int, p4: Int, p5: Int, p6: String?): Unit = throw UnsupportedOperationException("enforceUriPermission")
    override fun getExternalFilesDir(p0: String?): File? = throw UnsupportedOperationException("getExternalFilesDir")
    override fun registerReceiver(p0: BroadcastReceiver, p1: IntentFilter, p2: String?, p3: Handler?): Intent? = throw UnsupportedOperationException("registerReceiver")
    override fun registerReceiver(p0: BroadcastReceiver, p1: IntentFilter, p2: String?, p3: Handler?, p4: Int): Intent? = throw UnsupportedOperationException("registerReceiver")
    override fun sendBroadcast(p0: Intent, p1: String?): Unit = throw UnsupportedOperationException("sendBroadcast")
    override fun sendBroadcast(p0: Intent, p1: String?, p2: Bundle?): Unit = throw UnsupportedOperationException("sendBroadcast")
    override fun sendBroadcastAsUser(p0: Intent, p1: UserHandle, p2: String?): Unit = throw UnsupportedOperationException("sendBroadcastAsUser")
    override fun sendOrderedBroadcast(p0: Intent, p1: String?): Unit = throw UnsupportedOperationException("sendOrderedBroadcast")
    override fun sendOrderedBroadcast(p0: Intent, p1: String?, p2: BroadcastReceiver?, p3: Handler?, p4: Int, p5: String?, p6: Bundle?): Unit = throw UnsupportedOperationException("sendOrderedBroadcast")
    override fun sendOrderedBroadcast(p0: Intent, p1: String?, p2: Bundle?): Unit = throw UnsupportedOperationException("sendOrderedBroadcast")
    override fun sendOrderedBroadcast(p0: Intent, p1: String?, p2: Bundle?, p3: BroadcastReceiver?, p4: Handler?, p5: Int, p6: String?, p7: Bundle?): Unit = throw UnsupportedOperationException("sendOrderedBroadcast")
    override fun sendOrderedBroadcast(p0: Intent, p1: String?, p2: String?, p3: BroadcastReceiver?, p4: Handler?, p5: Int, p6: String?, p7: Bundle?): Unit = throw UnsupportedOperationException("sendOrderedBroadcast")
    override fun sendOrderedBroadcastAsUser(p0: Intent, p1: UserHandle, p2: String?, p3: BroadcastReceiver, p4: Handler?, p5: Int, p6: String?, p7: Bundle?): Unit = throw UnsupportedOperationException("sendOrderedBroadcastAsUser")
    override fun sendStickyOrderedBroadcast(p0: Intent, p1: BroadcastReceiver, p2: Handler?, p3: Int, p4: String?, p5: Bundle?): Unit = throw UnsupportedOperationException("sendStickyOrderedBroadcast")
    override fun sendStickyOrderedBroadcastAsUser(p0: Intent, p1: UserHandle, p2: BroadcastReceiver, p3: Handler?, p4: Int, p5: String?, p6: Bundle?): Unit = throw UnsupportedOperationException("sendStickyOrderedBroadcastAsUser")
    override fun startInstrumentation(p0: ComponentName, p1: String?, p2: Bundle?): Boolean = throw UnsupportedOperationException("startInstrumentation")
    override fun unbindService(p0: ServiceConnection): Unit = throw UnsupportedOperationException("unbindService")
    override fun checkUriPermission(p0: Uri, p1: Int, p2: Int, p3: Int): Int = throw UnsupportedOperationException("checkUriPermission")
    override fun enforceUriPermission(p0: Uri, p1: Int, p2: Int, p3: Int, p4: String): Unit = throw UnsupportedOperationException("enforceUriPermission")
    override fun registerReceiver(p0: BroadcastReceiver?, p1: IntentFilter): Intent? = throw UnsupportedOperationException("registerReceiver")
    override fun registerReceiver(p0: BroadcastReceiver?, p1: IntentFilter, p2: Int): Intent? = throw UnsupportedOperationException("registerReceiver")
    override fun sendBroadcast(p0: Intent): Unit = throw UnsupportedOperationException("sendBroadcast")
    override fun sendBroadcastAsUser(p0: Intent, p1: UserHandle): Unit = throw UnsupportedOperationException("sendBroadcastAsUser")
}
