package com.exponential.app.ui.markdown

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import coil3.compose.AsyncImagePainter
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.ContentBlock
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ListType
import com.exponential.app.ui.markdown.model.ParagraphAttrs
import com.exponential.app.ui.markdown.model.RichText
import com.exponential.app.ui.theme.resolvedStatusColor

/**
 * Read-only markdown renderer — the shared display path for issue descriptions
 * and comment bodies. Parses GFM into blocks once (memoized on the source) and
 * renders the same per-block visuals the editor uses, minus editing affordances.
 * Replaces the `compose-rich-editor` `RichText` read path so all three clients
 * render the same contract. When a [LocalIssueRefs] handler is provided,
 * inline `#IDENTIFIER` tokens that resolve to a visible issue render as
 * tappable pills, and with a [LocalMentions] resolver a known member's
 * `@email` renders as their name pill (both render-only — the stored markdown
 * keeps the plain tokens).
 */
@Composable
fun MarkdownView(
    markdown: String,
    modifier: Modifier = Modifier,
    /** Render every soft line break as its own line instead of a space —
     *  agent output (EXP-440) is written for a terminal, where a wrapped
     *  paragraph is really a list of lines. */
    softBreaksAsNewlines: Boolean = false,
) {
    if (markdown.isBlank()) return
    val blocks = remember(markdown, softBreaksAsNewlines) {
        MarkdownParser.parse(markdown, softBreaksAsNewlines)
    }
    val issueRefs = LocalIssueRefs.current
    // The whole rendered document is one selection scope (EXP-534): long-press
    // selection spans blocks AND images (the text on both sides selects; the
    // image itself is not selectable content). Links/chips stay tappable —
    // SelectionContainer only adds the long-press gesture. The caller's
    // modifier goes on the container so parent-data modifiers (weight) land on
    // the layout parent's direct child.
    SelectionContainer(modifier = modifier) {
        Column(modifier = Modifier.fillMaxWidth()) {
            blocks.forEach { block ->
                when (block) {
                    is ContentBlock.TextBlock -> TextBlockView(block.content, issueRefs)
                    is ContentBlock.ImageBlock -> ImageBlockView(block.url, block.alt)
                }
            }
        }
    }
}

@Composable
private fun ImageBlockView(url: String, alt: String) {
    // Pre-size from the synced attachment probe (REV2-79) so the row reserves
    // its real height instead of measuring 0 and jumping when the bitmap
    // lands. Our own attachments whose probe hasn't synced yet reserve the
    // editor's 4:3 tile; EXTERNAL image URLs (never probed) keep their natural
    // sizing rather than being letterboxed forever. Once the bitmap decodes,
    // ITS ratio wins (iOS `ImageBlockView.aspectRatio` parity) — the agent
    // feed renders attachments of OTHER issues (a batch prompt's embeds,
    // EXP-605) whose probes never sync here, and holding the 4:3 tile past
    // decode letterboxes those forever.
    val dims = LocalAttachmentDims.current
    var decodedAspect by remember(url) { mutableStateOf<Float?>(null) }
    val aspect = decodedAspect
        ?: dims.aspectRatioOf(url)
        ?: if (attachmentIdFromUrl(url) != null) DEFAULT_IMAGE_ASPECT_RATIO else null
    AsyncImage(
        model = url,
        contentDescription = alt,
        contentScale = ContentScale.Fit,
        onState = { state ->
            val size = (state as? AsyncImagePainter.State.Success)?.painter?.intrinsicSize
            if (size != null && size.isSpecified && size.width > 0f && size.height > 0f) {
                decodedAspect = size.width / size.height
            }
        },
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .then(if (aspect != null) Modifier.aspectRatio(aspect) else Modifier)
            .clip(RoundedCornerShape(8.dp)),
    )
}

