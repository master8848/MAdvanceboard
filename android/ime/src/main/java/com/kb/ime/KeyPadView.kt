package com.kb.ime

import android.content.Context
import android.util.AttributeSet

/**
 * Back-compat alias: the old hardcoded 9-key view is now a thin constructor
 * over the layout-driven [PadView] (`t9-9` spec from assets). New code should
 * use [PadView]/[padViewFor] directly.
 *
 * Legacy callbacks map onto [PadView] dispatch: [onDigit] receives single-char
 * text codes, [onDelete]/[onEnter]/[onSwitchIme] map 1:1.
 */
class KeyPadView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null
) : PadView(context, attrs, loadLayoutSpec(context, "t9-9")) {

    var onDigit: (Char) -> Unit = {}
    var onDeleteCompat: () -> Unit = {}
    var onEnterCompat: () -> Unit = {}
    var onSwitchImeCompat: () -> Unit = {}

    init {
        onCode = { code -> code.singleOrNull()?.let { onDigit(it) } }
        super.onDelete = { onDeleteCompat() }
        super.onEnter = { onEnterCompat() }
        super.onSwitchIme = { onSwitchImeCompat() }
    }
}
