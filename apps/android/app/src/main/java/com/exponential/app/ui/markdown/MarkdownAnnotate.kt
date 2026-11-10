package com.exponential.app.ui.markdown

import androidx.compose.runtime.compositionLocalOf
import com.exponential.app.ui.theme.ExpMono
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import com.exponential.app.domain.ProseLinks
import com.exponential.app.ui.markdown.model.InlineKind
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.theme.resolvedStatusColor

// The inline annotation the EDITOR's chip transform and the transcript's
// marker prose share (#VAPP-258: the read-only renderer moved onto genUI's
// Markdown, [MarkdownView]); the body / code / autolink locals below still
// pick a read surface's density and its link rule.

/**
 * Bare `http(s)://` URLs render as tappable links (EXP-440) — for RENDER-ONLY
 * surfaces such as the agent steering feed, whose text nobody edits back into
 * stored markdown. Default OFF: issue descriptions and comments round-trip
 * through the editor, and autolinking there would diverge the stored bytes
 * from web (see [MarkdownParser]'s note on the autolink extension).
 */
val LocalMarkdownAutolink = compositionLocalOf { false }

/**
 * The inline-`code` chrome this subtree renders with (EXP-698). Default is
 * [MdStyle.Default] — the flat white wash every DOCUMENT surface (issue
 * descriptions, comments, the editor) has always drawn. The agent steering
 * feed provides [MdStyle.Chat] so a `--flag` or a `Foo.kt` in a wall of
 * narration reads as code, exactly like web and the desktop tint theirs.
 */
val LocalInlineCodeStyle = compositionLocalOf { MdStyle.Default }

/**
 * The BODY style this subtree renders prose at (EXP-787). Default is
 * [MdStyle.body] — web's `.tiptap-content` document measure (14 sp on a
 * 1.625 line) issue descriptions and comments use. The agent transcript provides the shared
 * `DesignTokens.Transcript` step (14 sp on a 22 sp line, ×4 parity), which is
 * a reading measure for a wall of narration rather than a document body.
 * Headings keep their own scale; code derives from whatever is in force.
 */
val LocalMarkdownBodyStyle = compositionLocalOf { MdStyle.body }

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
    inlineCode: MdStyle.InlineCodeStyle = MdStyle.Default,
    bare: Boolean = false,
    appLinks: AppLinkHandler? = null,
): AnnotatedString =
    annotateLine(text, marks, issueRefs, mentions, autolink, inlineCode, bare, appLinks).text

