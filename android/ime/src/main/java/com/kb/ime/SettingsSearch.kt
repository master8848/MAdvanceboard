package com.kb.ime

/**
 * Settings search filter (plan/19 step 2): a dumb case-insensitive
 * substring filter over titles + subtitles + keywords. Pure Kotlin, no
 * Android imports — covered by JVM unit tests.
 */
data class SearchRow(
    val id: String,
    val title: String,
    val subtitle: String = "",
    val keywords: String = ""
)

/** Null/blank query returns every row (no maze); otherwise substring match. */
fun filterSearchRows(rows: List<SearchRow>, query: String): List<SearchRow> {
    val q = query.trim().lowercase()
    if (q.isEmpty()) return rows.toList()
    return rows.filter { row ->
        row.title.lowercase().contains(q) ||
            row.subtitle.lowercase().contains(q) ||
            row.keywords.lowercase().contains(q) ||
            row.id.lowercase().contains(q)
    }
}
