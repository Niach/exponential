package com.exponential.app.ui.markdown

import androidx.compose.ui.graphics.Color
import com.exponential.app.ui.theme.ExpMono
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.ParagraphStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextIndent
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.sp
import com.exponential.app.ui.markdown.model.BlockKind
import com.exponential.app.ui.markdown.model.InlineMark
import com.exponential.app.ui.markdown.model.ParagraphAttrs

/**
 * The editor-side `#IDENTIFIER` chip (EXP-322): a resolved token renders as
 * `#EXP-238 <issue title>` WHILE TYPING, exactly like the web editor, without
 * the title ever entering the stored markdown.
 *
 * Web does this with a CSS `::after` on a ProseMirror decoration
 * (`apps/web/src/lib/issue-ref-extension.ts` + `.issue-ref-pill::after`) — the
 * document text stays the bare token. The Compose analog is a
 * [VisualTransformation] with a non-identity [OffsetMapping], which is what
 * this class provides: [display] is the source with each chip's title spliced
 * in, and the two mapping functions translate between the two coordinate
 * spaces. `BasicTextField` hands `onValueChange` ORIGINAL offsets (it maps IME
 * and gesture offsets back through `transformedToOriginal` first), so the
 * model, the markdown, and the autocomplete probe all keep seeing the bare
 * token.
 *
 * The injection is a pure SUFFIX at the token's end — never a replacement — so
 * offsets inside the token stay linear. The chip is still ONE thing to the
 * caret (EXP-655, the IDE's EXP-547 rules): [caretToDisplay] renders a caret
 * at the token end AFTER the title, [transformedToOriginal] snaps a tap
 * inside the pill to its nearer edge, and [BlockTextField] skips a caret
 * moved into the token. `@email` mentions are deliberately NOT
 * substituted here (web's `mention-pill-extension.ts`: "hiding characters
 * under an active caret makes editing hazardous"); the read renderer
 * [MentionDisplay] still swaps in member names.
 */
