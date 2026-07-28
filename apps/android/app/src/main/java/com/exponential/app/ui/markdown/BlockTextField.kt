package com.exponential.app.ui.markdown

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.draw.clip
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
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
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
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs

// In-progress mention `@query` at the caret (after start-of-text or whitespace);
// the query stops at whitespace. Mirrors apps/web/src/components/mention-textarea.tsx.
private val MENTION_AT_CARET = Regex("(?:^|\\s)@([A-Za-z0-9._%+-]*)$")

// In-progress issue reference `#query` at the caret — same shape as the web
// ISSUE_REF_AT_CARET (mention-textarea.tsx / editor-autocomplete.ts).
private val ISSUE_REF_AT_CARET = Regex("(?:^|\\s)#([A-Za-z0-9-]*)$")

/**
 * One editable paragraph line, backed by a [BasicTextField]. Per-paragraph
 * styling (heading size / list glyph / indent / code background / quote color)
 * lives in the decoration so the editable text stays glyph-free. Enter splits
 * the paragraph and Backspace-at-start merges with the previous row — both routed
 * through [EditorModel]. The field only re-seeds its value when the row's
 * revision bumps (structural change), never on the user's own keystrokes.
 */
@Composable
fun BlockTextField(
    model: EditorModel,
    row: EditorRow.Para,
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
        // indistinguishable from an Enter split — hence the explicit signal.
        if (model.consumeAutocompleteArm(row.id)) armed = true
    }

    val focusRequester = remember { FocusRequester() }
    // Whether THIS field currently holds OS focus. Two focus-handoff guards
    // hang off it (EXP-25 — Enter left the caret in the old row):
    //  - a freshly-composed row emits an initial focused=false event; without
    //    the guard below that cleared the focusedRowId splitParagraphFrom had
    //    just pointed at the new row, so its requestFocus loop never ran and
    //    the old field kept focus.
    //  - requestFocus() on a not-yet-placed node can no-op WITHOUT throwing,
    //    so the retry loop must verify focus actually landed instead of
    //    trusting the absence of an exception.
    var hasOsFocus by remember(row.id) { mutableStateOf(false) }
    LaunchedEffect(model.focusedRowId) {
        if (model.focusedRowId == row.id) {
            // A freshly-created row (Enter/merge/insert) may not be laid out yet
            // when this effect first runs; requestFocus() throws (or silently
            // does nothing) until the node is placed. Retry across frames until
            // the focus change is actually observed, so focus deterministically
            // lands on the new row instead of staying on the old one.
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
        model.updatePara(row.id, newText, newCaret)
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
        model.updatePara(row.id, newText, newCaret)
        model.updateSelection(row.id, newCaret..newCaret)
        armed = false
    }

    val attrs = row.attrs
    val textStyle = paragraphTextStyle(attrs)
    val marks = row.marks
    val chipsEnabled = attrs.kind != BlockKind.CodeBlock

    val menuOpen = shouldOpenAutocomplete(
        armed = armed,
        hasOsFocus = hasOsFocus,
        isFocusedRow = model.focusedRowId == row.id,
        kind = attrs.kind,
        caretInInlineCode = caretInInlineCode(marks, value.selection.start),
        hasCandidates = mentionCandidates.isNotEmpty() || refCandidates.isNotEmpty(),
    )
    // The regex stopped matching (caret left the token, whitespace typed, the
    // trigger was deleted) — require a fresh text change to reopen.
    LaunchedEffect(mentionMatch == null && refMatch == null) {
        if (mentionMatch == null && refMatch == null) armed = false
    }

    // Caret geometry for the menu's anchor, in the text-glyph box's own
    // coordinates (see the Popup below).
    var textLayout by remember(row.id) { mutableStateOf<TextLayoutResult?>(null) }
    val chipTransform = remember(value.text, marks, issueRefs, chipsEnabled) {
        IssueChipTransform.build(value.text, marks, issueRefs, chipsEnabled)
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

    BasicTextField(
        value = value,
        onValueChange = { new ->
            if (new.text.contains('\n')) {
                // Newline(s) arrived — either Enter (one '\n' replacing the
                // selection) or a multi-line paste. Apply against the POST-EDIT
                // text so a replaced selection is honored and no characters are
                // dropped; splitParagraphFrom handles 1..N resulting lines.
                model.splitParagraphFrom(row.id, new.text)
            } else {
                // Only a real document change may open the menu (web parity).
                if (new.text != value.text) armed = true
                value = new
                if (new.text != row.text) model.updatePara(row.id, new.text, new.selection.start)
                model.updateSelection(row.id, new.selection.start..new.selection.end)
            }
        },
        textStyle = textStyle,
        onTextLayout = { textLayout = it },
        cursorBrush = SolidColor(MdStyle.Link),
        // Resolved `#IDENTIFIER` tokens render as `#EXP-238 <title>` chips
        // while editing (EXP-322, web parity) — display-only, the stored
        // markdown keeps the bare token. Code rows opt out, like read mode.
        visualTransformation = ChipVisualTransformation(
            marks = marks,
            issueRefs = issueRefs,
            chipsEnabled = chipsEnabled,
        ),
        modifier = modifier
            .focusRequester(focusRequester)
            .onFocusChanged { fs ->
                if (fs.isFocused) {
                    hasOsFocus = true
                    model.setFocused(row.id)
                } else {
                    // Only a field that actually HELD focus may clear the model's
                    // focus target — the initial focused=false of a row created
                    // by splitParagraphFrom must not cancel its pending handoff.
                    if (hasOsFocus) model.clearFocusIfMatches(row.id)
                    hasOsFocus = false
                    armed = false
                }
            }
            .onPreviewKeyEvent { event ->
                if (
                    event.type == KeyEventType.KeyDown &&
                    event.key == Key.Backspace &&
                    value.selection.collapsed &&
                    value.selection.start == 0
                ) {
                    val canHandle = attrs.kind != BlockKind.Paragraph || model.rows.indexOfFirst { it.id == row.id } > 0
                    if (canHandle) {
                        model.backspaceAtStart(row.id)
                        return@onPreviewKeyEvent true
                    }
                }
                false
            },
        decorationBox = { inner ->
            ParagraphDecoration(
                model = model,
                row = row,
                showPlaceholder = placeholder != null && row.text.isEmpty(),
                placeholder = placeholder,
            ) {
                // The Popup lives INSIDE the decoration, wrapping the glyph box:
                // that box becomes its anchorBounds and is the same coordinate
                // space getCursorRect reports in, so the menu sits at the caret
                // and tracks scroll / IME resize for free. As a sibling of the
                // field it used to anchor to the whole editor column (EXP-322).
                Box {
                    inner()
                    if (menuOpen) {
                        AutocompleteMenu(
                            caretRect = caretRect,
                            toolbarHeightPx = toolbarHeightPx,
                            mentionCandidates = mentionCandidates,
                            refCandidates = refCandidates,
                            onPickMention = ::insertMention,
                            onPickIssueRef = ::insertIssueRef,
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

@Composable
private fun AutocompleteMenu(
    caretRect: Rect?,
    toolbarHeightPx: Int,
    mentionCandidates: List<MentionMember>,
    refCandidates: List<IssueRefTarget>,
    onPickMention: (MentionMember) -> Unit,
    onPickIssueRef: (IssueRefTarget) -> Unit,
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
                        Text(
                            target.identifier,
                            style = MaterialTheme.typography.labelMedium,
                            fontFamily = FontFamily.Monospace,
                            maxLines = 1,
                        )
                        Spacer(Modifier.width(8.dp))
                        Text(
                            target.title,
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

@Composable
private fun ParagraphDecoration(
    model: EditorModel,
    row: EditorRow.Para,
    showPlaceholder: Boolean,
    placeholder: String?,
    inner: @Composable () -> Unit,
) {
    val attrs = row.attrs
    when (attrs.kind) {
        BlockKind.ListItem -> {
            val indent = MdStyle.listIndentBase + MdStyle.listIndentPerDepth * attrs.listDepth
            Row(modifier = Modifier.padding(start = indent, top = 2.dp, bottom = 2.dp), verticalAlignment = Alignment.Top) {
                ListGlyph(model, row, attrs)
                Box(Modifier.weight(1f)) { inner() }
            }
        }

        BlockKind.CodeBlock -> {
            // Consecutive code rows merge into ONE visual box (EXP-246): corners
            // round and outer/inner vertical padding apply only at the run's
            // edges, so an N-line fence reads as a single connected block —
            // read-view CodeBlockView parity. Neighbor kinds come straight off
            // model.rows (snapshot state — recomposes when a neighbor changes).
            val rows = model.rows
            val idx = rows.indexOfFirst { it.id == row.id }
            fun kindAt(i: Int) = (rows.getOrNull(i) as? EditorRow.Para)?.attrs?.kind
            val joinsPrev = kindAt(idx - 1) == BlockKind.CodeBlock
            val joinsNext = kindAt(idx + 1) == BlockKind.CodeBlock
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = if (joinsPrev) 0.dp else 1.dp, bottom = if (joinsNext) 0.dp else 1.dp)
                    .clip(
                        RoundedCornerShape(
                            topStart = if (joinsPrev) 0.dp else 4.dp,
                            topEnd = if (joinsPrev) 0.dp else 4.dp,
                            bottomStart = if (joinsNext) 0.dp else 4.dp,
                            bottomEnd = if (joinsNext) 0.dp else 4.dp,
                        ),
                    )
                    .background(MdStyle.CodeBlockBg)
                    .padding(
                        start = 8.dp,
                        end = 8.dp,
                        top = if (joinsPrev) 1.dp else 4.dp,
                        bottom = if (joinsNext) 1.dp else 4.dp,
                    ),
            ) { inner() }
        }

        BlockKind.Blockquote -> {
            // Linear-style quote (EXP-246): vertical left bar + indented text.
            // The bar fills the row's full height and rows stack flush in the
            // editor column, so a multi-line quote reads as one continuous bar.
            Row(modifier = Modifier.height(IntrinsicSize.Min)) {
                Box(
                    Modifier
                        .width(MdStyle.quoteBarWidth)
                        .fillMaxHeight()
                        .background(MdStyle.QuoteBar),
                )
                Spacer(Modifier.width(MdStyle.quoteIndent))
                Box(Modifier.weight(1f).padding(vertical = MdStyle.textInsetV)) { inner() }
            }
        }

        else -> {
            Box(modifier = Modifier.padding(vertical = MdStyle.textInsetV)) {
                if (showPlaceholder && placeholder != null) {
                    Text(placeholder, style = LocalTextStyle.current.copy(color = MdStyle.Placeholder))
                }
                inner()
            }
        }
    }
}

@Composable
private fun ListGlyph(model: EditorModel, row: EditorRow.Para, attrs: ParagraphAttrs) {
    when (attrs.listType) {
        ListType.Checklist -> Text(
            text = if (attrs.checked) "☑" else "☐",
            style = MdStyle.body,
            modifier = Modifier
                .width(24.dp)
                .padding(end = 2.dp)
                .clickable { model.toggleChecklistChecked(row.id) },
        )
        ListType.Ordered -> Text(
            text = "${attrs.orderedIndex}.",
            style = MdStyle.body,
            modifier = Modifier.width(24.dp),
        )
        ListType.Bullet, null -> Text(
            text = "•",
            style = MdStyle.body,
            modifier = Modifier.width(24.dp),
        )
    }
}

private fun paragraphTextStyle(attrs: ParagraphAttrs): TextStyle = when (attrs.kind) {
    BlockKind.Heading -> MdStyle.heading(attrs.headingLevel)
    BlockKind.CodeBlock -> MdStyle.mono
    BlockKind.Blockquote -> MdStyle.body.copy(color = MdStyle.Blockquote)
    else -> MdStyle.body
}