internal fun annotateLine(
    text: String,
    marks: List<InlineMark>,
    issueRefs: IssueRefHandler?,
    mentions: MentionResolver? = null,
    autolink: Boolean = false,
    /** EXP-698: chat feeds tint inline `code`; documents keep the flat wash. */
    inlineCode: MdStyle.InlineCodeStyle = MdStyle.Default,
    /** EXP-760: also chip BARE `EXP-758` tokens — steering feeds only. */
    bare: Boolean = false,
    /** EXP-1188: agent surfaces open links on this instance in the app. */
    appLinks: AppLinkHandler? = null,
): AnnotatedLine {
    if (text.isEmpty()) return AnnotatedLine(AnnotatedString(""))
    val refPills =
        if (issueRefs != null) resolvedRefPills(text, marks, issueRefs, bare) else emptyList()
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
    val display = MentionDisplay.build(text, mentionPills, refPills, proseChipMarks(text, marks))
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
                    SpanStyle(
                        fontFamily = ExpMono,
                        color = inlineCode.inlineCodeColor ?: Color.Unspecified,
                        background = inlineCode.inlineCodeBg,
                    ),
                    start, end,
                )
                InlineKind.Link -> {
                    val href = m.href ?: continue
                    val chipKind = ProseLinks.chipKind(href)
                    // EXP-1188: an ignored link (a placeholder host, another
                    // scheme) stays plain text — nothing to open.
                    val link = appLinkAnnotation(
                        href,
                        appLinks,
                        // A prose chip wears the chip's paint, not the link's.
                        if (chipKind != null) TextLinkStyles() else TextLinkStyles(style = SpanStyle(color = MdStyle.Link)),
                    ) ?: continue
                    addLinkGuarded(link, rawStart, rawEnd)
                    if (chipKind != null) {
                        // #EXP-1674 (prose-links.json): THE entity chip: the
                        // kind's glyph over the spliced, hidden cell, the name.
                        addStyle(SpanStyle(color = MdStyle.Text), start, end)
                        addStyle(
                            SpanStyle(color = Color.Transparent, letterSpacing = MdStyle.chipHashLetterSpacing),
                            start,
                            start + 1,
                        )
                        chipSpecs.add(
                            IssueRefChipSpec(
                                start = start,
                                end = end,
                                tokenStart = start,
                                iconName = proseChipGlyph(chipKind),
                                color = MdStyle.ChipToken,
                            ),
                        )
                    }
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
            // EXP-885: a bare token owns no `#`, so the display splices a
            // GLYPH CELL in front of it (see [CHIP_GLYPH_CELL]) — the
            // identifier then starts that one character in.
            val cell = if (display.hasGlyphCell(match.start)) CHIP_GLYPH_CELL.length else 0
            val tokenEnd = (chipStart + cell + (match.end - match.start)).coerceAtMost(chipEnd)
            // Title in the normal text color, identifier muted + monospace
            // (the glyph cell included: it wears the hidden `#`'s treatment).
            addStyle(SpanStyle(color = MdStyle.Text), chipStart, chipEnd)
            addStyle(
                SpanStyle(fontFamily = ExpMono, color = MdStyle.ChipToken),
                chipStart,
                tokenEnd,
            )
            // EXP-885: every resolved chip paints its status glyph, bare or
            // not — the cell it paints in is the `#` for a `#MET-1` token and
            // the spliced placeholder for a bare one. The ONE case left
            // unpainted is a bare token whose splice was dropped (an
            // overlapping candidate, below): there the chip starts on the
            // identifier's first LETTER, which nothing may hide.
            val status = if (match.bare && cell == 0) null else target.resolvedStatus
            if (status != null && chipStart < chipEnd) {
                // The status glyph is painted over that cell; hiding it costs
                // one transparent character span and zero offset-map changes.
                // The letter spacing widens it into the icon↔identifier gap
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
            val link = appLinkAnnotation(
                range.url,
                appLinks,
                TextLinkStyles(style = SpanStyle(color = MdStyle.Link)),
            ) ?: continue
            addLinkGuarded(link, range.start, range.end)
        }
        // Resolved `@email` mentions render as the member's name pill
        // (REV2-42) — display-only, not tappable (there is nothing to open),
        // so they can never collide with a link annotation.
        //
        // Session e4a04d22: the web `.mention-pill` look — `--accent-foreground`
        // at 13 medium inside a painted round capsule ([drawIssueRefChips],
        // `pill = true`). The splice wraps the name in two [MENTION_PAD]
        // cells; letter-spaced to the web's `0.4rem`, they ARE the padding.
        for (range in display.pills) {
            addStyle(
                SpanStyle(
                    color = MdStyle.MentionText,
                    fontSize = MdStyle.mentionFontSize,
                    fontWeight = FontWeight.Medium,
                ),
                range.first,
                range.second,
            )
            if (range.second - range.first >= 2) {
                val pad = SpanStyle(letterSpacing = MdStyle.mentionPadLetterSpacing)
                addStyle(pad, range.first, range.first + 1)
                addStyle(pad, range.second - 1, range.second)
            }
            chipSpecs.add(
                IssueRefChipSpec(start = range.first, end = range.second, tokenStart = range.first, pill = true),
            )
        }
    }
    return AnnotatedLine(annotated, chipSpecs)
}

/** Keep chips readable (web parity: `MAX_CHIP_TITLE_LENGTH` = 60). */
internal const val MAX_CHIP_TITLE_CHARS = 60

/** What of a title fits web's 18rem inline chip beside a short identifier. */
internal const val MAX_INLINE_CHIP_TITLE_CHARS = 34

internal const val CHIP_NBSP = '\u00A0'

/**
 * EXP-885: the cell a BARE `EXP-758` chip paints its status glyph in. A
 * `#IDENTIFIER` token brings its own — the `#` goes transparent and the art
 * lands on it — but a bare token starts on a LETTER, so before EXP-885 the
 * glyph was simply dropped and the steering feed's chips read status-less next
 * to the identical chips everywhere else. The display therefore splices this
 * one NON-BREAKING space in front of the token (the stored markdown is
 * untouched, like every other [MentionDisplay] splice) and [annotateLine]
 * gives it the hidden `#`'s exact treatment: transparent, letter-spaced to the
 * icon's width. Non-breaking so a wrap can never put the glyph on the line
 * above its identifier.
 */
internal const val CHIP_GLYPH_CELL = "\u00A0"

/** The registry glyph of a prose chip's kind: entity-chip.json `entityRefIcon`,
 *  locked against [com.exponential.app.ui.components.entityConceptIcon] by
 *  `MarkdownAnnotateTest`. */
internal fun proseChipGlyph(kind: ProseLinks.ChipKind): String = when (kind) {
    ProseLinks.ChipKind.Issue -> "file-text"
    ProseLinks.ChipKind.Board -> "folder-kanban"
    ProseLinks.ChipKind.Session -> "play"
    ProseLinks.ChipKind.Agent -> "bot"
}

/** The link marks in a line that are prose chips (prose-links.json). */
private fun proseChipMarks(text: String, marks: List<InlineMark>): List<InlineMark> =
    marks.filter { m ->
        m.kind == InlineKind.Link && m.href?.let(ProseLinks::chipKind) != null &&
            m.start >= 0 && m.end <= text.length && m.end > m.start
    }

/**
 * Session e4a04d22: the cell opening and closing a mention pill's display text.
 * Letter-spaced up to the web pill's `0.4rem` side padding, it is the room
 * the painted capsule draws its padding in (a span cannot pad). Non-breaking,
 * like the spaces inside the name: the pill never wraps (web `nowrap`).
 */