internal class IssueChipTransform private constructor(
    val source: String,
    val display: String,
    /** Ascending, non-overlapping, at least one entry unless this is the identity. */
    val chips: List<Chip>,
    /** Ascending paragraph gaps (EXP-1676), empty unless built with `paragraphGaps`. */
    val gaps: List<Gap> = emptyList(),
) {

    /**
     * EXP-1676: web's 0.5rem paragraph rhythm (`.tiptap-content p` / task
     * `li` margins) in a BasicTextField, which has no paragraph spacing: the
     * display injects a [GAP_CHAR] after a source '\n' and gives it its own
     * [MdStyle.blockSpacing]-tall paragraph ([ChipVisualTransformation]).
     * The gap is never a caret position: both mapping directions skip it.
     * When the '\n' ends the source (an empty last line), a SECOND char rides
     * along so that empty line owns a display character (its own range).
     */
    class Gap(
        /** Source offset of the '\n' the gap follows. */
        val newline: Int,
        /** Display offset of that '\n'. */
        val displayNewline: Int,
        /** Injected chars: 1, or 2 for the empty last line's own char. */
        val length: Int,
    ) {
        val sourceEnd: Int get() = newline + 1
        /** Display offset of the gap paragraph's one char. */
        val displayGap: Int get() = displayNewline + 1
        val displayEnd: Int get() = displayNewline + 1 + length
    }

    /**
     * One rendered chip. Source coordinates cover the bare token; display
     * coordinates cover the token (`displayStart`..`displayTokenEnd`) plus the
     * injected ` title` (..`displayEnd`).
     */
    class Chip(
        val sourceStart: Int,
        val sourceEnd: Int,
        val displayStart: Int,
        val displayTokenEnd: Int,
        val displayEnd: Int,
        /** The resolved issue — its status drives the painted glyph (EXP-423). */
        val target: IssueRefTarget,
    ) {
        val injectedLength: Int get() = displayEnd - displayTokenEnd
    }

    val isIdentity: Boolean get() = chips.isEmpty() && gaps.isEmpty()

    /** Display offset of the first character of the source line starting at [sourceLineStart]. */
    fun lineStartToDisplay(sourceLineStart: Int): Int {
        val d = caretToDisplay(sourceLineStart)
        val padded = gaps.any { it.length == 2 && it.sourceEnd == sourceLineStart }
        return if (padded) d - 1 else d
    }

    /**
     * Each source line's display bounds: its first character and its visual
     * end (the display offset of its '\n', or the display end).
     */
    fun lineBounds(): List<Pair<Int, Int>> {
        val lines = if (source.isEmpty()) listOf("") else source.split("\n")
        val out = ArrayList<Pair<Int, Int>>(lines.size)
        var start = 0
        for ((i, line) in lines.withIndex()) {
            val end = start + line.length
            val dStart = lineStartToDisplay(start)
            val dEnd = if (i == lines.lastIndex) display.length else caretToDisplay(end).coerceAtLeast(dStart)
            out.add(dStart to dEnd)
            start = end + 1
        }
        return out
    }

    /**
     * Source → display, RANGE semantics: strictly `<` at a chip's `sourceEnd`,
     * so a range ending AT the token end stops BEFORE that chip's title — a
     * bold mark or a line style over the token never bleeds into the
     * injected title. Caret positions go through [caretToDisplay].
     */
    fun originalToTransformed(offset: Int): Int = toDisplay(offset, inclusive = false)

    /**
     * Source → display, CARET semantics (EXP-655): a caret AT the token end
     * renders AFTER the chip's title, i.e. right of the whole pill, never
     * between the identifier and its title (the IDE's EXP-547 rule). Typing
     * there still extends the token in the source.
     */
    fun caretToDisplay(offset: Int): Int = toDisplay(offset, inclusive = true)

    private fun toDisplay(offset: Int, inclusive: Boolean): Int {
        val p = offset.coerceIn(0, source.length)
        var delta = 0
        for (chip in chips) {
            val counts = if (inclusive) chip.sourceEnd <= p else chip.sourceEnd < p
            if (counts) delta += chip.injectedLength else break
        }
        // A gap belongs to the NEXT line under both semantics: a range or a
        // caret at that line's start lands after it.
        for (gap in gaps) {
            if (gap.sourceEnd <= p) delta += gap.length else break
        }
        return (p + delta).coerceIn(0, display.length)
    }

    /**
     * Display → source. Anything landing INSIDE a chip snaps to its nearer
     * edge (EXP-655): a tap on the first half of the identifier puts the caret
     * before the `#`, on its second half or anywhere in the injected title
     * right after the identifier — never somewhere inside the pill, and never
     * at a display position that does not exist in the source.
     */
    fun transformedToOriginal(offset: Int): Int {
        val q = offset.coerceIn(0, display.length)
        var delta = 0
        var gi = 0
        for (chip in chips) {
            while (gi < gaps.size && gaps[gi].displayNewline < chip.displayStart) {
                val gap = gaps[gi]
                if (q <= gap.displayGap) return (q - delta).coerceIn(0, source.length)
                if (q <= gap.displayEnd) return gap.sourceEnd
                delta += gap.length
                gi++
            }
            if (q <= chip.displayStart) return (q - delta).coerceIn(0, source.length)
            if (q <= chip.displayEnd) {
                val nearStart = q - chip.displayStart <= chip.displayTokenEnd - q
                return if (nearStart) chip.sourceStart else chip.sourceEnd
            }
            delta += chip.injectedLength
        }
        while (gi < gaps.size) {
            val gap = gaps[gi]
            if (q <= gap.displayGap) return (q - delta).coerceIn(0, source.length)
            if (q <= gap.displayEnd) return gap.sourceEnd
            delta += gap.length
            gi++
        }
        return (q - delta).coerceIn(0, source.length)
    }

    /**
     * Compose wraps this in `ValidatingOffsetMapping`, which `check(result in
     * 0..length)` in BOTH directions and throws. Both functions above coerce,
     * so the check can never fire even if this transform is one frame stale
     * relative to the text Compose hands it.
     */
    val offsetMapping: OffsetMapping = object : OffsetMapping {
        // The field maps CARETS (and selection ends) through this one.
        override fun originalToTransformed(offset: Int): Int =
            this@IssueChipTransform.caretToDisplay(offset)

        override fun transformedToOriginal(offset: Int): Int =
            this@IssueChipTransform.transformedToOriginal(offset)
    }

    companion object {
        /**
         * Build the chip transform for one editable text run (multi-line since
         * EXP-534 — the injection stays a pure per-token suffix, so newlines
         * change nothing). Returns the identity instance when there is nothing
         * to chip — the common case, and the reason the `contains('#')`
         * early-out matters on the per-keystroke path.
         *
         * [paragraphs] is the run's per-line attrs: tokens on a code-block
         * line stay plain, the run-level analog of the old per-row
         * `chipsEnabled` flag (read-mode parity — code renders literally).
         *
         * MUST be called with the string the transformation is filtering; a
         * transform built from a captured, stale `row.text`/`value.text` is
         * how out-of-range mappings happen.
         */
        fun build(
            source: String,
            marks: List<InlineMark>,
            issueRefs: IssueRefHandler?,
            paragraphs: List<ParagraphAttrs>,
            /** EXP-1676: inject the paragraph [Gap]s (the editor field). */
            paragraphGaps: Boolean = false,
        ): IssueChipTransform {
            val gapAt = if (paragraphGaps) gapNewlines(source, paragraphs) else emptyList()
            val pills = if (issueRefs == null || !source.contains('#')) {
                emptyList()
            } else {
                // Same candidate rule as the read renderer: resolved tokens only,
                // never inside inline code, a markdown link, or a code-block line.
                resolvedRefPills(source, marks, issueRefs).filterNot { (match, _) ->
                    paragraphs.getOrNull(ParaRemap.paraIndexAt(source, match.start))
                        ?.kind == BlockKind.CodeBlock
                }.sortedBy { it.first.start }
            }
            if (pills.isEmpty() && gapAt.isEmpty()) return identity(source)

            val out = StringBuilder(source.length + pills.size * 16 + gapAt.size * 2)
            val chips = ArrayList<Chip>(pills.size)
            val gaps = ArrayList<Gap>(gapAt.size)
            var last = 0
            var pi = 0
            var gi = 0
            while (pi < pills.size || gi < gapAt.size) {
                val newline = gapAt.getOrNull(gi)
                val pill = pills.getOrNull(pi)
                if (pill != null && (newline == null || pill.first.start < newline)) {
                    pi++
                    val (match, target) = pill
                    if (match.start < last) continue // overlapping token: keep the first
                    if (match.start < 0 || match.end > source.length || match.end <= match.start) continue
                    val title = inlineChipTitle(target.title)
                    if (title.isEmpty()) continue // blank title keeps the bare token
                    out.append(source, last, match.start)
                    val displayStart = out.length
                    out.append(source, match.start, match.end)
                    val displayTokenEnd = out.length
                    out.append(CHIP_NBSP).append(title)
                    chips.add(
                        Chip(match.start, match.end, displayStart, displayTokenEnd, out.length, target),
                    )
                    last = match.end
                } else {
                    gi++
                    val n = newline ?: break
                    if (n < last) continue
                    out.append(source, last, n + 1)
                    val length = if (n + 1 == source.length) 2 else 1
                    gaps.add(Gap(n, out.length - 1, length))
                    repeat(length) { out.append(GAP_CHAR) }
                    last = n + 1
                }
            }
            if (chips.isEmpty() && gaps.isEmpty()) return identity(source)
            out.append(source, last, source.length)
            return IssueChipTransform(source, out.toString(), chips, gaps)
        }

        /** The injected gap character: zero-width, so it never paints. */
        const val GAP_CHAR = '\u200B'

        /**
         * The source '\n's a paragraph gap follows: every break except one
         * INSIDE a code fence (web's `pre` is one block, its lines pack).
         */
        internal fun gapNewlines(source: String, paragraphs: List<ParagraphAttrs>): List<Int> {
            if (!source.contains('\n')) return emptyList()
            val out = ArrayList<Int>()
            var line = 0
            for ((i, c) in source.withIndex()) {
                if (c != '\n') continue
                val above = paragraphs.getOrNull(line)?.kind
                val below = paragraphs.getOrNull(line + 1)?.kind
                if (!(above == BlockKind.CodeBlock && below == BlockKind.CodeBlock)) out.add(i)
                line++
            }
            return out
        }

        private fun identity(source: String) = IssueChipTransform(source, source, emptyList())
    }
}

