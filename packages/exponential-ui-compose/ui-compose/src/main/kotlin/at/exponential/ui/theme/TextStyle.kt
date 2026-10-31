package at.exponential.ui.theme

import at.exponential.ui.ffi.FfiTextStyle

/**
 * A resolved text style (the core's `TextStyle`, inherited like CSS): size,
 * CSS weight and line height in dp, an optional family NAME, tracking (dp,
 * font scale applied), case and italics. The measurer shapes with it and
 * the leaves paint with it.
 */
data class ResolvedTextStyle(
    val fontSize: Float,
    val fontWeight: Int,
    val lineHeight: Float,
    val fontFamily: String?,
    /** `letterSpacing` (dp); null = none. */
    val letterSpacing: Float? = null,
    /** `uppercase | lowercase | capitalize`; null = none. */
    val textTransform: String? = null,
    /** `italic`; null = normal. */
    val fontStyle: String? = null,
    /** OpenType feature settings (`tnum` = tabular figures); null = the font's defaults. A painter-side rule, never the core's. */
    val fontFeatures: String? = null,
) {
    /** From the facade's record. */
    constructor(f: FfiTextStyle) : this(f.fontSize, f.fontWeight.toInt(), f.lineHeight, f.fontFamily, f.letterSpacing, f.textTransform, f.fontStyle)

    /** Italic? */
    val italic: Boolean get() = fontStyle == "italic"

    /**
     * EXP-1495 (round-6 §3): the parts whose figures are tabular, so a
     * counting text keeps its width as it counts (the Carousel `counter`
     * pill, `i / n`). The measurer and the leaf apply the same rule.
     */
    fun forPart(ownerComponent: String?, part: String?): ResolvedTextStyle =
        if (ownerComponent == "Carousel" && part == "counter" && fontFeatures == null) copy(fontFeatures = "tnum") else this

    companion object {
        /** The body default (14 / 400 / 20). */
        val BODY = ResolvedTextStyle(14f, 400, 20f, null)
    }
}