@Composable
private fun TextBlockView(rich: RichText, issueRefs: IssueRefHandler?) {
    if (rich.isEmpty) return
    val lines = rich.lines
    val attrs = (lines.indices).map { rich.paragraphs.getOrElse(it) { ParagraphAttrs.PLAIN } }
    val lineMarks = lineLocalMarks(rich)

    // Group consecutive code-block lines so they render in one rounded box,
    // and consecutive quote lines behind one continuous left bar (EXP-246).
    var i = 0
    Column(modifier = Modifier.fillMaxWidth()) {
        while (i < lines.size) {
            val a = attrs[i]
            if (a.kind == BlockKind.CodeBlock) {
                var j = i
                while (j + 1 < lines.size && attrs[j + 1].kind == BlockKind.CodeBlock) j++
                CodeBlockView((i..j).map { lines[it] })
                i = j + 1
            } else if (a.kind == BlockKind.Blockquote) {
                var j = i
                while (j + 1 < lines.size && attrs[j + 1].kind == BlockKind.Blockquote) j++
                QuoteBlockView((i..j).map { lines[it] }, (i..j).map { lineMarks[it] }, issueRefs)
                i = j + 1
            } else {
                LineView(lines[i], a, lineMarks[i], issueRefs)
                i++
            }
        }
    }
}