/**
 * The editor's whole visual styling for one multi-line text run (EXP-534):
 * per-PARAGRAPH styles (heading size/weight, list/quote/code indents via
 * [ParagraphStyle], quote/code/thematic colors), the existing
 * bold/italic/strike/code/link marks, and resolved `#IDENTIFIER` chips
 * (EXP-322) — layered in that order so marks override line styles and chips
 * override both, exactly like the read renderer.
 */
internal class ChipVisualTransformation(
    private val marks: List<InlineMark>,
    private val issueRefs: IssueRefHandler?,
    private val paragraphs: List<ParagraphAttrs>,
    /** For dp→sp indent conversion — sp shrinks under font scaling, dp doesn't. */
    private val fontScale: Float,
    /** EXP-1676: web's paragraph rhythm ([IssueChipTransform.Gap]). */
    private val paragraphGaps: Boolean = false,
) : VisualTransformation {

    override fun filter(text: AnnotatedString): TransformedText {
        // Built from filter's OWN argument — never from captured state.
        val source = text.text
        val transform = IssueChipTransform.build(source, marks, issueRefs, paragraphs, paragraphGaps)
        // Reuse the shared mark styling by lifting the marks into display
        // coordinates first, so the editor and the read renderer can never
        // drift on how a bold/link/code span looks.
        val remapped = if (transform.isIdentity) marks else marks.mapNotNull { m ->
            val start = transform.originalToTransformed(m.start)
            val end = transform.originalToTransformed(m.end)
            if (end > start) m.copy(start = start, end = end) else null
        }
        val annotated = buildAnnotatedString {
            // Newlines render as zero-width spaces: every line carries its own
            // ParagraphStyle range below, so the paragraph BREAKS come from the
            // ranges — a real '\n' inside a range would add a trailing empty
            // line to its paragraph (StaticLayout renders one after a trailing
            // newline), doubling the spacing of every styled line. The 1:1
            // substitution keeps display offsets identical, keeps end-of-line
            // and start-of-next-line carets at DISTINCT display offsets (the
            // zero-width char still occupies one), and the copy/IME path never
            // sees it — those work on the source TextFieldValue.
            //
            // The ONE exception is a FINAL trailing '\n' (EXP-567): the empty
            // last line owns no display character, so no ParagraphStyle range
            // can stand in for it: substituted away, the layout simply lacked
            // that line, the caret stayed visually on the previous line after
            // Enter, and the line only popped in once a character landed on
            // it. Keeping the last '\n' real makes StaticLayout render exactly
            // that one trailing empty line (the behavior the substitution
            // avoids everywhere else, wanted here), still 1:1 on offsets.
            val display = transform.display
            if (display.endsWith('\n')) {
                append(display.substring(0, display.length - 1).replace('\n', '\u200B'))
                append('\n')
            } else {
                append(display.replace('\n', '\u200B'))
            }
            addLineStyles(this, source, transform)
            InlineMarks.addStyles(this, transform.display.length, remapped)
            addChipStyles(this, transform)
        }
        val mapping = if (transform.isIdentity) OffsetMapping.Identity else transform.offsetMapping
        return TransformedText(annotated, mapping)
    }

    /**
     * Per-paragraph span + paragraph styles over each line's display range.
     * EVERY line gets a ParagraphStyle range (plain lines an empty one): with
     * newlines substituted away (see [filter]) the ranges are what break the
     * text into paragraphs at all. Ranges INCLUDE the substituted newline so
     * they tile the display exactly. The one line WITHOUT a range is a
     * trailing EMPTY line — it has no display character to range over, so it
     * rides the previous line's range as that paragraph's real final '\n'
     * instead (see [filter], EXP-567). The paragraphs list can lag the text
     * by a frame; every index is guarded and a missing entry styles as plain.
     */
    private fun addLineStyles(
        builder: AnnotatedString.Builder,
        source: String,
        transform: IssueChipTransform,
    ) {
        val display = transform.display
        // The display position of each line's '\n' (chips injected at the
        // very end of the line sit BEFORE it: caret semantics at the line end).
        for ((i, bounds) in transform.lineBounds().withIndex()) {
            val (dStart, dLineEnd) = bounds
            val a = paragraphs.getOrNull(i) ?: ParagraphAttrs.PLAIN
            val span: SpanStyle? = when (a.kind) {
                BlockKind.Heading -> MdStyle.heading(a.headingLevel).toSpanStyle()
                BlockKind.CodeBlock -> SpanStyle(
                    fontFamily = ExpMono,
                    fontSize = MdStyle.mono.fontSize,
                )
                BlockKind.Blockquote -> SpanStyle(color = MdStyle.Blockquote)
                BlockKind.ThematicBreak -> SpanStyle(color = MdStyle.Dim)
                else -> null
            }
            val paragraph: ParagraphStyle = when (a.kind) {
                BlockKind.Heading -> ParagraphStyle(lineHeight = MdStyle.heading(a.headingLevel).lineHeight)
                BlockKind.ListItem -> ParagraphStyle(textIndent = indentOf(listIndentDp(a.listDepth)))
                BlockKind.Blockquote ->
                    ParagraphStyle(textIndent = indentOf(MdStyle.quoteBarWidth + MdStyle.quoteIndent))
                BlockKind.CodeBlock -> ParagraphStyle(textIndent = indentOf(MdStyle.codeInsetX))
                else -> ParagraphStyle()
            }
            if (span != null && dLineEnd > dStart) builder.addStyle(span, dStart, dLineEnd)
            val parEnd = (dLineEnd + 1).coerceAtMost(display.length)
            if (parEnd > dStart) builder.addStyle(paragraph, dStart, parEnd)
        }
        // Each gap char is its own paragraph, exactly [MdStyle.blockSpacing] tall.
        if (transform.gaps.isEmpty()) return
        val gapSp = (MdStyle.blockSpacing.value / fontScale).sp
        val gapParagraph = ParagraphStyle(
            lineHeight = gapSp,
            lineHeightStyle = LineHeightStyle(
                alignment = LineHeightStyle.Alignment.Center,
                trim = LineHeightStyle.Trim.None,
            ),
        )
        for (gap in transform.gaps) {
            val at = gap.displayGap
            if (at + 1 > display.length) continue
            builder.addStyle(SpanStyle(fontSize = gapSp), at, at + 1)
            builder.addStyle(gapParagraph, at, at + 1)
        }
    }

    private fun listIndentDp(depth: Int): Dp =
        MdStyle.listIndentBase + MdStyle.listIndentPerDepth * depth.coerceAtLeast(0) +
            MdStyle.listGlyphWidth

    private fun indentOf(dp: Dp): TextIndent {
        val sp = (dp.value / fontScale).sp
        return TextIndent(firstLine = sp, restLine = sp)
    }

    private fun addChipStyles(builder: AnnotatedString.Builder, transform: IssueChipTransform) {
        for (chip in transform.chips) {
            // The same span treatment MarkdownView's read-mode pills use
            // (EXP-423): title in the normal text color, identifier muted +
            // monospace, and — when the issue's status resolved — a
            // transparent `#` for the glyph BlockTextField paints over it.
            // The background and border are painted, not spanned. No
            // `addLink` here: a LinkAnnotation inside a BasicTextField
            // fights the caret — tap-to-open is a pointerInput on the
            // decoration box instead.
            builder.addStyle(
                SpanStyle(color = MdStyle.Text),
                chip.displayStart,
                chip.displayEnd,
            )
            builder.addStyle(
                SpanStyle(fontFamily = ExpMono, color = MdStyle.ChipToken),
                chip.displayStart,
                chip.displayTokenEnd,
            )
            if (chip.target.resolvedStatus != null) {
                // The extra letter spacing widens the hidden `#` cell so the
                // glyph painted at its left edge clears the identifier
                // (EXP-655) — an advance change, so no character moves.
                builder.addStyle(
                    SpanStyle(color = Color.Transparent, letterSpacing = MdStyle.chipHashLetterSpacing),
                    chip.displayStart,
                    (chip.displayStart + 1).coerceAtMost(chip.displayTokenEnd),
                )
            }
        }
    }

    override fun equals(other: Any?): Boolean =
        other is ChipVisualTransformation &&
            other.marks == marks &&
            other.issueRefs === issueRefs &&
            other.paragraphs == paragraphs &&
            other.fontScale == fontScale &&
            other.paragraphGaps == paragraphGaps

    override fun hashCode(): Int {
        var result = marks.hashCode()
        result = 31 * result + (issueRefs?.hashCode() ?: 0)
        result = 31 * result + paragraphs.hashCode()
        result = 31 * result + fontScale.hashCode()
        result = 31 * result + paragraphGaps.hashCode()
        return result
    }
}
