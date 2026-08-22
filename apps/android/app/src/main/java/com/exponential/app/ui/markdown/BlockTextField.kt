package com.exponential.app.ui.markdown

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.LineHeightStyle
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
import com.exponential.app.ui.emoji.EmojiTokenMatch
import com.exponential.app.ui.emoji.matchEmojiToken
import com.exponential.app.ui.emoji.rememberEmojiData
import com.exponential.app.ui.emoji.rememberEmojiPrefs
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.theme.resolvedStatusColor

// In-progress mention `@query` at the caret (after start-of-text or whitespace —
// '\n' counts as whitespace, so line starts inside the multi-line run trigger
// too); the query stops at whitespace. Mirrors apps/web/src/components/mention-textarea.tsx.
private val MENTION_AT_CARET = Regex("(?:^|\\s)@([A-Za-z0-9._%+-]*)$")

// In-progress issue reference `#query` at the caret — same shape as the web
// ISSUE_REF_AT_CARET (mention-textarea.tsx / editor-autocomplete.ts).
private val ISSUE_REF_AT_CARET = Regex("(?:^|\\s)#([A-Za-z0-9-]*)$")

// How many emoji the `:shortcode` typeahead offers (EXP-551) — the picker
// sheet's cap is larger; this menu is a keyboard-adjacent shortlist.
private const val EMOJI_TYPEAHEAD_LIMIT = 8

