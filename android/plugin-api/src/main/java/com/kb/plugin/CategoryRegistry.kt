package com.kb.plugin

import android.content.res.AssetManager
import org.json.JSONObject

/** Manifest for one category pack in `assets/categories/<id>.json`. */
data class CategoryManifest(
    val id: String,
    val displayName: String,
    val priority: Int = 0,
    val learn: Boolean = true,
    val langs: List<String> = listOf("en"),
    val description: String = ""
)

/** Loads [CategoryManifest] JSON files from `assets/categories/<id>.json`. */
object CategoryRegistry {
    fun loadManifests(assetManager: AssetManager, ids: List<String>): List<CategoryManifest> =
        ids.mapNotNull { id -> loadOne(assetManager, id) }

    fun loadOne(assetManager: AssetManager, id: String): CategoryManifest? {
        return try {
            assetManager.open("categories/$id.json").bufferedReader().use { reader ->
                parse(id, JSONObject(reader.readText()))
            }
        } catch (_: Exception) {
            null
        }
    }

    fun listIds(assetManager: AssetManager): List<String> {
        return try {
            (assetManager.list("categories") ?: emptyArray())
                .filter { it.endsWith(".json") }
                .map { it.removeSuffix(".json") }
        } catch (_: Exception) {
            emptyList()
        }
    }

    private fun parse(fallbackId: String, json: JSONObject): CategoryManifest {
        val langs = mutableListOf<String>()
        val arr = json.optJSONArray("langs")
        if (arr != null) {
            for (i in 0 until arr.length()) langs.add(arr.optString(i))
        }
        // privacy.learn nests under "privacy": {"learn": bool}; flat "learn" also accepted.
        val privacy = json.optJSONObject("privacy")
        val learn = privacy?.optBoolean("learn", true)
            ?: json.optBoolean("learn", true)
        return CategoryManifest(
            id = json.optString("id", fallbackId),
            displayName = json.optString("displayName", fallbackId),
            priority = json.optInt("priority", 0),
            learn = learn,
            langs = langs.ifEmpty { listOf("en") },
            description = json.optString("description", "")
        )
    }
}
