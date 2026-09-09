package com.exponential.app.ui.markdown

import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import com.exponential.app.ui.emoji.EmojiTokenMatch
import com.exponential.app.ui.emoji.matchEmojiToken
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

/**
 * The trigger token in progress at the caret, if any — and which of the three
 * it is. Exactly one field is ever non-null: `@` wins over `#`, and `:` only
 * runs when neither matched (web `mention-textarea.tsx` checks @ first), so
 * one menu opens and never two.
 */
internal data class AutocompleteTriggers(
    val mention: MatchResult?,
    val issueRef: MatchResult?,
    val emoji: EmojiTokenMatch?,
) {
    /** The `@query` typed so far, or null when `@` is not the live trigger. */
    val mentionQuery: String? get() = mention?.groupValues?.get(1)

    /** The `#query` typed so far, or null when `#` is not the live trigger. */
    val issueRefQuery: String? get() = issueRef?.groupValues?.get(1)

    /**
     * No trigger ends at the caret — the caret left the token, whitespace was
     * typed, the trigger was deleted. The ARMED latch resets on this: the menu
     * may only reopen on a fresh text change (EXP-322).
     */
    val none: Boolean get() = mention == null && issueRef == null && emoji == null
}

/**
 * Match the three composer triggers against the text BEFORE the caret. The
 * `*Enabled` flags are "there is a vocabulary at all": no team members means
 * `@` cannot offer anything, no issue-ref handler means `#` cannot.
 */
internal fun autocompleteTriggersAt(
    beforeCaret: String,
    mentionsEnabled: Boolean,
    refsEnabled: Boolean,
): AutocompleteTriggers {
    val mention = if (mentionsEnabled) MENTION_AT_CARET.find(beforeCaret) else null
    val issueRef = if (refsEnabled && mention == null) ISSUE_REF_AT_CARET.find(beforeCaret) else null
    val emoji = if (mention == null && issueRef == null) matchEmojiToken(beforeCaret) else null
    return AutocompleteTriggers(mention, issueRef, emoji)
}

/**
 * The members a `@query` offers: name or email containing the query, capped —
 * a null [query] (no live `@` trigger) offers nobody.
 */
internal fun mentionCandidatesFor(
    members: List<MentionMember>,
    query: String?,
    limit: Int = MENTION_CANDIDATE_LIMIT,
): List<MentionMember> {
    if (query == null) return emptyList()
    val q = query.lowercase()
    return members
        .filter { it.name.lowercase().contains(q) || it.email.lowercase().contains(q) }
        .take(limit)
}

/** How many rows the `@` and `#` menus offer. */
internal const val MENTION_CANDIDATE_LIMIT = 6

/**
 * How many emoji the `:shortcode` typeahead offers (EXP-551) — the picker
 * sheet's cap is larger; this menu is a keyboard-adjacent shortlist.
 */
internal const val EMOJI_TYPEAHEAD_LIMIT = 8
