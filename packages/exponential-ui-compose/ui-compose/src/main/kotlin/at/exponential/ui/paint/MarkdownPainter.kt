package at.exponential.ui.paint

import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import androidx.compose.ui.graphics.Color
import at.exponential.ui.primitives.MarkdownChip
import at.exponential.ui.primitives.MarkdownChipKindStyle
import at.exponential.ui.primitives.MarkdownChipLook
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
     * body); plain metrics in geometry mode. [fontScale] = the surface's
     * text scale: [body] arrives scaled (the core scales a node's type), the
     * recipe parts read here do not, so their sizes and line heights are
     * multiplied by it (one rule for the measurer and the painter, so the
     * measured height stays the painted one).
     */
    fun styles(theme: ThemeHandle?, mode: Mode, body: ResolvedTextStyle, props: Props, fontScale: Float = 1f): MarkdownStyles {
        val k = fontScale.takeIf { it.isFinite() && it > 0f } ?: 1f
        val bodySpec = MarkdownTextSpec(body.fontSize, body.lineHeight, body.fontWeight, body.fontFamily)
        val s = MarkdownStyles(bodySpec)
        s.mediaMaxWidth = CatalogConstants.MEDIA_INTRINSIC_WIDTH
        s.mediaAspect = CatalogConstants.MEDIA_ASPECT_RATIO
        if (theme == null) return s
        fun part(name: String, extra: Props = emptyMap()): PartStyle = theme.part("Markdown", name, props + extra, mode = mode)
        fun spec(p: PartStyle, base: MarkdownTextSpec): MarkdownTextSpec = MarkdownTextSpec(
            size = p.px("fontSize")?.times(k) ?: base.size,
            lineHeight = p.px("lineHeight")?.times(k) ?: base.lineHeight,
            weight = p.props["fontWeight"]?.number?.toInt() ?: base.weight,
            family = p.fontFamily ?: base.family,
        )
        // #VAPP-258 step 5: the chat rhythm re-resolves the root (the node's
        // text style knows no density): its type is the body, its gap the
        // block gap. The document keeps the node's style and the `sm` gap.
        val root = if (props["density"]?.string == "chat") part("root") else null
        if (root != null) s.body = spec(root, s.body)
        val headingBase = s.body.copy(weight = 600)
        // Round 9 §9.1: an inline paragraph and heading paint the recipes of
        // [inlineRecipes] under the document rhythm, the host's `chat` included.
        val inline = isInline(props)
        fun heading(extra: Props = emptyMap()): PartStyle = part("heading", if (inline) extra + ("density" to JsonValue.Str("document")) else extra)
        fun recipes(list: List<InlineRecipe>, base: MarkdownTextSpec): MarkdownTextSpec = list.fold(base) { over, q -> spec(theme.part(q.component, q.part, q.props, mode = mode), over) }
        s.heading = spec(heading(), headingBase)
        // #VAPP-258: each heading level's type and margins (the `level` the
        // painter supplies; margins collapse like CSS block margins).
        val headingPart = heading()
        s.headingTop = headingPart.px("marginTop") ?: theme.spacing("md")
        s.headingBottom = headingPart.px("marginBottom") ?: theme.spacing("xs")
        fun level(n: Int): Props = mapOf("level" to JsonValue.Num(n.toDouble()))
        s.headingLevels = (1..6).associateWith { if (inline) recipes(inlineRecipes("heading", level = it), headingBase) else spec(heading(level(it)), headingBase) }
        s.headingTops = (1..6).associateWith { heading(level(it)).px("marginTop") ?: s.headingTop }
        s.headingBottoms = (1..6).associateWith { heading(level(it)).px("marginBottom") ?: s.headingBottom }
        if (inline) s.paragraph = recipes(inlineRecipes("paragraph", variant(props)), s.body)
        val item = part("listItem")
        s.itemTop = item.px("marginTop") ?: 0f
        s.itemBottom = item.px("marginBottom") ?: 0f
        val code = part("codeBlock")
        s.code = spec(code, s.code.copy(family = theme.monoFamily))
        s.codePad = code.px("padding") ?: code.px("paddingHorizontal") ?: s.codePad
        s.codeBorder = code.style.borderWidth
        val quote = part("quote")
        s.quoteBorder = quote.px("borderWidth") ?: s.quoteBorder
        s.quotePad = quote.px("paddingHorizontal") ?: s.quotePad
        // #VAPP-258: a block image's corners (the `Markdown/image` recipe).
        s.imageRadius = part("image").style.radius
        s.blockGap = root?.takeUnless { inline }?.px("gap") ?: theme.spacing("sm")
        // #VAPP-258: every cell's paddings (`Markdown/cell`).
        val cell = part("cell")
        s.cellPadH = cell.px("paddingHorizontal") ?: cell.px("padding") ?: theme.spacing("sm")
        s.cellPadV = cell.px("paddingVertical") ?: cell.px("padding") ?: theme.spacing("xs")
        hostParts(theme, mode, s, k)
        // #VAPP-258: a theme's `Markdown/chip*` rules per chip kind (their
        // metrics; the colours are [chipLooks]').
        val kinds = HashMap<String, MarkdownChipKindStyle>()
        for (kind in MarkdownChip.KINDS) {
            val kp = mapOf("chip" to JsonValue.Str(kind))
            val box = part("chip", kp)
            fun over(p: PartStyle, base: MarkdownTextSpec): MarkdownTextSpec? =
                if (p.px("fontSize") == null && p.px("lineHeight") == null && p.props["fontWeight"] == null && p.fontFamily == null) null else spec(p, base)
            val look = MarkdownChipKindStyle(
                label = over(part("chipLabel", kp), s.chip.label),
                ident = over(part("chipIdent", kp), s.chip.identSpec.copy(family = theme.monoFamily)),
                padH = box.px("paddingHorizontal") ?: box.px("padding"),
                radius = box.px("borderRadius"),
            )
            if (look != MarkdownChipKindStyle()) kinds[kind] = look
        }
        s.chip = s.chip.copy(kinds = kinds)
        return s
    }

    /** One recipe an inline-profile part paints with (`RecipeQuery`). */
    data class InlineRecipe(val component: String, val part: String, val props: Props)

    /** Round 9 §9.1: does this Markdown take the inline profile? */
    fun isInline(props: Props): Boolean = props["profile"]?.string == "inline"

    /** Round 9 §9.1: an inline Markdown's paragraph type. */
    fun variant(props: Props): String = if (props["variant"]?.string == "caption") "caption" else "body"

    /**
     * Round 9 §9.1 (`markdownInlineRecipes`, fixtures/markdown-inline.json):
     * the recipes an inline-profile part paints with, first to last, later
     * over earlier: a paragraph IS a Text of its [variant]; a heading is what
     * the `Heading` macro paints (Text `title` + Heading/root; h5, h6 paint h4).
     */
    fun inlineRecipes(part: String, variant: String = "body", level: Int = 2): List<InlineRecipe> {
        fun text(v: String) = InlineRecipe("Text", "root", mapOf("variant" to JsonValue.Str(v)))
        if (part == "paragraph") return listOf(text(variant))
        return listOf(text("title"), InlineRecipe("Heading", "root", mapOf("level" to JsonValue.Str("h${level.coerceIn(1, 4)}"))))
    }

    /** Round 9 §9.1: the colour of an inline paragraph and heading (last recipe that sets one); null = the node's. */
    fun inlineInk(theme: ThemeHandle?, mode: Mode, recipes: List<InlineRecipe>): Color? =
        recipes.mapNotNull { q -> theme?.part(q.component, q.part, q.props, mode = mode)?.color }.lastOrNull()

    /**
     * #VAPP-258 step 5: the props a Markdown's recipes resolve with:
     * `density: chat` added under the host's chat rhythm (a painter-supplied
     * recipe prop, `catalog/recipes.json`).
     */
    fun props(props: Props, chat: Boolean): Props = if (chat) props + ("density" to JsonValue.Str("chat")) else props

    /**
     * #VAPP-258: fenced blocks in CodeBlock's token colours under [theme]
     * (`CodeBlock/token {kind}`), through the catalog's one tokenizer
     * ([at.exponential.ui.compose.markdownCodeSpans]): what the `Markdown`
     * leaf paints, for a host that draws the primitives' MarkdownView itself.
     */
    fun codeHighlighter(theme: ThemeHandle, mode: Mode): at.exponential.ui.primitives.MarkdownCodeHighlighter =
        at.exponential.ui.primitives.MarkdownCodeHighlighter { text, lang ->
            at.exponential.ui.compose.markdownCodeSpans(text, lang) { kind ->
                theme.part("CodeBlock", "token", mapOf("kind" to JsonValue.Str(kind)), mode = mode).color
            }
        }

    /**
     * #VAPP-258: each chip kind's `Markdown/chip*` colours under [props]
     * (the density), painted over the Chip look ([MarkdownChipLook]).
     */
    fun chipLooks(theme: ThemeHandle, mode: Mode, props: Props): Map<String, MarkdownChipLook> {
        val out = HashMap<String, MarkdownChipLook>()
        for (kind in MarkdownChip.KINDS) {
            val p = props + ("chip" to JsonValue.Str(kind))
            val box = theme.part("Markdown", "chip", p, mode = mode)
            val label = theme.part("Markdown", "chipLabel", p, mode = mode)
            val ident = theme.part("Markdown", "chipIdent", p, mode = mode)
            val look = MarkdownChipLook(
                background = box.style.background,
                border = box.style.borderColor ?: Color.Unspecified,
                borderWidth = box.style.borderWidth.takeIf { it > 0f },
                ink = label.color ?: Color.Unspecified,
                identInk = ident.color ?: Color.Unspecified,
            )
            if (look != MarkdownChipLook()) out[kind] = look
        }
        return out
    }

    /** #VAPP-258: the Chip props a Markdown chip resolves its look with (the catalog defaults). */
    val CHIP_PROPS: Props = mapOf("shape" to JsonValue.Str("pill"), "tone" to JsonValue.Str("neutral"), "selected" to JsonValue.Bool(false))

    /**
     * #VAPP-258 (catalog/host.json `markdown`): the inline chip (Text
     * `caption` label, `sm` padding, `xs` icon gap, `iconSm`, `badge`), the
     * audio attachment (the AudioPlayer track line + `xs` + its controls
     * row) and the file row (FileUpload `file`: min height, padding, the
     * icon or the name line).
     */
    private fun hostParts(theme: ThemeHandle, mode: Mode, s: MarkdownStyles, k: Float) {
        val caption = theme.part("Text", "root", mapOf("variant" to JsonValue.Str("caption")), mode = mode)
        val chipLabel = theme.part("Chip", "label", CHIP_PROPS, mode = mode)
        s.chip = MarkdownChipStyle(
            label = MarkdownTextSpec(
                size = (chipLabel.px("fontSize") ?: caption.px("fontSize") ?: 12f) * k,
                lineHeight = (chipLabel.px("lineHeight") ?: caption.px("lineHeight") ?: 16f) * k,
                weight = (chipLabel.props["fontWeight"] ?: caption.props["fontWeight"])?.number?.toInt() ?: s.body.weight,
                family = chipLabel.fontFamily ?: caption.fontFamily ?: s.body.family,
            ),
            padH = theme.spacing("sm"),
            gap = theme.spacing("xs"),
            iconSize = theme.control("iconSm", 16f),
            height = theme.control("badge", 20f),
        )
        val track = theme.part("AudioPlayer", "track", emptyMap(), mode = mode)
        s.audioTitle = track.px("lineHeight")?.times(k) ?: s.body.lineHeight
        s.audioGap = theme.spacing("xs")
        s.audioRow = theme.part("AudioPlayer", "controls", emptyMap(), mode = mode).px("height") ?: theme.control("row", 32f)
        val file = theme.part("FileUpload", "file", emptyMap(), mode = mode)
        val name = theme.part("FileUpload", "fileName", emptyMap(), mode = mode)
        val padV = file.px("paddingVertical") ?: file.px("padding") ?: 0f
        val content = 2 * padV + maxOf(theme.control("iconSm", 16f), name.px("lineHeight")?.times(k) ?: s.body.lineHeight)
        s.fileHeight = maxOf(file.px("minHeight") ?: file.px("height") ?: 0f, content)
    }

    /** #VAPP-258: an audio attachment's title style (the AudioPlayer `track` part over the body). */
    fun audioTitle(theme: ThemeHandle?, mode: Mode, body: ResolvedTextStyle, fontScale: Float = 1f): ResolvedTextStyle {
        val track = theme?.part("AudioPlayer", "track", emptyMap(), mode = mode) ?: return body
        return ResolvedTextStyle(
            track.px("fontSize")?.times(fontScale) ?: body.fontSize,
            track.props["fontWeight"]?.number?.toInt() ?: body.fontWeight,
            track.px("lineHeight")?.times(fontScale) ?: body.lineHeight,
            track.fontFamily ?: body.fontFamily,
        )
    }
}