internal const val MENTION_PAD = "\u00A0"

/** The displayed text of a resolved mention: `@<name>`, unbreakable, padded. */
internal fun mentionPillText(name: String): String {
    val joined = name.trim().split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString("\u00A0")
    return "$MENTION_PAD@$joined$MENTION_PAD"
}

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
 * EXP-1676: the title an INLINE chip splices: one chip stays on one line
 * (`.issue-chip` `nowrap`) with an ellipsis at web's `max-w-[18rem]`. The chip is
 * a painted run of the paragraph's own text, which has no box to clip, so the
 * width is a budget of [MAX_INLINE_CHIP_TITLE_CHARS] and non-breaking spaces.
 */
internal fun inlineChipTitle(title: String): String {
    val trimmed = title.trim()
    val cut = if (trimmed.length <= MAX_INLINE_CHIP_TITLE_CHARS) {
        trimmed
    } else {
        trimmed.take(MAX_INLINE_CHIP_TITLE_CHARS).trimEnd().trimEnd('…') + "…"
    }
    return cut.replace(' ', CHIP_NBSP)
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
    /** EXP-760: bare `EXP-758` tokens chip too (steering feeds only). */
    bare: Boolean = false,
): List<Pair<IssueRefs.Match, IssueRefTarget>> =
    IssueRefs.findAll(text, bare).mapNotNull { match ->
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
    /**
     * SOURCE starts of the ref tokens whose splice actually prepended a
     * [CHIP_GLYPH_CELL] (EXP-885). Only the builder knows: a candidate that
     * overlapped an earlier one was dropped, and that chip has no cell.
     */
    private val glyphCells: Set<Int> = emptySet(),
) {
    /** Whether the ref token at [sourceStart] carries a spliced glyph cell. */
    fun hasGlyphCell(sourceStart: Int): Boolean = sourceStart in glyphCells

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
            /** EXP-885: this replacement leads with a [CHIP_GLYPH_CELL]. */
            val glyphCell: Boolean = false,
        )

        /** Identity mapping when nothing is replaced. */
        fun build(
            text: String,
            pills: List<Pair<Mentions.Match, MentionMember>>,
            refPills: List<Pair<IssueRefs.Match, IssueRefTarget>> = emptyList(),
            /** #EXP-1674: prose chips: the glyph cell, then the name. */
            chipMarks: List<InlineMark> = emptyList(),
        ): MentionDisplay {
            val candidates = ArrayList<Candidate>(pills.size + refPills.size + chipMarks.size)
            for (mark in chipMarks) {
                candidates.add(
                    Candidate(
                        mark.start, mark.end,
                        CHIP_GLYPH_CELL + text.substring(mark.start, mark.end),
                        isMention = false,
                        glyphCell = true,
                    ),
                )
            }
            for ((match, member) in pills) {
                candidates.add(Candidate(match.start, match.end, mentionPillText(member.name), true))
            }
            // EXP-307: the chip shows the whole title next to the short code
            // (blank titles keep the bare token). EXP-885: a bare token that
            // has a status to show also gains the glyph cell its `#`-prefixed
            // twin gets for free — one leading placeholder character, the
            // splice being the one place an offset shift is already handled.
            for ((match, target) in refPills) {
                if (match.start < 0 || match.end > text.length || match.end <= match.start) continue
                val title = inlineChipTitle(target.title)
                val glyphCell = match.bare && target.resolvedStatus != null
                if (title.isEmpty() && !glyphCell) continue
                val token = text.substring(match.start, match.end)
                candidates.add(
                    Candidate(
                        match.start, match.end,
                        buildString {
                            if (glyphCell) append(CHIP_GLYPH_CELL)
                            append(token)
                            if (title.isNotEmpty()) append(CHIP_NBSP).append(title)
                        },
                        isMention = false,
                        glyphCell = glyphCell,
                    ),
                )
            }
            if (candidates.isEmpty()) return MentionDisplay(text, emptyList(), emptyList())
            candidates.sortBy { it.start }
            val out = StringBuilder(text.length)
            val ranges = ArrayList<Pair<Int, Int>>(pills.size)
            val replacements = ArrayList<Replacement>(candidates.size)
            val glyphCells = HashSet<Int>(refPills.size)
            var last = 0
            for (candidate in candidates) {
                if (candidate.start < last) continue // overlapping token: keep the first
                out.append(text, last, candidate.start)
                val displayStart = out.length
                out.append(candidate.replacement)
                if (candidate.isMention) ranges.add(displayStart to out.length)
                if (candidate.glyphCell) glyphCells.add(candidate.start)
                replacements.add(
                    Replacement(candidate.start, candidate.end, displayStart, out.length),
                )
                last = candidate.end
            }
            out.append(text, last, text.length)
            return MentionDisplay(out.toString(), ranges, replacements, glyphCells)
        }
    }
}

