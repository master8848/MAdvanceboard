package com.kb.ime

/**
 * Tier-1 snippet palettes for `js` / `rust` / `html` / `math` (plan 07).
 *
 * Tier-0 (static keyword vocab in `suggest()`) is untouched — this file is
 * the Tier-1 local table + macro snippets only. `SuggestionItem`
 * (`com.kb.plugin`) has no cursor/tabstop field by design, so palettes
 * return their own item type ([SnippetItem]: `body` + commit behavior).
 *
 * Math stays a flat-token baseline per 07:14: [MATH_FLAT] bodies commit
 * verbatim with the caret parked inside the braces ([SnippetItem.cursorBack]).
 * There is deliberately NO tabstop structure here — the deferred
 * `\frac{}{}` tabstop set needs the plan-09 #5 usability test first and
 * must not be built on this track.
 */
enum class SnippetCommit {
    /** Ephemeral 2-min follow token (`if` → `if (`, `{`, `}`). */
    FOLLOW_TOKEN,
    /** `<tag attrs>…</tag>` with the caret parked between the tags. */
    HTML_SKELETON,
    /** Void element (`<img …>`): no closing tag, caret at end. */
    HTML_VOID,
    /** `&nbsp;` / `&amp;`: verbatim, caret at end. */
    HTML_ENTITY,
    /** Flat math token (`\frac{}{}`, `^{}…`): verbatim, caret inside. */
    MATH_FLAT,
}

/**
 * Palette item: `body` is the literal text committed to the editor,
 * [cursorBack] parks the caret that many chars left after commit (0 = end).
 *
 * The service moves the caret with relative DPAD_LEFT key events, so no
 * absolute editor offsets are stored — there is no tabstop/AST field.
 */
data class SnippetItem(
    val display: String,
    val body: String,
    val cursorBack: Int = 0,
    val commit: SnippetCommit = SnippetCommit.FOLLOW_TOKEN,
    val source: String = "",
) {
    init {
        require(body.isNotEmpty()) { "SnippetItem body must not be empty (display=$display)" }
        require(cursorBack in 0..body.length) {
            "SnippetItem cursorBack=$cursorBack out of range for body of length ${body.length}"
        }
    }
}

object SnippetPalettes {
    /** Ephemeral helper TTL: ~2 minutes (plan 07:12, 12:57). */
    const val SNIPPET_TTL_MS: Long = 120_000L

    /** Max live entries in the RAM-only buffer (plan 12:57). */
    const val MAX_ENTRIES: Int = 3

    /**
     * `js` trigger → next 2-3 syntax tokens. Kept tiny on purpose: static
     * Tier-0 vocab already covers keywords, this only chains the syntax
     * that follows a just-typed trigger.
     */
    private val JS_TRIGGERS: Map<String, List<String>> = mapOf(
        "if" to listOf("if (", "{", "}"),
        "for" to listOf("for (", "{", "}"),
        "while" to listOf("while (", "{", "}"),
        "function" to listOf("function ", "{", "}"),
        "return" to listOf("return ", ";"),
        "const" to listOf("const ", "=", ";"),
        "let" to listOf("let ", "=", ";"),
        "else" to listOf("else ", "{", "}"),
        "class" to listOf("class ", "{", "}"),
        "import" to listOf("import ", "from ", ";"),
        "export" to listOf("export ", "from ", ";"),
        "try" to listOf("try ", "{", "}"),
        "catch" to listOf("catch (", "{", "}"),
        "switch" to listOf("switch (", "{", "}"),
        "case" to listOf("case ", ":"),
        "new" to listOf("new ", "(", ")"),
        "=>" to listOf("=> ", "{", "}"),
    )

    /** `rust` trigger → next 2-3 syntax tokens (same ephemeral pattern). */
    private val RUST_TRIGGERS: Map<String, List<String>> = mapOf(
        "fn" to listOf("fn ", "{", "}"),
        "let" to listOf("let ", "=", ";"),
        "mut" to listOf("mut ", "=", ";"),
        "if" to listOf("if ", "{", "}"),
        "else" to listOf("else ", "{", "}"),
        "for" to listOf("for ", "{", "}"),
        "while" to listOf("while ", "{", "}"),
        "loop" to listOf("loop ", "{", "}"),
        "match" to listOf("match ", "{", "}"),
        "struct" to listOf("struct ", "{", "}"),
        "enum" to listOf("enum ", "{", "}"),
        "impl" to listOf("impl ", "{", "}"),
        "use" to listOf("use ", "::", ";"),
        "pub" to listOf("pub ", "fn ", "{"),
        "return" to listOf("return ", ";"),
        "->" to listOf("-> ", "{"),
    )

    /** HTML tag skeleton descriptor: `attrs` render inside the open tag. */
    data class HtmlTag(val name: String, val attrs: String = "", val void: Boolean = false)

