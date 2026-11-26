package at.exponential.ui.paint

import at.exponential.ui.measure.TextRun
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownBlock
import at.exponential.ui.primitives.MarkdownBlockKind
import at.exponential.ui.primitives.MarkdownInline
import at.exponential.ui.primitives.MarkdownStyles
import at.exponential.ui.primitives.MarkdownTextMeasure

/**
 * Round 4 group B (`fixtures/text-runs.json`): a Text's `runs` and its
 * `dropCap` on Compose. The spans go through the Markdown span machinery
 * (one inline paragraph: weights, tones, the inline code pill, links,
 * fills); the drop cap is the web's float: a box `lines` × lineHeight tall at
 * the inline start, the spans wrapping beside it for those lines
 * ([Markdown.dropCapFlow]), then under it at the full width. The measurer
 * and the painter both lay out through [flow], so measure = paint.
 */
object RichText {
    /** The spans of [runs]; a link only when [href] allows it (else plain text). */
    fun inlines(runs: List<TextRun>, href: (String) -> String? = { it }): List<MarkdownInline> = runs.map { r ->
        MarkdownInline(
            text = r.text,
            code = r.mark == "code",
            strike = r.mark == "strike",
            underline = r.mark == "underline",
            fill = r.mark == "highlight",
            link = r.link?.let(href),
            weight = r.fontWeight,
            tone = r.tone,
        )
    }

    fun paragraph(inlines: List<MarkdownInline>): List<MarkdownBlock> = listOf(MarkdownBlock(MarkdownBlockKind.Paragraph, inlines))

    /** One laid-out piece: its paragraph, offset, width and height. */
    class Piece(val blocks: List<MarkdownBlock>, val x: Float, val y: Float, val width: Float, val height: Float)

    class Flow(val pieces: List<Piece>, val height: Float)

    private fun piece(inlines: List<MarkdownInline>, styles: MarkdownStyles, width: Float, x: Float, y: Float, text: MarkdownTextMeasure): Piece {
        val blocks = paragraph(inlines)
        return Piece(blocks, x, y, width, Markdown.layout(blocks, styles, width, text).height)
    }

    /** [inlines] at [width] beside a cap box ([cap] = its width and lines; RTL: the cap on the right). */
    fun flow(inlines: List<MarkdownInline>, styles: MarkdownStyles, width: Float, cap: Pair<Float, Int>?, rtl: Boolean, text: MarkdownTextMeasure): Flow {
        if (cap == null) return piece(inlines, styles, width, 0f, 0f, text).let { Flow(listOf(it), it.height) }
        val (capW, lines) = cap
        val boxed = lines * styles.body.lineHeight
        val beside = maxOf(width - capW, 1f)
        val (head, under) = Markdown.dropCapFlow(inlines, styles.body, beside, lines, styles.chip, text)
        val pieces = ArrayList<Piece>()
        var height = boxed
        if (head.isNotEmpty()) piece(head, styles, beside, if (rtl) 0f else capW, 0f, text).also { pieces.add(it); height = maxOf(boxed, it.height) }
        if (under.isNotEmpty()) piece(under, styles, width, 0f, height, text).also { pieces.add(it); height += it.height }
        return Flow(pieces, height)
    }

    /** Max-content = cap box + the spans on one line; min-content = the wider of the cap box and the widest unbreakable segment. */
    fun intrinsic(inlines: List<MarkdownInline>, styles: MarkdownStyles, cap: Pair<Float, Int>?, min: Boolean, text: MarkdownTextMeasure): Float {
        val blocks = paragraph(inlines)
        val content = if (min) Markdown.minContentWidth(blocks, styles, text) else Markdown.maxContentWidth(blocks, styles, text)
        return when {
            cap == null -> content
            min -> maxOf(content, cap.first)
            else -> cap.first + content
        }
    }
}