// A run of consecutive quote lines: one continuous vertical bar with the muted
// quoted text indented beside it (Linear-style, EXP-246) — matches the editor's
// Blockquote ParagraphDecoration.
@Composable
private fun QuoteBlockView(
    texts: List<String>,
    marks: List<List<InlineMark>>,
    issueRefs: IssueRefHandler?,
) {
    val mentions = LocalMentions.current
    val autolink = LocalMarkdownAutolink.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 2.dp)
            .height(IntrinsicSize.Min),
    ) {
        Box(
            Modifier
                .width(MdStyle.quoteBarWidth)
                .fillMaxHeight()
                .background(MdStyle.QuoteBar),
        )
        Spacer(Modifier.width(MdStyle.quoteIndent))
        Column(Modifier.weight(1f)) {
            texts.forEachIndexed { index, text ->
                key(index) {
                    ChipText(
                        line = annotateLine(text, marks[index], issueRefs, mentions, autolink),
                        style = MdStyle.body.copy(color = MdStyle.Blockquote),
                        modifier = Modifier.fillMaxWidth().padding(vertical = 1.dp),
                    )
                }
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
    val mentions = LocalMentions.current
    val autolink = LocalMarkdownAutolink.current
    when (a.kind) {
        BlockKind.Heading -> ChipText(
            line = annotateLine(text, marks, issueRefs, mentions, autolink),
            style = MdStyle.heading(a.headingLevel),
            modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
        )

        // Blockquote never reaches here — TextBlockView groups quote runs into
        // QuoteBlockView (runs of length ≥ 1), like CodeBlock.
        BlockKind.Blockquote -> Unit

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
                ChipText(
                    line = annotateLine(text, marks, issueRefs, mentions, autolink),
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
    val mentions = LocalMentions.current
    val autolink = LocalMarkdownAutolink.current
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
        ChipText(
            line = annotateLine(text, marks, issueRefs, mentions, autolink),
            style = MdStyle.body,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

/**
 * A markdown line that can host `#IDENTIFIER` chips: it keeps its own
 * [TextLayoutResult] so [drawIssueRefChips] can paint each chip's rounded fill,
 * hairline and status glyph behind the text (EXP-423).
 */
@Composable
private fun ChipText(line: AnnotatedLine, style: TextStyle, modifier: Modifier = Modifier) {
    var layout by remember { mutableStateOf<TextLayoutResult?>(null) }
    Text(
        text = line.text,
        style = style,
        onTextLayout = { layout = it },
        modifier = modifier.drawIssueRefChips(line.chips) { layout },
    )
}

// --- Annotated-string builder with natively-tappable links (read-only). ---

/**
 * Bare `http(s)://` URLs render as tappable links (EXP-440) — for RENDER-ONLY
 * surfaces such as the agent steering feed, whose text nobody edits back into
 * stored markdown. Default OFF: issue descriptions and comments round-trip
 * through the editor, and autolinking there would diverge the stored bytes
 * from web (see [MarkdownParser]'s note on the autolink extension).
 */
val LocalMarkdownAutolink = compositionLocalOf { false }

/**
 * One rendered line: its styled text plus the chip geometry the painter needs
 * (EXP-423 — a rounded fill, a hairline and a status glyph are not expressible
 * as spans).
 */
internal data class AnnotatedLine(
    val text: AnnotatedString,
    val chips: List<IssueRefChipSpec> = emptyList(),
)

/** [annotateLine]'s text half — the form the annotation tests assert on. */
internal fun annotate(
    text: String,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler?,
    mentions: MentionResolver? = null,
    autolink: Boolean = false,
): AnnotatedString = annotateLine(text, marks, issueRefs, mentions, autolink).text

internal fun annotateLine(
    text: String,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler?,
    mentions: MentionResolver? = null,
    autolink: Boolean = false,
): AnnotatedLine {
    if (text.isEmpty()) return AnnotatedLine(AnnotatedString(""))
    val refPills = if (issueRefs != null) resolvedRefPills(text, marks, issueRefs) else emptyList()
    val mentionPills =
        if (mentions != null) resolvedMentionPills(text, marks, mentions) else emptyList()
    val bareUrls = if (autolink) bareUrlLinks(text, marks) else emptyList()
    if (marks.isEmpty() && refPills.isEmpty() && mentionPills.isEmpty() && bareUrls.isEmpty()) {
        return AnnotatedLine(AnnotatedString(text))
    }
    val chipSpecs = ArrayList<IssueRefChipSpec>(refPills.size)
    // A resolved mention renders the member's NAME over the stored `@email`,
    // and a resolved `#IDENTIFIER` chip appends the issue TITLE (EXP-307, web
    // read-only parity), so the displayed string is no longer the source
    // string — every other span offset is mapped through [display].
    val display = MentionDisplay.build(text, mentionPills, refPills)
    val annotated = buildAnnotatedString {
        append(display.text)
        // Compose crashes if two `LinkAnnotation`s overlap (issue-detail crash,
        // masterplan §9.3). Every `addLink` must be range-coerced into the
        // appended text AND rejected if it overlaps an already-added link, so
        // no combination of markdown links + `#IDENTIFIER` pills can throw.
        val linkRanges = ArrayList<Pair<Int, Int>>() // half-open [start, end)
        // Takes SOURCE offsets and maps them onto the displayed string.
        // Returns whether the link was added (false = dropped by a guard).
        fun addLinkGuarded(annotation: LinkAnnotation, rawStart: Int, rawEnd: Int): Boolean {
            val start = display.map(rawStart.coerceIn(0, text.length))
            val end = display.map(rawEnd.coerceIn(0, text.length)).coerceAtLeast(start)
            if (end <= start) return false
            if (linkRanges.any { it.first < end && start < it.second }) return false
            when (annotation) {
                is LinkAnnotation.Url -> addLink(annotation, start, end)
                is LinkAnnotation.Clickable -> addLink(annotation, start, end)
            }
            linkRanges.add(start to end)
            return true
        }
        for (m in marks) {
            val rawStart = m.start.coerceIn(0, text.length)
            val rawEnd = m.end.coerceIn(rawStart, text.length)
            if (rawEnd <= rawStart) continue
            val start = display.map(rawStart)
            val end = display.map(rawEnd).coerceAtLeast(start)
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
                        rawStart, rawEnd,
                    )
                }
            }
        }
        // Resolved `#IDENTIFIER` tokens become tappable pills (masterplan §5e),
        // mirroring how the web renderer decorates them and how `@email`
        // mentions pill: display-only, navigation on tap. Guarded so a pill
        // that lands on an existing link span is dropped instead of crashing.
        // The chip covers the token PLUS the appended title (EXP-307).
        //
        // The link carries NO styles (EXP-423): the Linear chip needs a
        // per-character span (a transparent `#`) and a painted background, and
        // `TextLinkStyles` applies one flat style to the whole range. So the
        // annotation is reduced to its click behaviour and the visuals become
        // plain spans + [drawIssueRefChips].
        for ((match, target) in refPills) {
            val added = addLinkGuarded(
                LinkAnnotation.Clickable(
                    tag = target.identifier,
                    linkInteractionListener = { issueRefs?.onOpen?.invoke(target) },
                ),
                match.start, match.end,
            )
            if (!added) continue
            val chipStart = display.map(match.start.coerceIn(0, text.length))
            val chipEnd = display.map(match.end.coerceIn(0, text.length)).coerceAtLeast(chipStart)
            val tokenEnd = (chipStart + (match.end - match.start)).coerceAtMost(chipEnd)
            // Title in the normal text color, identifier muted + monospace.
            addStyle(SpanStyle(color = MdStyle.Text), chipStart, chipEnd)
            addStyle(
                SpanStyle(fontFamily = FontFamily.Monospace, color = MdStyle.ChipToken),
                chipStart,
                tokenEnd,
            )
            val status = target.resolvedStatus
            if (status != null && chipStart < chipEnd) {
                // The status glyph is painted over the `#`; hiding it costs one
                // transparent character span and zero offset-map changes. The
                // letter spacing widens that cell into the icon↔identifier gap
                // (EXP-655, same as the editor's addChipStyles).
                addStyle(
                    SpanStyle(color = Color.Transparent, letterSpacing = MdStyle.chipHashLetterSpacing),
                    chipStart,
                    chipStart + 1,
                )
            }
            chipSpecs.add(
                IssueRefChipSpec(
                    start = chipStart,
                    end = chipEnd,
                    tokenStart = chipStart,
                    iconName = status?.iconName,
                    color = status?.let { resolvedStatusColor(it) },
                ),
            )
        }
        // Bare URLs last (EXP-440): a markdown link or a `#IDENTIFIER` chip
        // over the same characters always wins, and the guard drops the
        // autolink instead of stacking a second annotation on the range.
        for (range in bareUrls) {
            addLinkGuarded(
                LinkAnnotation.Url(
                    url = range.url,
                    styles = TextLinkStyles(style = SpanStyle(color = MdStyle.Link)),
                ),
                range.start, range.end,
            )
        }
        // Resolved `@email` mentions render as the member's name pill
        // (REV2-42) — display-only, not tappable (there is nothing to open),
        // so they can never collide with a link annotation.
        for (range in display.pills) {
            addStyle(
                SpanStyle(color = MdStyle.Link, background = MdStyle.IssueRefBg),
                range.first,
                range.second,
            )
        }
    }
    return AnnotatedLine(annotated, chipSpecs)
}

/** Keep chips readable (web parity: `MAX_CHIP_TITLE_LENGTH` = 60). */
internal const val MAX_CHIP_TITLE_CHARS = 60

/**
 * The title text a `#IDENTIFIER` chip appends, truncated web-identically
 * (`chipTitle` in `apps/web/src/lib/issue-ref-extension.ts`). Shared by the
 * read renderer and the editor's chip transform so both agree byte for byte.
 */
internal fun chipTitle(title: String): String {
    val trimmed = title.trim()
    if (trimmed.length <= MAX_CHIP_TITLE_CHARS) return trimmed
    return trimmed.take(MAX_CHIP_TITLE_CHARS - 1).trimEnd() + "…"
}

/**
 * `#IDENTIFIER` tokens in this line that resolve to a visible issue. Tokens
 * inside inline code or links stay plain (mirrors the web decoration pass);
 * unresolved identifiers stay plain text.
 */
internal fun resolvedRefPills(
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

/**
 * `@email` tokens in this line that resolve to a visible team member. Tokens
 * inside inline code or a markdown link stay plain (same rule as the
 * `#IDENTIFIER` pills), and unknown addresses stay plain text.
 */
private fun resolvedMentionPills(
    text: String,
    marks: List<InlineMark>,
    mentions: MentionResolver,
): List<Pair<Mentions.Match, MentionMember>> =
    Mentions.findAll(text).mapNotNull { match ->
        val covered = marks.any { m ->
            (m.kind == InlineKind.InlineCode || m.kind == InlineKind.Link) &&
                m.start < match.end && match.start < m.end
        }
        if (covered) return@mapNotNull null
        mentions.resolve(match.email)?.let { match to it }
    }

/**
 * Bare `http(s)://` URLs in this line that should autolink (EXP-440). A URL
 * inside inline code stays literal and one inside a markdown link is already
 * tappable — the same coverage rule the `#IDENTIFIER` and `@email` pills use.
 */
private fun bareUrlLinks(text: String, marks: List<InlineMark>): List<UrlRange> =
    urlRanges(text).filterNot { range ->
        marks.any { m ->
            (m.kind == InlineKind.InlineCode || m.kind == InlineKind.Link) &&
                m.start < range.end && range.start < m.end
        }
    }

/**
 * The rendered string when mention tokens are replaced by member names and
 * `#IDENTIFIER` chips gain their issue title (EXP-307), plus the
 * source→display offset mapping every other span is placed through.
 * Desktop parity (`markdown::editor`'s decoration pass): the STORED markdown
 * is untouched — only the display differs.
 */
internal class MentionDisplay private constructor(
    val text: String,
    /** Half-open display ranges of the rendered name pills. */
    val pills: List<Pair<Int, Int>>,
    private val replacements: List<Replacement>,
) {
    class Replacement(
        val sourceStart: Int,
        val sourceEnd: Int,
        val displayStart: Int,
        val displayEnd: Int,
    )

    /** Map a source offset onto the displayed string. */
    fun map(offset: Int): Int {
        if (replacements.isEmpty()) return offset
        var delta = 0
        for (r in replacements) {
            if (offset <= r.sourceStart) break
            if (offset < r.sourceEnd) {
                // Inside a replaced token: snap to the nearer pill edge.
                return if (offset - r.sourceStart <= r.sourceEnd - offset) {
                    r.displayStart
                } else {
                    r.displayEnd
                }
            }
            delta += (r.displayEnd - r.displayStart) - (r.sourceEnd - r.sourceStart)
        }
        return offset + delta
    }

    companion object {
        private class Candidate(
            val start: Int,
            val end: Int,
            val replacement: String,
            /** Mention pills get their own display style range; ref chips are
             *  styled through their link annotation instead. */
            val isMention: Boolean,
        )

        /** Identity mapping when nothing is replaced. */
        fun build(
            text: String,
            pills: List<Pair<Mentions.Match, MentionMember>>,
            refPills: List<Pair<IssueRefs.Match, IssueRefTarget>> = emptyList(),
        ): MentionDisplay {
            val candidates = ArrayList<Candidate>(pills.size + refPills.size)
            for ((match, member) in pills) {
                candidates.add(Candidate(match.start, match.end, "@${member.name}", true))
            }
            // EXP-307: the chip shows the whole title next to the short code
            // (blank titles keep the bare token).
            for ((match, target) in refPills) {
                val title = chipTitle(target.title)
                if (title.isEmpty()) continue
                if (match.start < 0 || match.end > text.length || match.end <= match.start) continue
                candidates.add(
                    Candidate(
                        match.start, match.end,
                        "${text.substring(match.start, match.end)} $title",
                        isMention = false,
                    ),
                )
            }
            if (candidates.isEmpty()) return MentionDisplay(text, emptyList(), emptyList())
            candidates.sortBy { it.start }
            val out = StringBuilder(text.length)
            val ranges = ArrayList<Pair<Int, Int>>(pills.size)
            val replacements = ArrayList<Replacement>(candidates.size)
            var last = 0
            for (candidate in candidates) {
                if (candidate.start < last) continue // overlapping token: keep the first
                out.append(text, last, candidate.start)
                val displayStart = out.length
                out.append(candidate.replacement)
                if (candidate.isMention) ranges.add(displayStart to out.length)
                replacements.add(
                    Replacement(candidate.start, candidate.end, displayStart, out.length),
                )
                last = candidate.end
            }
            out.append(text, last, text.length)
            return MentionDisplay(out.toString(), ranges, replacements)
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
