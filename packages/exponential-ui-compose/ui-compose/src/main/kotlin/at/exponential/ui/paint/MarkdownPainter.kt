package at.exponential.ui.paint

import at.exponential.ui.json.Props
import at.exponential.ui.primitives.MarkdownStyles
import at.exponential.ui.primitives.MarkdownTextSpec
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.PartStyle
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle

/**
 * The markdown painter's model half: resolves the `Markdown` recipe parts
 * into [MarkdownStyles], shared by the measurer and the view.
 */
object MarkdownPainter {
    /**
     * From the theme's `Markdown` recipes (the root's own text style is the
     * body); plain metrics in geometry mode.
     */
    fun styles(theme: ThemeHandle?, mode: Mode, body: ResolvedTextStyle, props: Props): MarkdownStyles {
        val bodySpec = MarkdownTextSpec(body.fontSize, body.lineHeight, body.fontWeight, body.fontFamily)
        val s = MarkdownStyles(bodySpec)
        if (theme == null) return s
        fun part(name: String): PartStyle = theme.part("Markdown", name, props, mode = mode)
        fun spec(p: PartStyle, base: MarkdownTextSpec): MarkdownTextSpec = MarkdownTextSpec(
            size = p.px("fontSize") ?: base.size,
            lineHeight = p.px("lineHeight") ?: base.lineHeight,
            weight = p.props["fontWeight"]?.number?.toInt() ?: base.weight,
            family = p.fontFamily ?: base.family,
        )
        s.heading = spec(part("heading"), s.body.copy(weight = 600))
        val code = part("codeBlock")
        s.code = spec(code, s.code.copy(family = theme.monoFamily))
        s.codePad = code.px("padding") ?: code.px("paddingHorizontal") ?: s.codePad
        val quote = part("quote")
        s.quoteBorder = quote.px("borderWidth") ?: s.quoteBorder
        s.quotePad = quote.px("paddingHorizontal") ?: s.quotePad
        s.blockGap = theme.spacing("sm")
        s.headingTop = theme.spacing("md")
        s.headingBottom = theme.spacing("xs")
        s.cellPadH = theme.spacing("sm")
        s.cellPadV = theme.spacing("xs")
        return s
    }
}
