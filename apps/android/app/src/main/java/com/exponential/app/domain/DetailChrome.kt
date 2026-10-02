package com.exponential.app.domain

/**
 * EXP-1162: THE detail chrome, identical ×4 (web `lib/detail-chrome.ts`,
 * desktop `domain::detail_chrome`, iOS `DetailChrome`), locked against the
 * contract fixture `detail-chrome.json`. Lengths are dp.
 *
 * TITLE COLLAPSE: the Issue face keeps its large title as a ROW of the
 * scrolling content; the header shows the identifier alone until that row has
 * scrolled under the header band, then BREAKS (a threshold, never a morph)
 * into the identifier over the one-line title. A face with no title row of
 * its own (Run, Changes, Results) is always collapsed.
 *
 * EDGE STRIPS: content scrolls under the header band and the floating bottom
 * bar; behind them the page background at [SCRIM] over an [EDGE_BLUR] blur,
 * and a strip past each ([EDGE_TOP] / [EDGE_BOTTOM]) fades both to nothing.
 */
object DetailChrome {
    /** The collapsed title's fade-in. */
    const val COLLAPSE_MS = 160

    /** How far the collapsed title rises while it fades in. */
    const val COLLAPSE_RISE = 4f

    /** The strip under the header band. */
    const val EDGE_TOP = 24f

    /** How far the bottom strip reaches above the floating bar's top. */
    const val EDGE_BOTTOM = 32f

    /** The backdrop blur radius. */
    const val EDGE_BLUR = 8f

    /** The page background's alpha over the blur. */
    const val SCRIM = 0.72f

    /**
     * `collapsed = !hasTitleRow || titleBottom <= headerBottom`, both in the
     * same coordinate space. A title row not measured yet stays expanded.
     */
    fun isTitleCollapsed(hasTitleRow: Boolean, titleBottom: Double?, headerBottom: Double): Boolean {
        if (!hasTitleRow) return true
        val bottom = titleBottom ?: return false
        return bottom <= headerBottom
    }
}
