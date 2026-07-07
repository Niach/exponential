package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.LocalTextStyle
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
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

    // Re-seed from the model only on structural/external change (revision bump).
    LaunchedEffect(revision) {
        val caret = model.consumeDesiredSelection(row.id) ?: value.selection.start.coerceIn(0, row.text.length)
        if (value.text != row.text || value.selection.start != caret) {
            value = TextFieldValue(text = row.text, selection = TextRange(caret.coerceIn(0, row.text.length)))
        }
    }

    val focusRequester = remember { FocusRequester() }
    LaunchedEffect(model.focusedRowId) {
        if (model.focusedRowId == row.id) {
            // A freshly-created row (Enter/merge/insert) may not be laid out yet
            // when this effect first runs; requestFocus() throws until the node
            // is placed. Retry across frames so focus reliably lands on the new
            // row instead of silently staying on the old one.
            var attempts = 0
            while (attempts < 8 && model.focusedRowId == row.id) {
                if (runCatching { focusRequester.requestFocus() }.isSuccess) break
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
    }

    // #issue-ref autocomplete (masterplan §5e): detect an in-progress `#query`
    // before the caret and offer same-workspace issues from [LocalIssueRefs]
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
    }

    val attrs = row.attrs
    val textStyle = paragraphTextStyle(attrs)
    val marks = row.marks

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
                value = new
                if (new.text != row.text) model.updatePara(row.id, new.text, new.selection.start)
                model.updateSelection(row.id, new.selection.start..new.selection.end)
            }
        },
        textStyle = textStyle,
        cursorBrush = SolidColor(MdStyle.Link),
        visualTransformation = InlineMarkVisualTransformation(marks),
        modifier = modifier
            .focusRequester(focusRequester)
            .onFocusChanged { fs ->
                if (fs.isFocused) model.setFocused(row.id) else model.clearFocusIfMatches(row.id)
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
                inner = inner,
            )
        },
    )

    if (mentionCandidates.isNotEmpty() || refCandidates.isNotEmpty()) {
        val provider = remember {
            object : PopupPositionProvider {
                override fun calculatePosition(
                    anchorBounds: IntRect,
                    windowSize: IntSize,
                    layoutDirection: LayoutDirection,
                    popupContentSize: IntSize,
                ): IntOffset = IntOffset(anchorBounds.left, anchorBounds.bottom)
            }
        }
        Popup(
            popupPositionProvider = provider,
            properties = PopupProperties(focusable = false),
        ) {
            Surface(
                shape = RoundedCornerShape(8.dp),
                color = MaterialTheme.colorScheme.surfaceContainerHigh,
                tonalElevation = 4.dp,
                shadowElevation = 8.dp,
            ) {
                Column(modifier = Modifier.width(260.dp)) {
                    mentionCandidates.forEach { m ->
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .clickable { insertMention(m) }
                                .padding(horizontal = 10.dp, vertical = 8.dp),
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
                                .clickable { insertIssueRef(target) }
                                .padding(horizontal = 10.dp, vertical = 8.dp),
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
            Box(
                modifier = Modifier
                    .padding(vertical = 1.dp)
                    .clip(RoundedCornerShape(4.dp))
                    .background(MdStyle.CodeBlockBg)
                    .padding(horizontal = 8.dp, vertical = 4.dp),
            ) { inner() }
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

/** Applies inline marks as cosmetic spans; identity offset mapping (no chars added). */
private class InlineMarkVisualTransformation(
    private val marks: List<com.exponential.app.ui.markdown.model.InlineMark>,
) : VisualTransformation {
    override fun filter(text: androidx.compose.ui.text.AnnotatedString): TransformedText {
        val annotated = InlineMarks.annotate(text.text, marks)
        return TransformedText(annotated, OffsetMapping.Identity)
    }
}
