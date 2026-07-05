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
 * render the same contract. When a [LocalIssueRefs] handler is provided,
 * inline `#IDENTIFIER` tokens that resolve to a visible issue render as
 * tappable pills (render-only — the stored markdown keeps the plain token).
 */
@Composable
fun MarkdownView(markdown: String, modifier: Modifier = Modifier) {
    if (markdown.isBlank()) return
    val blocks = remember(markdown) { MarkdownParser.parse(markdown) }
    val issueRefs = LocalIssueRefs.current
    Column(modifier = modifier.fillMaxWidth()) {
        blocks.forEach { block ->
            when (block) {
                is ContentBlock.TextBlock -> TextBlockView(block.content, issueRefs)
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
private fun TextBlockView(rich: RichText, issueRefs: IssueRefHandler?) {
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
                LineView(lines[i], a, lineMarks[i], issueRefs)
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
private fun LineView(
    text: String,
    a: ParagraphAttrs,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler?,
) {
    when (a.kind) {
        BlockKind.Heading -> Text(
            text = annotate(text, marks, issueRefs),
            style = MdStyle.heading(a.headingLevel),
            modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
        )

        BlockKind.Blockquote -> Text(
            text = annotate(text, marks, issueRefs),
            style = MdStyle.body.copy(color = MdStyle.Blockquote),
            modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        )

        BlockKind.ThematicBreak -> Text(
            text = MarkdownParser.THEMATIC_BREAK_GLYPH,
            style = MdStyle.body.copy(color = MdStyle.Dim),
            modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
        )

        BlockKind.ListItem -> ListItemView(text, a, marks, issueRefs)

        BlockKind.Paragraph, BlockKind.CodeBlock -> {
            if (text.isEmpty()) {
                Spacer(Modifier.padding(vertical = 2.dp))
            } else {
                Text(
                    text = annotate(text, marks, issueRefs),
                    style = MdStyle.body,
                    modifier = Modifier.fillMaxWidth().padding(vertical = 2.dp),
                )
            }
        }
    }
}

@Composable
private fun ListItemView(
    text: String,
    a: ParagraphAttrs,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler?,
) {
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
        Text(
            text = annotate(text, marks, issueRefs),
            style = MdStyle.body,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

// --- Annotated-string builder with natively-tappable links (read-only). ---

internal fun annotate(
    text: String,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler?,
): AnnotatedString {
    if (text.isEmpty()) return AnnotatedString("")
    val refPills = if (issueRefs != null) resolvedRefPills(text, marks, issueRefs) else emptyList()
    if (marks.isEmpty() && refPills.isEmpty()) return AnnotatedString(text)
    return buildAnnotatedString {
        append(text)
        // Compose crashes if two `LinkAnnotation`s overlap (issue-detail crash,
        // masterplan §9.3). Every `addLink` must be range-coerced into the
        // appended text AND rejected if it overlaps an already-added link, so
        // no combination of markdown links + `#IDENTIFIER` pills can throw.
        val linkRanges = ArrayList<Pair<Int, Int>>() // half-open [start, end)
        fun addLinkGuarded(annotation: LinkAnnotation, rawStart: Int, rawEnd: Int) {
            val start = rawStart.coerceIn(0, text.length)
            val end = rawEnd.coerceIn(start, text.length)
            if (end <= start) return
            if (linkRanges.any { it.first < end && start < it.second }) return
            when (annotation) {
                is LinkAnnotation.Url -> addLink(annotation, start, end)
                is LinkAnnotation.Clickable -> addLink(annotation, start, end)
            }
            linkRanges.add(start to end)
        }
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
                    addLinkGuarded(
                        LinkAnnotation.Url(
                            url = href,
                            styles = TextLinkStyles(style = SpanStyle(color = MdStyle.Link)),
                        ),
                        start, end,
                    )
                }
            }
        }
        // Resolved `#IDENTIFIER` tokens become tappable pills (masterplan §5e),
        // mirroring how the web renderer decorates them and how `@email`
        // mentions pill: display-only, navigation on tap. Guarded so a pill
        // that lands on an existing link span is dropped instead of crashing.
        for ((match, target) in refPills) {
            addLinkGuarded(
                LinkAnnotation.Clickable(
                    tag = target.identifier,
                    styles = TextLinkStyles(
                        style = SpanStyle(
                            color = MdStyle.Link,
                            background = MdStyle.IssueRefBg,
                            fontFamily = FontFamily.Monospace,
                        ),
                    ),
                    linkInteractionListener = { issueRefs?.onOpen?.invoke(target) },
                ),
                match.start, match.end,
            )
        }
    }
}

/**
 * `#IDENTIFIER` tokens in this line that resolve to a visible issue. Tokens
 * inside inline code or links stay plain (mirrors the web decoration pass);
 * unresolved identifiers stay plain text.
 */
private fun resolvedRefPills(
    text: String,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler,
): List<Pair<IssueRefs.Match, IssueRefTarget>> =
    IssueRefs.findAll(text).mapNotNull { match ->
        val covered = marks.any { m ->
            (m.kind == InlineKind.InlineCode || m.kind == InlineKind.Link) &&
                m.start < match.end && match.start < m.end
        }
        if (covered) return@mapNotNull null
        issueRefs.resolve(match.identifier)?.let { match to it }
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
