package com.exponential.app.ui.markdown

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import com.exponential.app.ui.markdown.model.InlineMark

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
 * offsets inside the token stay linear and the caret can sit between `#EXP`
 * and `-238` exactly as before. `@email` mentions are deliberately NOT
 * substituted here (web's `mention-pill-extension.ts`: "hiding characters
 * under an active caret makes editing hazardous"); the read renderer
 * [MentionDisplay] still swaps in member names.
 */
internal class IssueChipTransform private constructor(
    val source: String,
    val display: String,
    /** Ascending, non-overlapping, at least one entry unless this is the identity. */
    val chips: List<Chip>,
) {

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

    val isIdentity: Boolean get() = chips.isEmpty()

    /**
     * Source → display. Strictly `<` at a chip's `sourceEnd`, so an offset AT
     * the token end maps BEFORE that chip's title: typing there extends the
     * token (the caret is visually right after `238`), and a selection ending
     * at the token end highlights the token without the title.
     */
    fun originalToTransformed(offset: Int): Int {
        val p = offset.coerceIn(0, source.length)
        var delta = 0
        for (chip in chips) {
            if (chip.sourceEnd < p) delta += chip.injectedLength else break
        }
        return (p + delta).coerceIn(0, display.length)
    }

    /**
     * Display → source. Anything landing inside an injected title snaps to the
     * token end, so tapping the title puts the caret right after the
     * identifier instead of somewhere that does not exist in the source.
     */
    fun transformedToOriginal(offset: Int): Int {
        val q = offset.coerceIn(0, display.length)
        var delta = 0
        for (chip in chips) {
            if (q <= chip.displayTokenEnd) return (q - delta).coerceIn(0, source.length)
            if (q <= chip.displayEnd) return chip.sourceEnd
            delta += chip.injectedLength
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
        override fun originalToTransformed(offset: Int): Int =
            this@IssueChipTransform.originalToTransformed(offset)

        override fun transformedToOriginal(offset: Int): Int =
            this@IssueChipTransform.transformedToOriginal(offset)
    }

    companion object {
        /**
         * Build the chip transform for one editable line. Returns the identity
         * instance when there is nothing to chip — the common case, and the
         * reason the `contains('#')` early-out matters on the per-keystroke
         * path.
         *
         * MUST be called with the string the transformation is filtering; a
         * transform built from a captured, stale `row.text`/`value.text` is
         * how out-of-range mappings happen.
         */
        fun build(
            source: String,
            marks: List<InlineMark>,
            issueRefs: IssueRefHandler?,
            enabled: Boolean,
        ): IssueChipTransform {
            if (!enabled || issueRefs == null || !source.contains('#')) return identity(source)
            // Same candidate rule as the read renderer: resolved tokens only,
            // never inside inline code or a markdown link.
            val pills = resolvedRefPills(source, marks, issueRefs)
            if (pills.isEmpty()) return identity(source)

            val out = StringBuilder(source.length + pills.size * 16)
            val chips = ArrayList<Chip>(pills.size)
            var last = 0
            for ((match, target) in pills.sortedBy { it.first.start }) {
                if (match.start < last) continue // overlapping token: keep the first
                if (match.start < 0 || match.end > source.length || match.end <= match.start) continue
                val title = chipTitle(target.title)
                if (title.isEmpty()) continue // blank title keeps the bare token
                out.append(source, last, match.start)
                val displayStart = out.length
                out.append(source, match.start, match.end)
                val displayTokenEnd = out.length
                out.append(' ').append(title)
                chips.add(
                    Chip(match.start, match.end, displayStart, displayTokenEnd, out.length, target),
                )
                last = match.end
            }
            if (chips.isEmpty()) return identity(source)
            out.append(source, last, source.length)
            return IssueChipTransform(source, out.toString(), chips)
        }

        private fun identity(source: String) = IssueChipTransform(source, source, emptyList())
    }
}

/**
 * The editor's inline styling: the existing bold/italic/strike/code/link marks
 * plus resolved `#IDENTIFIER` chips (EXP-322). Falls back to the pre-chip
 * behaviour byte for byte when nothing chips.
 */
internal class ChipVisualTransformation(
    private val marks: List<InlineMark>,
    private val issueRefs: IssueRefHandler?,
    private val chipsEnabled: Boolean,
) : VisualTransformation {

    override fun filter(text: AnnotatedString): TransformedText {
        // Built from filter's OWN argument — never from captured state.
        val transform = IssueChipTransform.build(text.text, marks, issueRefs, chipsEnabled)
        if (transform.isIdentity) {
            return TransformedText(InlineMarks.annotate(text.text, marks), OffsetMapping.Identity)
        }
        // Reuse the shared mark styling by lifting the marks into display
        // coordinates first, so the editor and the read renderer can never
        // drift on how a bold/link/code span looks.
        val remapped = marks.mapNotNull { m ->
            val start = transform.originalToTransformed(m.start)
            val end = transform.originalToTransformed(m.end)
            if (end > start) m.copy(start = start, end = end) else null
        }
        val base = InlineMarks.annotate(transform.display, remapped)
        val annotated = buildAnnotatedString {
            append(base)
            for (chip in transform.chips) {
                // The same span treatment MarkdownView's read-mode pills use
                // (EXP-423): title in the normal text color, identifier muted +
                // monospace, and — when the issue's status resolved — a
                // transparent `#` for the glyph BlockTextField paints over it.
                // The background and border are painted, not spanned. No
                // `addLink` here: a LinkAnnotation inside a BasicTextField
                // fights the caret — tap-to-open is a pointerInput on the
                // decoration box instead.
                addStyle(
                    SpanStyle(color = MdStyle.Text),
                    chip.displayStart,
                    chip.displayEnd,
                )
                addStyle(
                    SpanStyle(fontFamily = FontFamily.Monospace, color = MdStyle.ChipToken),
                    chip.displayStart,
                    chip.displayTokenEnd,
                )
                if (chip.target.resolvedStatus != null) {
                    addStyle(
                        SpanStyle(color = Color.Transparent),
                        chip.displayStart,
                        (chip.displayStart + 1).coerceAtMost(chip.displayTokenEnd),
                    )
                }
            }
        }
        return TransformedText(annotated, transform.offsetMapping)
    }

    override fun equals(other: Any?): Boolean =
        other is ChipVisualTransformation &&
            other.marks == marks &&
            other.issueRefs === issueRefs &&
            other.chipsEnabled == chipsEnabled

    override fun hashCode(): Int {
        var result = marks.hashCode()
        result = 31 * result + (issueRefs?.hashCode() ?: 0)
        result = 31 * result + chipsEnabled.hashCode()
        return result
    }
}
