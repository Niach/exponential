package at.exponential.ui.paint

import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.primitives.MarkdownChipStyle
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
        s.mediaMaxWidth = CatalogConstants.MEDIA_INTRINSIC_WIDTH
        s.mediaAspect = CatalogConstants.MEDIA_ASPECT_RATIO
        if (theme == null) return s
        fun part(name: String): PartStyle = theme.part("Markdown", name, props, mode = mode)
        fun spec(p: PartStyle, base: MarkdownTextSpec): MarkdownTextSpec = MarkdownTextSpec(
            size = p.px("fontSize") ?: base.size,
            lineHeight = p.px("lineHeight") ?: base.lineHeight,
            weight = p.props["fontWeight"]?.number?.toInt() ?: base.weight,
            family = p.fontFamily ?: base.family,
        )
        // #VAPP-258 step 5: the chat rhythm re-resolves the root (the node's
        // text style knows no density): its type is the body, its gap the
        // block gap. The document keeps the node's style and the `sm` gap.
        val root = if (props["density"]?.string == "chat") part("root") else null
        if (root != null) s.body = spec(root, s.body)
        s.heading = spec(part("heading"), s.body.copy(weight = 600))
        val code = part("codeBlock")
        s.code = spec(code, s.code.copy(family = theme.monoFamily))
        s.codePad = code.px("padding") ?: code.px("paddingHorizontal") ?: s.codePad
        val quote = part("quote")
        s.quoteBorder = quote.px("borderWidth") ?: s.quoteBorder
        s.quotePad = quote.px("paddingHorizontal") ?: s.quotePad
        s.blockGap = root?.px("gap") ?: theme.spacing("sm")
        s.headingTop = theme.spacing("md")
        s.headingBottom = theme.spacing("xs")
        s.cellPadH = theme.spacing("sm")
        s.cellPadV = theme.spacing("xs")
        hostParts(theme, mode, s)
        return s
    }

    /**
     * #VAPP-258 step 5: the props a Markdown's recipes resolve with:
     * `density: chat` added under the host's chat rhythm (a painter-supplied
     * recipe prop, `catalog/recipes.json`).
     */
    fun props(props: Props, chat: Boolean): Props = if (chat) props + ("density" to JsonValue.Str("chat")) else props

    /** #VAPP-258: the Chip props a Markdown chip resolves its look with (the catalog defaults). */
    val CHIP_PROPS: Props = mapOf("shape" to JsonValue.Str("pill"), "tone" to JsonValue.Str("neutral"), "selected" to JsonValue.Bool(false))

    /**
     * #VAPP-258 (catalog/host.json `markdown`): the inline chip (Text
     * `caption` label, `sm` padding, `xs` icon gap, `iconSm`, `badge`), the
     * audio attachment (the AudioPlayer track line + `xs` + its controls
     * row) and the file row (FileUpload `file`: min height, padding, the
     * icon or the name line).
     */
    private fun hostParts(theme: ThemeHandle, mode: Mode, s: MarkdownStyles) {
        val caption = theme.part("Text", "root", mapOf("variant" to JsonValue.Str("caption")), mode = mode)
        val chipLabel = theme.part("Chip", "label", CHIP_PROPS, mode = mode)
        s.chip = MarkdownChipStyle(
            label = MarkdownTextSpec(
                size = chipLabel.px("fontSize") ?: caption.px("fontSize") ?: 12f,
                lineHeight = chipLabel.px("lineHeight") ?: caption.px("lineHeight") ?: 16f,
                weight = (chipLabel.props["fontWeight"] ?: caption.props["fontWeight"])?.number?.toInt() ?: s.body.weight,
                family = chipLabel.fontFamily ?: caption.fontFamily ?: s.body.family,
            ),
            padH = theme.spacing("sm"),
            gap = theme.spacing("xs"),
            iconSize = theme.control("iconSm", 16f),
            height = theme.control("badge", 20f),
        )
        val track = theme.part("AudioPlayer", "track", emptyMap(), mode = mode)
        s.audioTitle = track.px("lineHeight") ?: s.body.lineHeight
        s.audioGap = theme.spacing("xs")
        s.audioRow = theme.part("AudioPlayer", "controls", emptyMap(), mode = mode).px("height") ?: theme.control("row", 32f)
        val file = theme.part("FileUpload", "file", emptyMap(), mode = mode)
        val name = theme.part("FileUpload", "fileName", emptyMap(), mode = mode)
        val padV = file.px("paddingVertical") ?: file.px("padding") ?: 0f
        val content = 2 * padV + maxOf(theme.control("iconSm", 16f), name.px("lineHeight") ?: s.body.lineHeight)
        s.fileHeight = maxOf(file.px("minHeight") ?: file.px("height") ?: 0f, content)
    }

    /** #VAPP-258: an audio attachment's title style (the AudioPlayer `track` part over the body). */
    fun audioTitle(theme: ThemeHandle?, mode: Mode, body: ResolvedTextStyle): ResolvedTextStyle {
        val track = theme?.part("AudioPlayer", "track", emptyMap(), mode = mode) ?: return body
        return ResolvedTextStyle(
            track.px("fontSize") ?: body.fontSize,
            track.props["fontWeight"]?.number?.toInt() ?: body.fontWeight,
            track.px("lineHeight") ?: body.lineHeight,
            track.fontFamily ?: body.fontFamily,
        )
    }
}
