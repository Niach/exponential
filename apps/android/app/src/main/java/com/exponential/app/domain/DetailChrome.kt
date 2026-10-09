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
 * its own (Run, Guide) is always collapsed.
 *
 * EDGE STRIPS: content scrolls under the header band and the floating bottom
 * bar; behind them the page background at [SCRIM] over an [EDGE_BLUR] blur,
 * and a strip past each ([EDGE_TOP] / [EDGE_BOTTOM]) fades both to nothing.
 *
 * FACE MARKS: the header title carries NO state dot; state lives on the face
 * tabs ([faceDots]). The Run tab never draws a dot: a live run wears its
 * agent's brand mark LEADING the label ([FACE_MARK], amber [FACE_MARK_BADGE]
 * for needs input, beating while `agent_busy`); the `Review` tone stays a
 * trailing dot.
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

    /** A face tab's state dot, wide. */
    const val FACE_DOT = 6f

    /** The gap between a tab's label and its dot. */
    const val FACE_DOT_GAP = 6f

    /** The Run tab's agent brand mark, square. */
    const val FACE_MARK = 14f

    /** The gap between the Run tab's mark and its label. */
    const val FACE_MARK_GAP = 6f

    /** The needs-input badge on the mark's top end corner, wide. */
    const val FACE_MARK_BADGE = 6f

    /**
     * Which face tabs wear a state dot, in the session-dot tones. The Run tab
     * while its run is LIVE (`NeedsInput` while it waits on a person, else
     * `Running`); an OPEN pull request puts `Review` on the Guide (EXP-1251).
     * A face not on show carries no dot, an
     * ended run none.
     */
    fun faceDots(
        faces: List<WorkFaceKind>,
        runLive: Boolean,
        needsInput: Boolean,
        prOpen: Boolean,
    ): Map<WorkFaceKind, SessionDotTone> = buildMap {
        if (runLive && WorkFaceKind.Run in faces) {
            put(WorkFaceKind.Run, if (needsInput) SessionDotTone.NeedsInput else SessionDotTone.Running)
        }
        if (prOpen && WorkFaceKind.Guide in faces) put(WorkFaceKind.Guide, SessionDotTone.Review)
    }

    /** What a dotted tab says after its label ("Run, running"); null = no dot. */
    fun faceDotCaption(tone: SessionDotTone?): String? = when (tone) {
        SessionDotTone.Running -> "running"
        SessionDotTone.NeedsInput -> "needs input"
        SessionDotTone.Review -> "pull request open"
        else -> null
    }

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