    /** Tier-1 HTML skeletons: common tags + context attrs (`a→href`, `img→src/alt`). */
    val HTML_TAGS: List<HtmlTag> = listOf(
        HtmlTag("div"),
        HtmlTag("span"),
        HtmlTag("p"),
        HtmlTag("a", "href=\"\""),
        HtmlTag("img", "src=\"\" alt=\"\"", void = true),
        HtmlTag("input", "type=\"\"", void = true),
        HtmlTag("br", void = true),
        HtmlTag("button", "type=\"\""),
        HtmlTag("ul"),
        HtmlTag("li"),
        HtmlTag("h1"),
        HtmlTag("h2"),
        HtmlTag("script", "src=\"\""),
        HtmlTag("style"),
        HtmlTag("form", "action=\"\""),
        HtmlTag("label", "for=\"\""),
    )

    /** Tier-1 HTML entities. */
    val HTML_ENTITIES: List<String> = listOf("&nbsp;", "&amp;")

    /**
     * Flat math tokens (07:14 baseline — verbatim bodies, caret parked
     * inside the braces, NO tabstop navigation). The deferred structured
     * set (`\frac{}{}` tabstops, matrix `Tab→&`) needs the plan-09 #5
     * usability test first and is explicitly out of scope here.
     */
    val MATH_FLAT: List<SnippetItem> = listOf(
        SnippetItem(
            display = "\\frac{}{}",
            body = "\\frac{}{}",
            cursorBack = 3,
            commit = SnippetCommit.MATH_FLAT,
            source = "math-flat",
        ),
        SnippetItem(
            display = "^{}",
            body = "^{}",
            cursorBack = 1,
            commit = SnippetCommit.MATH_FLAT,
            source = "math-flat",
        ),
        SnippetItem(
            display = "_{}",
            body = "_{}",
            cursorBack = 1,
            commit = SnippetCommit.MATH_FLAT,
            source = "math-flat",
        ),
        SnippetItem(
            display = "\\sqrt{}",
            body = "\\sqrt{}",
            cursorBack = 1,
            commit = SnippetCommit.MATH_FLAT,
            source = "math-flat",
        ),
        SnippetItem(
            display = "\\sum",
            body = "\\sum",
            commit = SnippetCommit.MATH_FLAT,
            source = "math-flat",
        ),
        SnippetItem(
            display = "\\int",
            body = "\\int",
            commit = SnippetCommit.MATH_FLAT,
            source = "math-flat",
        ),
    )

    /**
     * Follow-tokens for a just-committed trigger word in `tab`.
     * Exact lowercase match; `js`/`rust` only — every other tab returns
     * empty (their Tier-1 is the static palette, not the ephemeral chain).
     */
    fun followTokens(tab: String, trigger: String): List<String> {
        val table = when (tab) {
            "js" -> JS_TRIGGERS
            "rust" -> RUST_TRIGGERS
            else -> return emptyList()
        }
        return table[trigger.lowercase()].orEmpty()
    }

    /** Skeleton item for one HTML tag (`<tag>$0</tag>`, caret = `$0`). */
    fun skeletonFor(tag: HtmlTag): SnippetItem {
        val open = if (tag.attrs.isEmpty()) "<${tag.name}>" else "<${tag.name} ${tag.attrs}>"
        if (tag.void) {
            return SnippetItem(
                display = open,
                body = open,
                cursorBack = 0,
                commit = SnippetCommit.HTML_VOID,
                source = "html-skeleton",
            )
        }
        val body = "$open</${tag.name}>"
        // Caret parks between the tags: back over the whole closing tag.
        val cursorBack = "</${tag.name}>".length
        return SnippetItem(
            display = open.dropLast(1) + ">…</${tag.name}>",
            body = body,
            cursorBack = cursorBack,
            commit = SnippetCommit.HTML_SKELETON,
            source = "html-skeleton",
        )
    }

    /**
     * Auto-close text for a committed tag name: `"</tag>"`, or null for
     * void elements and unknown/blank names. Null is a normal answer
     * (nothing to close), never an error.
     */
    fun autoClose(tagName: String): String? {
        val name = tagName.trim().lowercase()
        if (name.isEmpty()) return null
        val tag = HTML_TAGS.firstOrNull { it.name == name } ?: return null
        if (tag.void) return null
        return "</${tag.name}>"
    }

    /**
     * Static Tier-1 palette for `tab`, prefix-filtered (case-insensitive):
     * - `html`: tag skeletons + `&nbsp;`/`&amp;` entities.
     * - `math`: flat tokens (`\fr` → `\frac{}{}` though nothing is learned).
     * - `js`/`rust`/others: empty — their Tier-1 is the ephemeral
     *   follow-token buffer, not a static list.
     */
    fun staticSnippets(tab: String, prefix: String): List<SnippetItem> {
        val q = prefix.trimStart('<', '\\').lowercase()
        return when (tab) {
            "html" -> {
                val tags = HTML_TAGS
                    .filter { q.isEmpty() || it.name.startsWith(q) }
                    .map { skeletonFor(it) }
                val entities = HTML_ENTITIES
                    .filter { q.isEmpty() || it.lowercase().contains(q) }
                    .map {
                        SnippetItem(
                            display = it,
                            body = it,
                            commit = SnippetCommit.HTML_ENTITY,
                            source = "html-entity",
                        )
                    }
                tags + entities
            }
            "math" -> MATH_FLAT.filter { q.isEmpty() || it.body.lowercase().contains(q) }
            else -> emptyList()
        }
    }
}
