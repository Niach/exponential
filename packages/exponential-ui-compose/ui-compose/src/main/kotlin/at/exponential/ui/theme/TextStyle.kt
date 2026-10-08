package at.exponential.ui.theme

import at.exponential.ui.ffi.FfiTextStyle

/**
 * A resolved text style (the core's `TextStyle`): size, CSS weight and line
 * height in dp, and an optional family NAME.
 */
data class ResolvedTextStyle(
    val fontSize: Float,
    val fontWeight: Int,
    val lineHeight: Float,
    val fontFamily: String?,
) {
    /** From the facade's record. */
    constructor(f: FfiTextStyle) : this(f.fontSize, f.fontWeight.toInt(), f.lineHeight, f.fontFamily)

    companion object {
        /** The body default (14 / 400 / 20). */
        val BODY = ResolvedTextStyle(14f, 400, 20f, null)
    }
}
