package com.kb.plugin

/** Query issued per keystroke: T9 digit sequence plus previous-word context. */
data class SuggestionQuery(
    val seq: String,
    val contextPrev: String? = null,
    val activeCategory: String = "words",
    val limit: Int = 30
)

/** Single ranked candidate returned by a CategoryProvider or the core engine. */
data class SuggestionItem(
    val word: String,
    val seq: String,
    val freq: Long = 1L,
    val category: String = "words",
    val lang: String = "en",
    val sourcePackId: String? = null
)

/** Context captured at commit time. `isPasswordField` must suppress learning. */
data class CommitContext(
    val word: String,
    val activeCategory: String,
    val isPasswordField: Boolean,
    val timestampMillis: Long = System.currentTimeMillis()
)
