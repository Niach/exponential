package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.RichText

/**
 * Read-only markdown renderer — the shared display path for issue descriptions
 * and comment bodies. Parses GFM into blocks once (memoized on the source) and
 * renders the same per-block visuals the editor uses, minus editing affordances.
 * Replaces the `compose-rich-editor` `RichText` read path so all three clients
 * render the same contract.
 */
@Composable
fun MarkdownView(markdown: String, modifier: Modifier = Modifier) {
    if (markdown.isBlank()) return
    val blocks = remember(markdown) { MarkdownParser.parse(markdown) }
    Column(modifier = modifier.fillMaxWidth()) {
        blocks.forEach { block ->
            when (block) {
                is ContentBlock.TextBlock -> TextBlockView(block.content)
                is ContentBlock.ImageBlock -> ImageBlockView(block.url, block.alt)
            }
        }
    }
}

@Composable
private fun ImageBlockView(url: String, alt: String) {
    AsyncImage(
        model = url,
        contentDescription = alt,
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .clip(RoundedCornerShape(8.dp)),
    )
}

@Composable
private fun TextBlockView(rich: RichText) {
    if (rich.isEmpty) return
    val lines = rich.lines
    val attrs = (lines.indices).map { rich.paragraphs.getOrElse(it) { ParagraphAttrs.PLAIN } }
    val lineMarks = lineLocalMarks(rich)

    // Group consecutive code-block lines so they render in one rounded box.
    var i = 0
    Column(modifier = Modifier.fillMaxWidth()) {
        while (i < lines.size) {
            val a = attrs[i]
            if (a.kind == BlockKind.CodeBlock) {
                var j = i
                while (j + 1 < lines.size && attrs[j + 1].kind == BlockKind.CodeBlock) j++
                CodeBlockView((i..j).map { lines[it] })
                i = j + 1
            } else {
                LineView(lines[i], a, lineMarks[i])
                i++
            }
        }
    }
}

@Composable
private fun CodeBlockView(codeLines: List<String>) {
    Text(
        text = codeLines.joinToString("\n"),
        style = MdStyle.mono,
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .clip(RoundedCornerShape(6.dp))
            .background(MdStyle.CodeBlockBg)
            .padding(horizontal = 10.dp, vertical = 8.dp),
    )
}

@Composable
private fun LineView(text: String, a: ParagraphAttrs, marks: List<InlineMark>) {
    when (a.kind) {
        BlockKind.Heading -> Text(
            text = annotate(text, marks),
            style = MdStyle.heading(a.headingLevel),
            modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
        )

        BlockKind.Blockquote -> Text(
            text = annotate(text, marks),
            style = MdStyle.body.copy(color = MdStyle.Blockquote),
            modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        )

        BlockKind.ThematicBreak -> Text(
            text = MarkdownParser.THEMATIC_BREAK_GLYPH,
            style = MdStyle.body.copy(color = MdStyle.Dim),
            modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        )

        BlockKind.ListItem -> ListItemView(text, a, marks)

        BlockKind.Paragraph, BlockKind.CodeBlock -> {
            if (text.isEmpty()) {
                Spacer(Modifier.padding(vertical = 2.dp))
            } else {
                Text(
                    text = annotate(text, marks),
                    style = MdStyle.body,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
                )
            }
        }
    }
}

@Composable
private fun ListItemView(text: String, a: ParagraphAttrs, marks: List<InlineMark>) {
    val indent = MdStyle.listIndentBase + MdStyle.listIndentPerDepth * a.listDepth
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(start = indent, top = 2.dp, bottom = 2.dp),
        verticalAlignment = Alignment.Top,
    ) {
        when (a.listType) {
            ListType.Checklist -> Text(
                if (a.checked) "☑" else "☐",
                style = MdStyle.body,
                modifier = Modifier.width(22.dp),
            )
            ListType.Ordered -> Text(
                "${a.orderedIndex}.",
                style = MdStyle.body,
                modifier = Modifier.width(22.dp),
            )
            ListType.Bullet, null -> Text(
                "•",
                style = MdStyle.body,
                modifier = Modifier.width(22.dp),
            )
        }
        Text(text = annotate(text, marks), style = MdStyle.body, modifier = Modifier.fillMaxWidth())
    }
}

// --- Annotated-string builder with natively-tappable links (read-only). ---

private fun annotate(text: String, marks: List<InlineMark>): AnnotatedString {
    if (text.isEmpty()) return AnnotatedString("")
    if (marks.isEmpty()) return AnnotatedString(text)
    return buildAnnotatedString {
        append(text)
        for (m in marks) {
            val start = m.start.coerceIn(0, text.length)
            val end = m.end.coerceIn(start, text.length)
            if (end <= start) continue
            when (m.kind) {
                InlineKind.Bold -> addStyle(SpanStyle(fontWeight = FontWeight.Bold), start, end)
                InlineKind.Italic -> addStyle(SpanStyle(fontStyle = FontStyle.Italic), start, end)
                InlineKind.Strikethrough ->
                    addStyle(SpanStyle(textDecoration = TextDecoration.LineThrough), start, end)
                InlineKind.InlineCode -> addStyle(
                    SpanStyle(fontFamily = FontFamily.Monospace, background = MdStyle.InlineCodeBg),
                    start, end,
                )
                InlineKind.Link -> {
                    val href = m.href ?: continue
                    addLink(
                        LinkAnnotation.Url(
                            url = href,
                            styles = TextLinkStyles(style = SpanStyle(color = MdStyle.Link)),
                        ),
                        start, end,
                    )
                }
            }
        }
    }
}

/** Offset each block's marks into per-line-local coordinates. */
internal fun lineLocalMarks(rich: RichText): List<List<InlineMark>> {
    val lines = rich.lines
    val result = ArrayList<List<InlineMark>>(lines.size)
    var charStart = 0
    for (line in lines) {
        val lineStart = charStart
        val lineEnd = charStart + line.length
        val local = rich.marks.mapNotNull { m ->
            val s = maxOf(m.start, lineStart)
            val e = minOf(m.end, lineEnd)
            if (e > s) m.copy(start = s - lineStart, end = e - lineStart) else null
        }
        result.add(local)
        charStart = lineEnd + 1 // + '\n'
    }
    return result
}