/**
 * One editable text RUN — every '\n'-separated paragraph between two images in
 * ONE multi-line [BasicTextField] (EXP-534, the iOS one-UITextView-per-run
 * architecture), which is what lets selection/copy/cut span paragraphs,
 * headings, list items, quotes and code fences. Per-paragraph styling
 * (heading size / indents / quote & code colors) rides the visual
 * transformation ([ChipVisualTransformation]); quote bars, code backgrounds
 * and list glyphs are painted behind the text ([drawRunDecorations]). Enter is
 * an ordinary '\n' and Backspace over a line boundary an ordinary delete —
 * both flow through [EditorModel.updateRun], which remaps the paragraph-attrs
 * list; the field only re-seeds its value when the row's revision bumps
 * (structural/external change), never on the user's own keystrokes.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun BlockTextField(
    model: EditorModel,
    row: EditorRow.TextRun,
    placeholder: String?,
    mentionMembers: List<MentionMember> = emptyList(),
    modifier: Modifier = Modifier,
) {
    val revision = model.revision(row.id)
    var value by remember(row.id) {
        mutableStateOf(TextFieldValue(text = row.text, selection = TextRange(row.text.length)))
    }

    // Whether a TEXT change armed the `@`/`#` menu. Web only opens the
    // autocomplete on a document change (editor-autocomplete.ts:
    // `if (next && last === null && !docChanged) return`); without this, moving
    // the caret into an existing `#EXP-238` popped the menu and nothing could
    // dismiss it (EXP-322).
    var armed by remember(row.id) { mutableStateOf(false) }

    // Re-seed from the model only on structural/external change (revision bump).
    LaunchedEffect(revision) {
        val caret = model.consumeDesiredSelection(row.id) ?: value.selection.start.coerceIn(0, row.text.length)
        if (value.text != row.text || value.selection.start != caret) {
            value = TextFieldValue(text = row.text, selection = TextRange(caret.coerceIn(0, row.text.length)))
        }
        // A toolbar `@`/`#` tap arrives here as a plain revision bump,
        // indistinguishable from any other reseed — hence the explicit signal.
        if (model.consumeAutocompleteArm(row.id)) armed = true
    }

    val focusRequester = remember { FocusRequester() }
    // Whether THIS field currently holds OS focus. Two focus-handoff guards
    // hang off it (EXP-25 — a structural change left the caret in the old row):
    //  - a freshly-composed row emits an initial focused=false event; without
    //    the guard below that cleared the focusedRowId the model had just
    //    pointed at the new row, so its requestFocus loop never ran and
    //    the old field kept focus.
    //  - requestFocus() on a not-yet-placed node can no-op WITHOUT throwing,
    //    so the retry loop must verify focus actually landed instead of
    //    trusting the absence of an exception.
    var hasOsFocus by remember(row.id) { mutableStateOf(false) }
    LaunchedEffect(model.focusedRowId) {
        if (model.focusedRowId == row.id) {
            // A freshly-created row (image insert/delete, run merge) may not be
            // laid out yet when this effect first runs; requestFocus() throws
            // (or silently does nothing) until the node is placed. Retry across
            // frames until the focus change is actually observed, so focus
            // deterministically lands on the new row instead of staying on the
            // old one.
            var attempts = 0
            while (attempts < 8 && model.focusedRowId == row.id && !hasOsFocus) {
                runCatching { focusRequester.requestFocus() }
                if (hasOsFocus) break
                withFrameNanos { }
                attempts++
            }
        }
    }

    // @mention autocomplete: detect an in-progress `@query` before the caret and
    // offer matching members; tapping inserts the canonical `@email ` form the
    // server resolves. Tap-to-insert keeps Enter behaving as a newline.
    val beforeCaret = value.text.take(value.selection.start)
    val mentionMatch =
        if (mentionMembers.isNotEmpty()) MENTION_AT_CARET.find(beforeCaret) else null
    val mentionQuery = mentionMatch?.groupValues?.get(1)
    val mentionCandidates =
        if (mentionQuery != null) {
            val q = mentionQuery.lowercase()
            mentionMembers
                .filter { it.name.lowercase().contains(q) || it.email.lowercase().contains(q) }
                .take(6)
        } else {
            emptyList()
        }

    fun insertMention(member: MentionMember) {
        val caret = value.selection.start
        val q = mentionQuery ?: return
        val start = caret - q.length - 1
        if (start < 0) return
        val newText =
            value.text.substring(0, start) + "@" + member.email + " " + value.text.substring(caret)
        val newCaret = start + member.email.length + 2
        value = TextFieldValue(newText, TextRange(newCaret))
        model.updateRun(row.id, newText, newCaret)
        model.updateSelection(row.id, newCaret..newCaret)
        armed = false
    }

    // #issue-ref autocomplete (masterplan §5e): detect an in-progress `#query`
    // before the caret and offer same-team issues from [LocalIssueRefs]
    // (identifier + title substring, newest first, empty query = most recent —
    // web IssueRefProvider.search parity). Tapping inserts the plain
    // `#IDENTIFIER ` interchange token, never a custom span, so the GFM
    // round-trip stays byte-identical. Mention detection wins when both could
    // match (web checks @ first).
    val issueRefs = LocalIssueRefs.current
    val refMatch =
        if (issueRefs != null && mentionMatch == null) ISSUE_REF_AT_CARET.find(beforeCaret) else null
    val refQuery = refMatch?.groupValues?.get(1)
    val refCandidates =
        if (refQuery != null && issueRefs != null) issueRefs.search(refQuery, limit = 6)
        else emptyList()

    fun insertIssueRef(target: IssueRefTarget) {
        val caret = value.selection.start
        val q = refQuery ?: return
        val start = caret - q.length - 1
        if (start < 0) return
        val newText =
            value.text.substring(0, start) + "#" + target.identifier + " " + value.text.substring(caret)
        val newCaret = start + target.identifier.length + 2
        value = TextFieldValue(newText, TextRange(newCaret))
        model.updateRun(row.id, newText, newCaret)
        model.updateSelection(row.id, newCaret..newCaret)
        armed = false
    }

    // `:shortcode` emoji typeahead (EXP-551): the same trigger shape as `@`/`#`,
    // matched only when neither of those does. Candidates come from the shared
    // dataset (assets/emoji.json), loaded lazily the first time a token
    // appears. A pick replaces the WHOLE token with the unicode + a space —
    // never `:shortcode:` text, because the markdown is shared with clients
    // that render only unicode.
    val emojiMatch: EmojiTokenMatch? =
        if (mentionMatch == null && refMatch == null) matchEmojiToken(beforeCaret) else null
    val emojiData = rememberEmojiData(enabled = emojiMatch != null)
    val emojiPrefs = rememberEmojiPrefs()
    val emojiCandidates =
        if (emojiMatch != null && emojiData != null) {
            emojiData.search(emojiMatch.query, limit = EMOJI_TYPEAHEAD_LIMIT)
        } else {
            emptyList()
        }

    fun insertEmoji(record: EmojiRecord, trailingSpace: Boolean) {
        val match = emojiMatch ?: return
        val caret = value.selection.start
        val start = caret - match.length
        if (start < 0) return
        val unicode = record.unicode
        val inserted = if (trailingSpace) unicode + " " else unicode
        val newText = value.text.substring(0, start) + inserted + value.text.substring(caret)
        val newCaret = start + inserted.length
        value = TextFieldValue(newText, TextRange(newCaret))
        model.updateRun(row.id, newText, newCaret)
        model.updateSelection(row.id, newCaret..newCaret)
        emojiPrefs.pushRecent(record.unicode)
        armed = false
    }

    val marks = row.marks
    val paragraphs = row.paragraphs
    // The paragraph under the caret gates the autocomplete (code lines are
    // inert) — per-LINE now that the field spans the whole run.
    val caretAttrs = paragraphs.getOrNull(ParaRemap.paraIndexAt(value.text, value.selection.start))
        ?: ParagraphAttrs.PLAIN

    // Whether an autocomplete may show here at all, candidates aside — the
    // `:shortcode:` auto-commit below rides the same gate (never inside code).
    val autocompleteEligible = shouldOpenAutocomplete(
        armed = armed,
        hasOsFocus = hasOsFocus,
        isFocusedRow = model.focusedRowId == row.id,
        kind = caretAttrs.kind,
        caretInInlineCode = caretInInlineCode(marks, value.selection.start),
        hasCandidates = true,
    )
    val menuOpen = autocompleteEligible &&
        (mentionCandidates.isNotEmpty() || refCandidates.isNotEmpty() || emojiCandidates.isNotEmpty())
    // The regex stopped matching (caret left the token, whitespace typed, the
    // trigger was deleted) — require a fresh text change to reopen.
    val noTrigger = mentionMatch == null && refMatch == null && emojiMatch == null
    LaunchedEffect(noTrigger) {
        if (noTrigger) armed = false
    }

    // `:tada:` — the closing colon plus an EXACT shortcode commits immediately
    // (no trailing space), so a user who habitually types the closed form never
    // leaves literal shortcode text behind. Web/iOS/desktop parity.
    val closedShortcode = emojiMatch?.takeIf { it.closed }?.query
    LaunchedEffect(closedShortcode, emojiData, autocompleteEligible) {
        val code = closedShortcode ?: return@LaunchedEffect
        if (!autocompleteEligible) return@LaunchedEffect
        val record = emojiData?.findShortcode(code) ?: return@LaunchedEffect
        insertEmoji(record, trailingSpace = false)
    }

    // Caret geometry for the menu's anchor, in the text-glyph box's own
    // coordinates (see the Popup below).
    var textLayout by remember(row.id) { mutableStateOf<TextLayoutResult?>(null) }
    val chipTransform = remember(value.text, marks, issueRefs, paragraphs) {
        IssueChipTransform.build(value.text, marks, issueRefs, paragraphs)
    }
    // The painted half of the editor's chips (EXP-423) — same geometry rules as
    // the read renderer, in the decoration box's coordinate space.
    val chipSpecs = remember(chipTransform) {
        chipTransform.chips.map { chip ->
            IssueRefChipSpec(
                start = chip.displayStart,
                end = chip.displayEnd,
                tokenStart = chip.displayStart,
                iconName = chip.target.resolvedStatus?.iconName,
                color = chip.target.resolvedStatus?.let { resolvedStatusColor(it) },
            )
        }
    }
    // The per-paragraph vertical bands the painted decorations + checkbox tap
    // targets key off, in the same display coordinates as [chipSpecs].
    val bands = remember(chipTransform, paragraphs) {
        paraBands(value.text, paragraphs, chipTransform)
    }
    val caretRect = remember(textLayout, value.selection.start, chipTransform) {
        val layout = textLayout ?: return@remember null
        // getCursorRect wants a TRANSFORMED offset and throws out of range;
        // the layout lags `value` by up to a frame while typing fast, so coerce
        // against the laid-out text and fall back to the row bottom for that
        // one frame rather than crash.
        val offset = chipTransform.originalToTransformed(value.selection.start)
            .coerceIn(0, layout.layoutInput.text.length)
        runCatching { layout.getCursorRect(offset) }.getOrNull()
    }
    val toolbarHeightPx = LocalMarkdownToolbarController.current?.toolbarHeightPx ?: 0

    // Caret auto-scroll while typing (EXP-534). The legacy CoreTextField only
    // issues bringIntoView on focus GAIN, so a run that grows taller while
    // typing (soft wrap or Enter — no focus handoff anymore) walks the caret
    // below the viewport / behind the formatting bar. Re-request whenever the
    // post-layout caret rect changes while THIS field holds focus. The request
    // propagates through every scrollable ancestor, covering both the page
    // scroll (issue detail/create) and the comment composer's bounded box. The
    // rect is the caret rect EXACTLY — no breathing-room margin: a rect
    // reaching outside the requester node's bounds sends ContentInViewNode
    // into a creeping never-visible scroll animation (verified on foundation
    // 1.7.6), and the viewport is already inset by the toolbar + IME so the
    // bare rect is enough.
    //
    // Requests are gated on the layout MATCHING the text (EXP-609): a
    // keystroke recomposes with the previous frame's TextLayoutResult, and a
    // rect coerced against that stale layout can itself reach outside the
    // node's fresh bounds — backspacing over a newline shrinks the field by a
    // line while the coerced rect still points at the old last line — which
    // trips exactly the creeping-scroll animation above and read as the page
    // jumping while typing under an image (tall-enough-to-scroll content).
    // Skipping the stale frame loses nothing: the fresh layout lands a frame
    // later, recomputes caretRect, and re-fires this effect with a rect that
    // is always inside the node.
    val bringIntoViewRequester = remember(row.id) { BringIntoViewRequester() }
    val layoutMatchesText =
        textLayout?.layoutInput?.text?.length == chipTransform.display.length
    LaunchedEffect(caretRect, hasOsFocus, layoutMatchesText) {
        val rect = caretRect
        if (hasOsFocus && rect != null && layoutMatchesText) {
            bringIntoViewRequester.bringIntoView(rect)
        }
    }

    BasicTextField(
        value = value,
        onValueChange = { new ->
            // Only a real document change may open the menu (web parity).
            if (new.text != value.text) armed = true
            value = new
            if (new.text != row.text) model.updateRun(row.id, new.text, new.selection.start)
            model.updateSelection(row.id, new.selection.start..new.selection.end)
        },
        // Fixed line boxes (center + no trim): every line is exactly lineHeight
        // tall, so lines that carry their own ParagraphStyle (list/quote/code
        // indents — each range is a separate paragraph in the AnnotatedString)
        // don't pick up the platform's extra first/last-line font padding and a
        // multi-line code fence packs as tightly as the read renderer's.
        textStyle = MdStyle.body.copy(
            lineHeightStyle = LineHeightStyle(
                alignment = LineHeightStyle.Alignment.Center,
                trim = LineHeightStyle.Trim.None,
            ),
        ),
        onTextLayout = { textLayout = it },
        cursorBrush = SolidColor(MdStyle.Link),
        // Per-paragraph styles (heading/list/quote/code) + inline marks +
        // resolved `#IDENTIFIER` chips (EXP-322) — display-only, the stored
        // markdown keeps the bare tokens.
        visualTransformation = ChipVisualTransformation(
            marks = marks,
            issueRefs = issueRefs,
            paragraphs = paragraphs,
            fontScale = LocalDensity.current.fontScale,
        ),
        modifier = modifier
            .focusRequester(focusRequester)
            .onFocusChanged { fs ->
                if (fs.isFocused) {
                    hasOsFocus = true
                    model.setFocused(row.id)
                } else {
                    // Only a field that actually HELD focus may clear the model's
                    // focus target — the initial focused=false of a freshly-
                    // created row must not cancel its pending handoff.
                    if (hasOsFocus) model.clearFocusIfMatches(row.id)
                    hasOsFocus = false
                    armed = false
                }
            }
            .onPreviewKeyEvent { event ->
                if (
                    event.type == KeyEventType.KeyDown &&
                    event.key == Key.Backspace &&
                    value.selection.collapsed
                ) {
                    val caret = value.selection.start
                    val atLineStart = caret == 0 || value.text.getOrNull(caret - 1) == '\n'
                    if (atLineStart) {
                        val kind = row.paragraphs
                            .getOrNull(ParaRemap.paraIndexAt(value.text, caret))?.kind
                            ?: BlockKind.Paragraph
                        if (kind != BlockKind.Paragraph) {
                            // First press on a formatted line clears its block
                            // formatting instead of deleting into the previous
                            // line (per-row editor / iOS parity).
                            model.clearParagraphFormat(row.id, caret)
                            return@onPreviewKeyEvent true
                        }
                        if (caret == 0 && model.rows.indexOfFirst { it.id == row.id } > 0) {
                            // Run start: delete the image above and merge runs.
                            model.backspaceAtRunStart(row.id)
                            return@onPreviewKeyEvent true
                        }
                    }
                }
                false
            },
        decorationBox = { inner ->
            Box(modifier = Modifier.fillMaxWidth().padding(vertical = MdStyle.textInsetV)) {
                if (placeholder != null && row.text.isEmpty()) {
                    Text(placeholder, style = LocalTextStyle.current.copy(color = MdStyle.Placeholder))
                }
                // The Popup lives INSIDE the decoration, wrapping the glyph box:
                // that box becomes its anchorBounds and is the same coordinate
                // space getCursorRect reports in, so the menu sits at the caret
                // and tracks scroll / IME resize for free. As a sibling of the
                // field it used to anchor to the whole editor column (EXP-322).
                Box(
                    Modifier
                        .fillMaxWidth()
                        // Same coordinate space getCursorRect reports in, so the
                        // caret rect passes through to bringIntoView untranslated.
                        .bringIntoViewRequester(bringIntoViewRequester)
                        // Painted decorations first so chips draw over a code
                        // background, then chips, then the tap interceptors.
                        .drawRunDecorations(bands) { textLayout }
                        .drawIssueRefChips(chipSpecs) { textLayout }
                        .checklistTapTargets(bands, { textLayout }) { paraIndex ->
                            model.toggleChecklistChecked(row.id, paraIndex)
                        }
                        // Chips stay tappable while the row is NOT focused (iOS
                        // parity — the description editor is always the editable
                        // path on Android, so this is the only way a description
                        // chip can navigate). A focused row always wins: the
                        // caret must land where the user tapped.
                        .pointerInput(chipTransform, hasOsFocus, issueRefs) {
                            if (chipTransform.isIdentity || issueRefs?.canOpen != true) {
                                return@pointerInput
                            }
                            awaitEachGesture {
                                // The INITIAL pass, because the field's own tap
                                // handling lives on a DESCENDANT node and the
                                // main pass reaches children first: preempting a
                                // chip tap is the only way it doesn't just place
                                // the caret. Every other tap is left untouched
                                // and falls through as before.
                                val down = awaitFirstDown(
                                    requireUnconsumed = false,
                                    pass = PointerEventPass.Initial,
                                )
                                if (hasOsFocus) return@awaitEachGesture
                                val layout = textLayout ?: return@awaitEachGesture
                                val offset = runCatching {
                                    layout.getOffsetForPosition(down.position)
                                }.getOrNull() ?: return@awaitEachGesture
                                val chip = chipTransform.chips.firstOrNull {
                                    offset >= it.displayStart && offset < it.displayEnd
                                } ?: return@awaitEachGesture
                                down.consume()
                                val up = waitForUpOrCancellation(PointerEventPass.Initial)
                                    ?: return@awaitEachGesture
                                up.consume()
                                issueRefs.onOpen(chip.target)
                            }
                        }
                        // EXP-608: a tap on an EMPTY line must land the caret ON
                        // that line. Empty lines render as a lone zero-width
                        // space (the '\n' substitution in ChipVisualTransformation),
                        // and platform hit-testing resolves a tap past a
                        // zero-width glyph — to the offset AFTER it, which
                        // belongs to the NEXT paragraph. Untouched, the caret
                        // landed one line below and an empty first line could
                        // never take the caret at all. Intercept those taps
                        // (Initial pass, same preemption as the chip handler)
                        // and place the caret at the line start ourselves.
                        .pointerInput(chipTransform) {
                            awaitEachGesture {
                                val down = awaitFirstDown(
                                    requireUnconsumed = false,
                                    pass = PointerEventPass.Initial,
                                )
                                val layout = textLayout ?: return@awaitEachGesture
                                val display = layout.layoutInput.text
                                // A layout lagging the transform by a frame can
                                // misattribute lines; leave those taps to the
                                // field's own handling.
                                if (display.length != chipTransform.display.length) {
                                    return@awaitEachGesture
                                }
                                val line = layout.getLineForVerticalPosition(down.position.y)
                                val target = emptyDisplayLineStart(
                                    display.text,
                                    layout.getLineStart(line),
                                    layout.getLineEnd(line),
                                ) ?: return@awaitEachGesture
                                down.consume()
                                val up = waitForUpOrCancellation(PointerEventPass.Initial)
                                    ?: return@awaitEachGesture
                                up.consume()
                                val caret = chipTransform.transformedToOriginal(target)
                                value = value.copy(selection = TextRange(caret))
                                model.updateSelection(row.id, caret..caret)
                                if (!hasOsFocus) runCatching { focusRequester.requestFocus() }
                            }
                        },
                ) {
                    inner()
                    if (menuOpen) {
                        AutocompleteMenu(
                            caretRect = caretRect,
                            toolbarHeightPx = toolbarHeightPx,
                            mentionCandidates = mentionCandidates,
                            refCandidates = refCandidates,
                            emojiCandidates = emojiCandidates,
                            onPickMention = ::insertMention,
                            onPickIssueRef = ::insertIssueRef,
                            onPickEmoji = { insertEmoji(it, trailingSpace = true) },
                        )
                    }
                }
            }
        },
    )

    // Back closes the menu without leaving the field (registered only while the
    // menu is up, so the LIFO dispatcher gives it priority over the screen's).
    BackHandler(enabled = menuOpen) { armed = false }
}

/**
 * The display offset a tap on a laid-out line should place the caret at when
 * that line is an EMPTY source line — rendered as a lone zero-width space by
 * the '\n' substitution in [ChipVisualTransformation] — or null for every
 * other line (EXP-608). A real trailing empty line (kept as an actual '\n',
 * EXP-567) has `lineEnd == lineStart` and correctly returns null: default hit
 * testing already handles it.
 */
internal fun emptyDisplayLineStart(displayText: String, lineStart: Int, lineEnd: Int): Int? =
    if (lineEnd == lineStart + 1 && displayText.getOrNull(lineStart) == '\u200B') lineStart
    else null

@Composable
private fun AutocompleteMenu(
    caretRect: Rect?,
    toolbarHeightPx: Int,
    mentionCandidates: List<MentionMember>,
    refCandidates: List<IssueRefTarget>,
    emojiCandidates: List<EmojiRecord>,
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
        // EXP-332: the same container as every DropdownMenu in the app, so the
        // `@`/`#` menu is no longer a second menu look.
        GlassMenuSurface {
            Column(
                modifier = Modifier
                    .width(260.dp)
                    .heightIn(max = 240.dp)
                    .verticalScroll(rememberScrollState())
                    // Scrolls with the content, matching M3's menu padding.
                    .padding(vertical = 4.dp),
            ) {
                mentionCandidates.forEach { m ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 44.dp)
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
                refCandidates.forEach { target ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 44.dp)
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
                emojiCandidates.forEach { emoji ->
                    Row(
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(min = 44.dp)
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
}
