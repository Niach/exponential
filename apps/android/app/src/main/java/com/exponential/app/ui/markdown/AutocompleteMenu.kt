package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import com.exponential.app.ui.components.GlassMenuSurface
import com.exponential.app.ui.components.StatusIcon
import com.exponential.app.ui.emoji.EmojiRecord

/**
 * The `@`/`#`/`:` autocomplete menu, in two pieces (EXP-802/EXP-805).
 *
 * [AutocompleteRows] is the menu itself and NOTHING else — no popup, no caret
 * geometry — so a composer that is already pinned above the IME (the steer
 * composer, next to its `/` command menu) mounts it as a plain sibling above
 * the field and cannot have its focus stolen by construction. [AutocompleteMenu]
 * is the markdown editor's wrapper: the same rows in the caret-anchored Popup
 * [BlockTextField] needs, because there the field is a whole scrolling document
 * and the token can sit anywhere in it.
 */
@Composable
internal fun AutocompleteMenu(
    caretRect: Rect?,
    toolbarHeightPx: Int,
    mentionCandidates: List<MentionMember>,
    refCandidates: List<IssueRefTarget>,
    emojiCandidates: List<EmojiRecord>,
    selectedIndex: Int,
    onPickMention: (MentionMember) -> Unit,
    onPickIssueRef: (IssueRefTarget) -> Unit,
    onPickEmoji: (EmojiRecord) -> Unit,
) {
    val density = LocalDensity.current
    val imeBottomPx = WindowInsets.ime.getBottom(density)
    val marginPx = with(density) { 8.dp.roundToPx() }
    val gapPx = with(density) { 4.dp.roundToPx() }
    val provider = remember(caretRect, imeBottomPx, toolbarHeightPx, marginPx, gapPx) {
        object : PopupPositionProvider {
            override fun calculatePosition(
                anchorBounds: IntRect,
                windowSize: IntSize,
                layoutDirection: LayoutDirection,
                popupContentSize: IntSize,
            ): IntOffset = autocompletePopupOffset(
                anchorBounds = anchorBounds,
                caretLeftInAnchor = caretRect?.left?.toInt() ?: 0,
                caretTopInAnchor = caretRect?.top?.toInt() ?: 0,
                caretBottomInAnchor = caretRect?.bottom?.toInt() ?: anchorBounds.height,
                popupSize = popupContentSize,
                windowSize = windowSize,
                imeBottomPx = imeBottomPx,
                toolbarHeightPx = toolbarHeightPx,
                marginPx = marginPx,
                gapPx = gapPx,
            )
        }
    }
    Popup(
        popupPositionProvider = provider,
        // Focusable would steal focus from the field and drop the keyboard, so
        // dismissal rides the armed state + BackHandler instead.
        properties = PopupProperties(focusable = false),
    ) {
        AutocompleteRows(
            mentionCandidates = mentionCandidates,
            refCandidates = refCandidates,
            emojiCandidates = emojiCandidates,
            selectedIndex = selectedIndex,
            onPickMention = onPickMention,
            onPickIssueRef = onPickIssueRef,
            onPickEmoji = onPickEmoji,
            // A floating menu has no width to inherit.
            modifier = Modifier.width(260.dp),
        )
    }
}

/**
 * The candidate rows: members, then same-team issues, then emoji — only one of
 * the three lists is ever non-empty (the triggers are exclusive, see
 * [autocompleteTriggersAt]).
 *
 * [modifier] sizes the surface: a fixed width under the Popup, the composer's
 * full width when it is mounted in the layout. EXP-332: the same container as
 * every DropdownMenu in the app, so this is not a second menu look.
 */
