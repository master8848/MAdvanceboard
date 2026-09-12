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

/** Explicit per-manifest load failure. Replaces silent null-skips: every
 * unreadable or corrupt `assets/categories/<id>.json` lands here with its
 * cause, so callers can surface it instead of dropping the tab quietly. */
data class CategoryLoadError(val id: String, val cause: String)

/**
 * Strict manifest-load outcome: entries that parsed plus one [CategoryLoadError]
 * per id that did not (missing asset, I/O failure, malformed JSON).
 * `errors.isEmpty()` means every requested id loaded.
 */
data class CategoryLoadOutcome(
    val loaded: List<CategoryManifest>,
    val errors: List<CategoryLoadError>
)

/** Loads [CategoryManifest] JSON files from `assets/categories/<id>.json`. */
object CategoryRegistry {
    /**
     * Lenient load (kept for compat): corrupt/missing manifests are skipped.
     * Prefer [loadManifestsStrict] — it returns the same entries PLUS the
     * per-id [CategoryLoadError]s this method drops.
     */
    fun loadManifests(assetManager: AssetManager, ids: List<String>): List<CategoryManifest> =
        loadManifestsStrict(assetManager, ids).loaded

    /**
     * Strict load: never skips silently. Every id yields either a manifest
     * in [CategoryLoadOutcome.loaded] or an entry in
     * [CategoryLoadOutcome.errors] carrying the cause. Callers MUST surface
     * a non-empty `errors` (status line / log) — a missing tab is user-visible
     * state, not an internal detail.
     */
    fun loadManifestsStrict(assetManager: AssetManager, ids: List<String>): CategoryLoadOutcome {
        val loaded = mutableListOf<CategoryManifest>()
        val errors = mutableListOf<CategoryLoadError>()
        for (id in ids) {
            try {
                assetManager.open("categories/$id.json").bufferedReader().use { reader ->
                    loaded.add(parse(id, JSONObject(reader.readText())))
                }
            } catch (e: Exception) {
                errors.add(CategoryLoadError(id, e.message ?: e.javaClass.simpleName))
            }
        }
        return CategoryLoadOutcome(loaded, errors)
    }

    /**
     * Lenient single load (kept for compat): null on any failure. Prefer
     * [loadManifestsStrict] for the explicit per-id cause.
     */
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
