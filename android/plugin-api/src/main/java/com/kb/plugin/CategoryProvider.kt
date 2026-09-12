package com.kb.plugin

/** Static dictionary pack. No arbitrary code: data + optional tokenizer only. */
interface CategoryProvider {
    val id: String
    val displayName: String
    val priority: Int
    val learnEnabled: Boolean

    fun candidates(query: SuggestionQuery): List<SuggestionItem>
    fun contains(word: String): SuggestionItem?
}