@Composable
internal fun AutocompleteRows(
    mentionCandidates: List<MentionMember>,
    refCandidates: List<IssueRefTarget>,
    emojiCandidates: List<EmojiRecord>,
    /**
     * EXP-892: the highlighted row, an index into the FLAT candidate list
     * ([autocompleteCandidateCount]). The top row is selected while typing;
     * a hardware ↑/↓ moves it and Enter/Tab picks it, exactly like the `/`
     * command menu — one selection contract on every client.
     */
    selectedIndex: Int,
    onPickMention: (MentionMember) -> Unit,
    onPickIssueRef: (IssueRefTarget) -> Unit,
    onPickEmoji: (EmojiRecord) -> Unit,
    modifier: Modifier = Modifier,
) {
    // Only one of the three lists is ever non-empty, so the flat index is the
    // index within whichever list is showing.
    GlassMenuSurface(modifier = modifier) {
        Column(
            modifier = Modifier
                .heightIn(max = AutocompleteMenuMaxHeight)
                .verticalScroll(rememberScrollState())
                // Scrolls with the content, matching M3's menu padding.
                .padding(vertical = 4.dp),
        ) {
            mentionCandidates.forEachIndexed { index, m ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = AutocompleteRowHeight)
                        .background(selectionFill(index == selectedIndex))
                        .clickable { onPickMention(m) }
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(m.name, style = MaterialTheme.typography.bodyMedium, maxLines = 1)
                    Spacer(Modifier.weight(1f))
                    Text(
                        m.email,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                    )
                }
            }
            refCandidates.forEachIndexed { index, target ->
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = AutocompleteRowHeight)
                        .background(selectionFill(mentionCandidates.size + index == selectedIndex))
                        .clickable { onPickIssueRef(target) }
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    // EXP-581: status glyph first, then the mono identifier,
                    // then the title — the web IssueCandidateRow layout,
                    // now uniform across all four clients.
                    target.resolvedStatus?.let { status ->
                        StatusIcon(status, size = 16.dp)
                        Spacer(Modifier.width(8.dp))
                    }
                    Text(
                        target.identifier,
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        target.title,
                        style = MaterialTheme.typography.bodyMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
            emojiCandidates.forEachIndexed { index, emoji ->
                val flat = mentionCandidates.size + refCandidates.size + index
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .heightIn(min = AutocompleteRowHeight)
                        .background(selectionFill(flat == selectedIndex))
                        .clickable { onPickEmoji(emoji) }
                        .padding(horizontal = 12.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(emoji.unicode, style = MaterialTheme.typography.bodyLarge)
                    Spacer(Modifier.width(10.dp))
                    Text(
                        ":" + (emoji.shortcodes.firstOrNull() ?: emoji.label) + ":",
                        style = MaterialTheme.typography.labelMedium,
                        fontFamily = FontFamily.Monospace,
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(8.dp))
                    Text(
                        emoji.label,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

/**
 * The active fill of every menu in the app (the `/` command menu's, EXP-724) —
 * what "this row is what Enter picks" looks like.
 */
private fun selectionFill(selected: Boolean): Color =
    if (selected) Color.White.copy(alpha = 0.06f) else Color.Transparent

/**
 * The flat candidate count the ↑/↓ selection wraps around: at most one of the
 * three lists is ever non-empty (the triggers are exclusive), so the sum IS
 * the showing list's size.
 */
internal fun autocompleteCandidateCount(
    mentionCandidates: List<MentionMember>,
    refCandidates: List<IssueRefTarget>,
    emojiCandidates: List<EmojiRecord>,
): Int = mentionCandidates.size + refCandidates.size + emojiCandidates.size

/**
 * Pick the row at [index] of the flat candidate list — the ONE dispatcher
 * every host's Enter/Tab handler calls, so a key press and a tap can never
 * pick different things. False when the index is out of range.
 */
internal fun pickAutocompleteAt(
    index: Int,
    mentionCandidates: List<MentionMember>,
    refCandidates: List<IssueRefTarget>,
    emojiCandidates: List<EmojiRecord>,
    onPickMention: (MentionMember) -> Unit,
    onPickIssueRef: (IssueRefTarget) -> Unit,
    onPickEmoji: (EmojiRecord) -> Unit,
): Boolean {
    mentionCandidates.getOrNull(index)?.let { onPickMention(it); return true }
    refCandidates.getOrNull(index - mentionCandidates.size)?.let { onPickIssueRef(it); return true }
    emojiCandidates
        .getOrNull(index - mentionCandidates.size - refCandidates.size)
        ?.let { onPickEmoji(it); return true }
    return false
}

/** The scroll cap every autocomplete menu shares with the `/` command menu. */
private val AutocompleteMenuMaxHeight = 240.dp

/** One candidate row — the touch-target minimum, never a dense list. */
private val AutocompleteRowHeight = 44.dp

/**
 * The three splices a picked row performs, over the field's LIVE value.
 *
 * Each re-matches its own trigger against the live text at the caret (EXP-655)
 * instead of trusting a query captured when the row was composed, and returns
 * null when no token ends there — a stale row is then a no-op rather than a
 * mangled splice. The inserted form is always the plain interchange token
 * (`@email `, `#IDENTIFIER `, the unicode emoji), never a custom span, so the
 * stored markdown round-trips byte-identically on every client.
 */
internal fun TextFieldValue.withMention(member: MentionMember): TextFieldValue? =
    spliceTriggerToken(text, selection.start, MENTION_AT_CARET, "@" + member.email + " ")
        .asFieldValue()

/** The `#IDENTIFIER ` twin of [withMention]. */
internal fun TextFieldValue.withIssueRef(target: IssueRefTarget): TextFieldValue? =
    spliceTriggerToken(text, selection.start, ISSUE_REF_AT_CARET, "#" + target.identifier + " ")
        .asFieldValue()

/**
 * The `:shortcode` twin (EXP-551). [trailingSpace] is false for the closing-colon
 * auto-commit — `:tada:` is already a finished word — and true for a picked row.
 */
internal fun TextFieldValue.withEmoji(
    record: EmojiRecord,
    trailingSpace: Boolean,
): TextFieldValue? =
    spliceEmojiToken(
        text,
        selection.start,
        if (trailingSpace) record.unicode + " " else record.unicode,
    ).asFieldValue()

private fun Pair<String, Int>?.asFieldValue(): TextFieldValue? =
    this?.let { (text, caret) -> TextFieldValue(text, TextRange(caret)) }
