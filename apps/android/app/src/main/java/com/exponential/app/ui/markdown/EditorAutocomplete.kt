package com.exponential.app.ui.markdown

import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark

/**
 * The `@`/`#` autocomplete menu's open rule and placement, kept pure so both
 * are unit-testable without a Compose UI harness (EXP-322 — the menu used to
 * anchor to the whole editor column and could not be dismissed).
 */

/**
 * Whether the menu may be shown. The decisive term is [armed]: web only OPENS
 * the menu on a document change (`editor-autocomplete.ts`:
 * `if (next && last === null && !docChanged) return`), so merely moving the
 * caret into an existing `#EXP-238` must not pop it — that is the reported
 * bug. The remaining terms keep it off unfocused rows, code rows, and inline
 * code spans.
 */
internal fun shouldOpenAutocomplete(
    armed: Boolean,
    hasOsFocus: Boolean,
    isFocusedRow: Boolean,
    kind: BlockKind,
    caretInInlineCode: Boolean,
    hasCandidates: Boolean,
): Boolean =
    armed &&
        hasOsFocus &&
        isFocusedRow &&
        kind != BlockKind.CodeBlock &&
        !caretInInlineCode &&
        hasCandidates

/** Whether [caret] sits inside an inline-code span, where triggers are inert. */
internal fun caretInInlineCode(marks: List<InlineMark>, caret: Int): Boolean =
    marks.any { it.kind == InlineKind.InlineCode && caret >= it.start && caret <= it.end }

/**
 * Where to place the menu, in window coordinates.
 *
 * [anchorBounds] is the text-glyph box the popup is parented to and the caret
 * offsets are relative to that same box, so the result tracks scrolling and
 * IME resize without any listener. The vertical band excludes the keyboard and
 * the floating markdown toolbar, and the menu flips above the caret line when
 * it does not fit below.
 */
internal fun autocompletePopupOffset(
    anchorBounds: IntRect,
    caretLeftInAnchor: Int,
    caretTopInAnchor: Int,
    caretBottomInAnchor: Int,
    popupSize: IntSize,
    windowSize: IntSize,
    imeBottomPx: Int,
    toolbarHeightPx: Int,
    marginPx: Int,
    gapPx: Int,
): IntOffset {
    val maxX = (windowSize.width - popupSize.width - marginPx).coerceAtLeast(marginPx)
    val x = (anchorBounds.left + caretLeftInAnchor).coerceIn(marginPx, maxX)

    val usableBottom = windowSize.height - imeBottomPx - toolbarHeightPx - marginPx
    val below = anchorBounds.top + caretBottomInAnchor + gapPx
    if (below + popupSize.height <= usableBottom) return IntOffset(x, below)

    val above = anchorBounds.top + caretTopInAnchor - gapPx - popupSize.height
    if (above >= marginPx) return IntOffset(x, above)

    // Neither side fits: pin into the usable band. The menu caps its own
    // height and scrolls, so this is always a sane placement rather than a
    // menu drawn under the keyboard.
    return IntOffset(x, (usableBottom - popupSize.height).coerceAtLeast(marginPx))
}

/**
 * Where a rail-button menu goes: ABOVE the button (the rail sits directly on
 * the IME, so below-the-anchor always lands in the keyboard band, which M3's
 * provider can't see in an edge-to-edge window — EXP-607), left-aligned to it,
 * clamped into the window. Pure for the same reason [autocompletePopupOffset] is.
 */
internal fun railMenuPopupOffset(
    anchorBounds: IntRect,
    popupSize: IntSize,
    windowSize: IntSize,
    marginPx: Int,
    gapPx: Int,
): IntOffset {
    val maxX = (windowSize.width - popupSize.width - marginPx).coerceAtLeast(marginPx)
    val x = anchorBounds.left.coerceIn(marginPx, maxX)
    val y = (anchorBounds.top - gapPx - popupSize.height).coerceAtLeast(marginPx)
    return IntOffset(x, y)
}
