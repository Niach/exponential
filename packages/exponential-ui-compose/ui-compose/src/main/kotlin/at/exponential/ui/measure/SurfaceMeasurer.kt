package at.exponential.ui.measure

import androidx.compose.ui.geometry.Size
import at.exponential.ui.catalog.CatalogConstants
import at.exponential.ui.extension.ExtensionLeaf
import at.exponential.ui.extension.ExtensionRegistry
import at.exponential.ui.ffi.FfiControlBox
import at.exponential.ui.ffi.FfiHeightRequest
import at.exponential.ui.ffi.FfiIntrinsics
import at.exponential.ui.ffi.FfiLeaf
import at.exponential.ui.ffi.Measurer
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.percent
import at.exponential.ui.json.px
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.paint.DateModel
import at.exponential.ui.paint.MarkdownPainter
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.theme.GeometrySpacing
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.PartStyle
import at.exponential.ui.theme.ResolvedTextStyle
import at.exponential.ui.theme.ThemeHandle
import kotlin.math.max
import kotlin.math.min

/** A control's box around its content (the core's `ControlBox`, dp). */
class ControlBox(
    val paddingHorizontal: Float = 0f,
    val paddingVertical: Float = 0f,
    val borderWidth: Float = 0f,
    val gap: Float = 0f,
    val minWidth: Float? = null,
    val minHeight: Float? = null,
    val width: Float? = null,
    val height: Float? = null,
    /** Each side `[top, right, bottom, left]` (all 0 = the averages above). */
    val padding: List<Float> = listOf(0f, 0f, 0f, 0f),
) {
    /** From the facade's record. */
    constructor(c: FfiControlBox) : this(
        c.paddingHorizontal, c.paddingVertical, c.borderWidth, c.gap, c.minWidth, c.minHeight, c.width, c.height,
        listOf(c.paddingTop, c.paddingRight, c.paddingBottom, c.paddingLeft),
    )

    private val perSide: Boolean get() = padding.any { it != 0f }

    /** The horizontal and vertical insets around the content. */
    val insets: Pair<Float, Float>
        get() = if (perSide) {
            (padding[1] + padding[3] + 2 * borderWidth) to (padding[0] + padding[2] + 2 * borderWidth)
        } else {
            2 * (paddingHorizontal + borderWidth) to 2 * (paddingVertical + borderWidth)
        }

    /** The top inset (padding + border). */
    val topInset: Float get() = (if (perSide) padding[0] else paddingVertical) + borderWidth

    /** The content wrap width inside a border-box wrap (`0` stays min-content). */
    fun innerWrap(wrap: Float?): Float? {
        val w = wrap ?: return null
        return if (w <= 0f) 0f else max(0f, w - insets.first)
    }

    /** Content size → the border box: insets added, fixed sizes win, minimums clamp. */
    fun borderBox(content: Size): Size {
        val (ih, iv) = insets
        val w = max(width ?: (content.width + ih), minWidth ?: 0f)
        val h = max(height ?: (content.height + iv), minHeight ?: 0f)
        return Size(w, h)
    }
}

/** One leaf the core asks about (the facade's `FfiLeaf`, props parsed). */
class LeafRequest(l: FfiLeaf, ownerOf: (Int, String, String?) -> String? = { _, _, _ -> null }) {
    val index: Int = l.index.toInt()
    val id: String = l.id
    val component: String = l.component
    val part: String? = l.part
    val props: Props = JsonValue.parse(l.propsJson).obj ?: emptyMap()
    /** The text prop as shown (a number or boolean as its display string, `412`). */
    val text: String = l.text
    /** The core's style: tracking, case and italics included (inherited like CSS). */
    val textStyle: ResolvedTextStyle = ResolvedTextStyle(l.textStyle)
    val control: ControlBox = ControlBox(l.control)
    val lines: Int? = l.lines?.toInt()

    /**
     * The component owning the part (`Tabs` for `Tabs/tab`, the macro for a
     * macro part); null for a plain node. The facade names it; [ownerOf]
     * covers a facade that does not.
     */
    val ownerComponent: String? = if (part == null) null else l.ownerComponent ?: ownerOf(index, id, part)
}

/**
 * The painter's [Measurer]: answers the core's BATCHED questions with the
 * Compose [TextShaper], the recipe boxes and the extension painters. Every
 * answer is the BORDER box of the control (the leaf's `ControlBox` padding
 * and border added around the content; fixed / minimum sizes win), the
 * gpui painter's rules plus round 2's explicit sizes (§7). Called
 * synchronously from inside `Surface.layout` on the thread that runs the
 * pass (the main thread).
 */
class SurfaceMeasurer(
    val theme: ThemeHandle?,
    val mode: Mode,
    val extensions: ExtensionRegistry,
    /** Extension node id → its kind (`FfiLeaf` carries no kind). */
    val kinds: Map<String, String>,
    val generation: Long,
    val shaper: TextShaper,
    /** Field id → the text the user typed (not yet in the props). */
    val live: Map<String, String> = emptyMap(),
    /** (index, id, part) → the part's owner component. */
    val ownerOf: (Int, String, String?) -> String? = { _, _, _ -> null },
    /** A painter failed measuring component `id` (`onPaintError`; it measures 0×0 and paints an empty box). */
    val onPaintError: (id: String, message: String) -> Unit = { _, _ -> },
    /** Whether a markdown image's src passes the host's media policy (the painter asks the same question). */
    val mediaAllowed: (String) -> Boolean = { true },
) : Measurer {
    /** How many answers this measurer gave (stats). */
    var calls = 0
        private set

    override fun measureId(): ULong = (MEASURE_IDENTITY + generation).toULong()

    override fun measureIntrinsics(leaves: List<FfiLeaf>): List<FfiIntrinsics> = leaves.map { raw ->
        val leaf = LeafRequest(raw, ownerOf)
        val maxC = answer(leaf, null)
        val minC = answer(leaf, 0f)
        FfiIntrinsics(min(minC.size.width, maxC.size.width), maxC.size.width, maxC.size.height, maxC.baseline)
    }

    override fun measureHeights(leaves: List<FfiLeaf>, requests: List<FfiHeightRequest>): List<Float> {
        val byIndex = HashMap<UInt, LeafRequest>()
        for (l in leaves) byIndex[l.index] = LeafRequest(l, ownerOf)
        return requests.map { r ->
            val leaf = byIndex[r.index] ?: return@map 0f
            answer(leaf, r.width).size.height
        }
    }

    /** A border box and its first baseline from the box top (null = the bottom edge). */
    class Answer(val size: Size, val baseline: Float? = null)

    /** Content size and its first baseline from the CONTENT top. */
    private class Content(val w: Float, val h: Float, val baseline: Float? = null)

    // rules

    private fun part(component: String, part: String, props: Props, states: List<String> = emptyList()): PartStyle =
        theme?.part(component, part, props, states, mode) ?: PartStyle.EMPTY

    private fun spacing(name: String): Float = theme?.spacing(name) ?: GeometrySpacing.value(name)

    private fun control(name: String, fallback: Float): Float = theme?.control(name, fallback) ?: fallback

    private fun shown(text: String, leaf: LeafRequest): String = applyTransform(text, leaf.textStyle.textTransform)

    private fun ts(leaf: LeafRequest): ResolvedTextStyle = leaf.textStyle

    private fun line(text: String, ts: ResolvedTextStyle): Float = shaper.maxContent(text, ts)

    private fun baseline(ts: ResolvedTextStyle): Float = shaper.baseline(ts)

    /** A one-line text with fixed chrome beside it (lead + text + trail). */
    private fun row(text: String, ts: ResolvedTextStyle, lead: Float, trail: Float, wrap: Float?): Content {
        val w = lead + line(text, ts) + trail
        val used = when {
            wrap == null -> w
            wrap <= 0f -> lead + trail
            else -> min(w, wrap)
        }
        return Content(used, ts.lineHeight, baseline(ts))
    }

    /** Plain wrapped text (round 2: an empty text is 0 lines). */
    private fun para(text: String, ts: ResolvedTextStyle, wrap: Float?, lines: Int?): Content {
        if (text.isEmpty()) return Content(0f, 0f, null)
        val s = shaper.measure(text, ts, wrap, lines)
        return Content(s.width, s.height, baseline(ts))
    }

    /** The border box of `leaf` at `wrap` (null = max-content, 0 = min-content). */
    fun answer(leaf: LeafRequest, wrap: Float?): Answer {
        calls += 1
        if (leaf.component == "Extension") {
            val kind = kinds[leaf.id] ?: return Answer(Size.Zero)
            val painter = extensions.painter(kind) ?: return Answer(Size.Zero)
            val size = try {
                painter.measure(ExtensionLeaf(leaf, theme, mode), wrap)
            } catch (e: Exception) {
                onPaintError(leaf.id, "$kind: ${e.message ?: e.toString()}")
                null
            }
            return Answer(size ?: Size.Zero)
        }
        return measureLeaf(leaf, wrap)
    }

    /** The per-component rules (everything but extensions). */
    fun measureLeaf(leaf: LeafRequest, wrap: Float?): Answer {
        val c = leaf.control
        val inner = c.innerWrap(wrap)
        val props = leaf.props
        val ts = ts(leaf)
        val p = leaf.part
        fun plain(w: Float, h: Float) = Content(w, h)
        val content: Content = when {
            isInlineField(leaf.component, p) -> inlineField(leaf)
            p == "trigger" && leaf.component in PICKERS -> trigger(leaf)
            else -> when (leaf.component) {
                "Text" -> text(leaf, ts, inner)
                "Markdown" -> markdown(leaf, inner)
                "Button", "Toggle" -> button(leaf)
                "Link" -> {
                    val label = shown(props.shownText("label").ifEmpty { props.str("href") }, leaf)
                    Content(line(label, ts), ts.lineHeight, baseline(ts))
                }
                "Icon" -> plain(16f, 16f)
                "Avatar" -> plain(32f, 32f)
                "Image", "Video" -> media(props, inner).let { plain(it.width, it.height) }
                "AudioPlayer" -> {
                    // Round 2 §7: the title line + xs + a controls row of `$control.row`.
                    val track = if (props.str("title").isEmpty()) 0f else ts.lineHeight + spacing("xs")
                    val w = when {
                        inner == null -> 300f
                        inner <= 0f -> 160f
                        else -> inner
                    }
                    plain(w, track + control("row", 32f))
                }
                "Spinner" -> plain(20f, 20f)
                "Ring" -> plain(32f, 32f)
                "Skeleton" -> skeleton(props, inner).let { plain(it.width, it.height) }
                "Chart" -> chart(leaf, inner).let { plain(it.width, it.height) }
                "Composer" -> composer(leaf, inner).let { plain(it.width, it.height) }
                // depth × `treeGuideColumn` wide; no height of its own (it
                // stretches to its row, round 2 §7).
                "TreeGuides" -> plain(max((props.num("depth") ?: 0.0).toFloat(), 0f) * TREE_GUIDE_COLUMN, 0f)
                "Segmented" -> segmented(leaf).let { plain(it.width, it.height) }
                "Unknown" -> {
                    val label = part("Unknown", "label", props)
                    val lts = ResolvedTextStyle(label.px("fontSize") ?: 12f, 400, label.px("lineHeight") ?: 16f, label.fontFamily)
                    para(unknownLabel(props, leaf.component), lts, inner, null)
                }
                "Box" -> if (p == "indicator") {
                    // The dots row: the recipe sizes each DOT, the row is one dot tall (gpui, web).
                    val n = max((props.num("count") ?: 0.0).toFloat(), 0f)
                    val recipe = part("Carousel", "indicator", props)
                    val dot = recipe.width ?: 8f
                    val gap = spacing("xs")
                    plain(n * dot + max(n - 1f, 0f) * gap, recipe.height ?: dot)
                } else plain(0f, 0f)
                "Input" -> when (p) {
                    "field" -> Content(FIELD_INTRINSIC_WIDTH, ts.lineHeight, baseline(ts))
                    null -> plain(FIELD_INTRINSIC_WIDTH, 36f)
                    else -> plain(0f, 0f)
                }
                "Textarea" -> when (p) {
                    "field" -> textarea(leaf, inner).let { Content(it.width, it.height, baseline(ts)) }
                    null -> plain(FIELD_INTRINSIC_WIDTH, 72f)
                    else -> plain(0f, 0f)
                }
                "Select", "DatePicker", "TimePicker", "DateRangePicker", "NumberField", "ChipInput" ->
                    if (p == null) Content(FIELD_INTRINSIC_WIDTH, 36f) else plain(0f, 0f)
                "Checkbox" -> if (p == "box" || p == "checkbox" || p == null) plain(16f, 16f) else plain(0f, 0f)
                "Radio" -> if (p == "dot" || p == null) plain(16f, 16f) else plain(0f, 0f)
                "Switch" -> when (p) {
                    "track" -> plain(32f, 20f)
                    null -> plain(44f, 24f)
                    else -> plain(0f, 0f)
                }
                "Slider" -> when (p) {
                    "track" -> plain(FIELD_INTRINSIC_WIDTH, part("Slider", "thumb", props).height ?: control("slider", 16f))
                    null -> plain(FIELD_INTRINSIC_WIDTH, 16f)
                    else -> plain(0f, 0f)
                }
                // Geometry mode (no theme): these natives are ONE measured leaf.
                "Table" -> if (p == null) {
                    val rows = props.list("rows").size
                    plain(inner?.takeIf { it > 0f } ?: 320f, (rows + 1) * 36f)
                } else plain(0f, 0f)
                "CodeBlock" -> if (p == null) {
                    para(props.str("code"), ts.copy(fontFamily = "ui-monospace"), null, null)
                } else plain(0f, 0f)
                "FileUpload" -> if (p == null) plain(240f, 96f) else plain(0f, 0f)
                else -> plain(0f, 0f)
            }
        }
        val box = c.borderBox(Size(content.w, content.h))
        val b = content.baseline?.let { cb ->
            val fixed = c.height != null && kotlin.math.abs(c.height - (content.h + c.insets.second)) > 0.5f
            if (fixed || p in CENTRED_PARTS || leaf.component == "Button" || leaf.component == "Toggle") {
                // A control centres its line in its box.
                max(0f, (box.height - content.h) / 2f) + cb
            } else {
                c.topInset + cb
            }
        }
        return Answer(box, b)
    }

    /** Any `Text` leaf: a part's chrome beside one line, typed Table cells, else a paragraph. */
    private fun text(leaf: LeafRequest, ts: ResolvedTextStyle, inner: Float?): Content {
        val props = leaf.props
        val raw = shown(props.shownText("text"), leaf)
        val countW = props["count"]?.displayText?.takeIf { it.isNotEmpty() }?.let { line(it, ts) } ?: 0f
        val (lead, trail) = textChrome(leaf.ownerComponent ?: leaf.component, leaf.part, props, leaf.control.gap, countW, spacing("xs"))
        if (lead != 0f || trail != 0f) return row(raw, ts, lead, trail, inner)
        if (leaf.part == "cell" && props.str("cellType") == "boolean") return Content(16f, ts.lineHeight)
        if (leaf.part == "cell" && props.str("cellType") == "badge" && raw.isNotEmpty()) {
            val small = ts.copy(fontSize = max(ts.fontSize - 2f, 10f))
            return Content(line(raw, small) + 16f, ts.lineHeight, baseline(ts))
        }
        if (leaf.ownerComponent == "CodeBlock" && leaf.part == "code") {
            // A code line never wraps (the block scrolls).
            return if (raw.isEmpty()) Content(0f, ts.lineHeight) else Content(line(raw, ts), ts.lineHeight, baseline(ts))
        }
        return para(raw, ts, inner, leaf.lines)
    }

    private fun markdown(leaf: LeafRequest, wrap: Float?): Content {
        val blocks = Markdown.resolveImages(Markdown.parse(leaf.props.str("text")), mediaAllowed)
        val ts = leaf.textStyle
        val styles = MarkdownPainter.styles(theme, mode, ts, leaf.props)
        val md = MarkdownShaper(shaper, theme?.monoFamily)
        val clamp = leaf.props.num("lines")?.toInt()?.takeIf { it > 0 }
        fun height(w: Float): Float {
            val h = Markdown.layout(blocks, styles, w, md).height
            return if (clamp != null) min(h, clamp * ts.lineHeight) else h
        }
        val out = when {
            wrap == null -> {
                val w = Markdown.maxContentWidth(blocks, styles, md)
                Size(w, height(w))
            }
            wrap <= 0f -> {
                val mw = Markdown.minContentWidth(blocks, styles, md)
                Size(mw, height(mw))
            }
            else -> Size(wrap, height(wrap))
        }
        return Content(out.width, max(out.height, if (blocks.isEmpty()) 0f else ts.lineHeight), if (blocks.isEmpty()) null else baseline(ts))
    }

    private fun button(leaf: LeafRequest): Content {
        val props = leaf.props
        val ts = ts(leaf)
        val label = shown(props.shownText("label"), leaf)
        val hasIcon = props.str("icon").isNotEmpty() || props.flag("loading")
        val iconOnly = props.str("size") == "icon"
        val icon = if (hasIcon || iconOnly) (part(leaf.component, "icon", props).width ?: control("iconSm", 16f)) else 0f
        val labelW = if (iconOnly || label.isEmpty()) 0f else line(label, ts)
        val gap = if (hasIcon && labelW > 0f) leaf.control.gap else 0f
        val w = (if (hasIcon || iconOnly) icon else 0f) + gap + labelW
        val h = max(ts.lineHeight, if (hasIcon) icon else 0f)
        return Content(w, h, if (labelW > 0f) (h - ts.lineHeight) / 2f + baseline(ts) else null)
    }

    /** Image / Video (round 2 §7): `aspectRatio` (default 16:9), max-content 320 wide, min-content 0. */
    private fun media(props: Props, wrap: Float?): Size {
        val ratio = props.num("aspectRatio")?.toFloat()?.takeIf { it > 0f } ?: MEDIA_ASPECT_RATIO
        val fixedW = props.px("width")
        val fixedH = props.px("height")
        val w = when {
            fixedW != null -> fixedW
            wrap == null -> MEDIA_INTRINSIC_WIDTH
            wrap <= 0f -> 0f
            else -> min(wrap, max(MEDIA_INTRINSIC_WIDTH, wrap))
        }
        val h = fixedH ?: if (w > 0f) w / ratio else MEDIA_INTRINSIC_WIDTH / ratio
        return Size(w, h)
    }

    private fun skeleton(props: Props, wrap: Float?): Size {
        val px = props.px("width")
        val pct = props.percent("width")
        val w = when {
            px != null -> px
            pct != null -> if (wrap != null && wrap > 0f) wrap * pct else if (wrap != null) 0f else 240f
            wrap == null -> 240f
            wrap > 0f -> wrap
            else -> 0f
        }
        return Size(w, props.px("height") ?: 16f)
    }

    /** A Chart (round 2 §7): `height` is the WHOLE box (title, legend, axes inside); min-content width 0. */
    private fun chart(leaf: LeafRequest, wrap: Float?): Size {
        val props = leaf.props
        val spark = props.str("kind") == "sparkline"
        val w = when {
            wrap == null -> if (spark) 120f else 320f
            wrap <= 0f -> 0f
            else -> wrap
        }
        return Size(w, (props.num("height") ?: if (spark) 32.0 else 200.0).toFloat())
    }

    private fun composer(leaf: LeafRequest, wrap: Float?): Size {
        val props = leaf.props
        val ts = leaf.textStyle
        val field = part("Composer", "field", props)
        val fs = field.px("fontSize") ?: ts.fontSize
        val lh = field.px("lineHeight") ?: ts.lineHeight
        val minH = field.px("minHeight") ?: lh
        val send = part("Composer", "send", props).height ?: control("buttonIcon", 36f)
        val gap = leaf.control.gap
        val value = live[leaf.id] ?: props.str("value")
        // An empty field (no placeholder either) is one line tall.
        val text = value.ifEmpty { props.str("placeholder") }.ifEmpty { " " }
        val fieldTs = ResolvedTextStyle(fs, ts.fontWeight, lh, ts.fontFamily)
        val w = when {
            wrap == null -> 320f
            wrap <= 0f -> 120f
            else -> wrap
        }
        val th = shaper.measure(text, fieldTs, max(w, 1f), null).height
        return Size(w, min(max(th, minH), 200f) + gap + send)
    }

    /** A Textarea field: `rows` lines, or (autosize) its live text's wrapped lines clamped to `rows..maxRows`. */
    private fun textarea(leaf: LeafRequest, inner: Float?): Size {
        val props = leaf.props
        val ts = leaf.textStyle
        val rows = max((props.num("rows") ?: 3.0).toFloat(), 1f)
        if (!props.flag("autosize")) return Size(FIELD_INTRINSIC_WIDTH, rows * ts.lineHeight)
        val value = live[leaf.id] ?: props["value"]?.displayText ?: ""
        val maxRows = props.num("maxRows")?.toFloat() ?: Float.POSITIVE_INFINITY
        val at = inner?.takeIf { it > 0f } ?: FIELD_INTRINSIC_WIDTH
        val lines = if (value.isEmpty()) 1f else shaper.wrappedHeight(value, ts, at) / ts.lineHeight
        return Size(FIELD_INTRINSIC_WIDTH, min(max(lines, rows), max(maxRows, rows)) * ts.lineHeight)
    }

    /**
     * A `bar` Segmented (round 3, the old TabBar): `$control.tabBar` tall,
     * each item a COLUMN (icon over a caption label) sharing the width; its
     * min-content width = the widest of icon and label per item (gpui).
     */
    private fun segmentedBar(leaf: LeafRequest): Size {
        val ts = barCaption(ts(leaf))
        val icon = control("iconMd", 20f)
        val pad = spacing("xs")
        var w = 0f
        for (it in leaf.props.list("items")) {
            val label = shown(it["label"]?.displayText ?: "", leaf)
            val lw = if (label.isEmpty()) 0f else line(label, ts)
            val iw = if (it["icon"]?.string != null) icon else 0f
            w += max(lw, iw) + 2 * pad
        }
        return Size(w, control("tabBar", 56f))
    }

    /** The `bar` Segmented's caption: the `Text` caption recipe (size, line height), the item's weight. */
    internal fun barCaption(base: ResolvedTextStyle): ResolvedTextStyle {
        val caption = part("Text", "root", mapOf("variant" to JsonValue.Str("caption")))
        return ResolvedTextStyle(caption.px("fontSize") ?: 12f, base.fontWeight, caption.px("lineHeight") ?: 16f, base.fontFamily)
    }

    private fun segmented(leaf: LeafRequest): Size {
        val props = leaf.props
        if (props.str("variant") == "bar") return segmentedBar(leaf)
        val items = props.list("items")
        val item = part("Segmented", "item", props)
        val pad = item.px("paddingHorizontal") ?: item.px("padding") ?: 12f
        val border = item.px("borderWidth") ?: 0f
        val h = item.height ?: 36f
        val base = ts(leaf)
        val ts = ResolvedTextStyle(item.px("fontSize") ?: base.fontSize, (item.props["fontWeight"]?.number ?: 500.0).toInt(), base.lineHeight, item.fontFamily, base.letterSpacing)
        // Icon ↔ label: the `Segmented/item` recipe's gap (none = 0, as the web and gpui).
        val itemGap = item.px("gap") ?: 0f
        var w = 0f
        for ((i, it) in items.withIndex()) {
            val label = shown(it["label"]?.displayText ?: "", leaf)
            val hasIcon = it["icon"]?.string != null
            val lw = if (label.isEmpty()) 0f else line(label, ts)
            val iw = if (hasIcon) 16f else 0f
            val innerGap = if (hasIcon && lw > 0f) itemGap else 0f
            w += 2 * (pad + border) + iw + innerGap + lw
            if (i > 0) w += leaf.control.gap
        }
        return Size(w, h)
    }

    /**
     * A picker `trigger` (Select, DatePicker, DateRangePicker, TimePicker;
     * round 2 §7): content-sized, the core's `text` (else the placeholder)
     * plus the gap and the 16 dp glyph, one line, no 160 floor.
     */
    private fun trigger(leaf: LeafRequest): Content {
        val ts = ts(leaf)
        val c = leaf.control
        val text = leaf.props.str("text").ifEmpty { leaf.props.str("placeholder") }
        val gap = max(c.gap, spacing("sm"))
        return Content(line(shown(text, leaf), ts) + gap + 16f, ts.lineHeight, baseline(ts))
    }

    /**
     * An inline field (NumberField / ChipInput `.input`, Select `.search`):
     * the wider of its LIVE text (else the prop) and the placeholder plus the
     * caret, at least 24 dp, one line; a search with an icon adds it. Fields
     * paint without tracking, so they measure without it.
     */
    private fun inlineField(leaf: LeafRequest): Content {
        val props = leaf.props
        val ts = leaf.textStyle
        val typed = live[leaf.id] ?: props.str("text")
        val text = typed.ifEmpty { props["value"]?.displayText ?: "" }
        val w = max(line(text, ts), line(props.str("placeholder"), ts)) + 2f
        val icon = if (leaf.part == "search" && props["icon"] != null) 16f + 8f else 0f
        return Content(max(w, 24f) + icon, ts.lineHeight, baseline(ts))
    }

    companion object {
        /** The identity the core keys its memo on ("Compose"; a font change = a new generation). */
        const val MEASURE_IDENTITY: Long = 0x436F6D706F7365L

        /** `catalog/layout.json` (round 2 §7). */
        val FIELD_INTRINSIC_WIDTH: Float = CatalogConstants.FIELD_INTRINSIC_WIDTH
        val MEDIA_INTRINSIC_WIDTH: Float = CatalogConstants.MEDIA_INTRINSIC_WIDTH
        val MEDIA_ASPECT_RATIO: Float = CatalogConstants.MEDIA_ASPECT_RATIO
        val TREE_GUIDE_COLUMN: Float = CatalogConstants.TREE_GUIDE_COLUMN

        private val PICKERS = setOf("Select", "DatePicker", "TimePicker", "DateRangePicker")
        private val CENTRED_PARTS = setOf("trigger", "field", "input", "search")

        /** Is a leaf a host-owned one-line field (`search`, NumberField / ChipInput `input`)? */
        fun isInlineField(component: String, part: String?): Boolean =
            (component == "Select" && part == "search") || (component == "NumberField" && part == "input") || (component == "ChipInput" && part == "input")

        /**
         * The leading / trailing chrome (dp) a one-line text part carries
         * beside its text (an icon, a count, a chevron, a check, a sort arrow),
         * keyed by the OWNER component (the gpui rule).
         */
        fun textChrome(owner: String, part: String?, props: Props, gap: Float, countW: Float, xs: Float = 4f): Pair<Float, Float> {
            fun icon(key: String) = if (props[key] != null && props[key] !is JsonValue.Null) 16f + max(gap, 4f) else 0f
            return when {
                // The web's tab body: [icon] label [count], `$spacing.xs` apart.
                owner == "Tabs" && part == "tab" -> (if (props["icon"] != null) 16f + xs else 0f) to (if (props["count"] != null) xs + countW else 0f)
                // Round 2 §7: the count is its own muted part after the title.
                owner == "Accordion" && part == "trigger" -> 0f to ((if (props["count"] != null) 8f + countW else 0f) + 16f + max(gap, 8f))
                owner == "Select" && part == "item" -> icon("icon") to (16f + max(gap, 8f))
                owner == "Menu" && part == "itemLabel" -> icon("icon") to 0f
                owner == "Table" && part == "headerCell" -> 0f to (if (props["sortIcon"] != null || props.flag("sortable")) 16f + 4f else 0f)
                else -> 0f to 0f
            }
        }

        /** The text an `Unknown` placeholder shows. */
        fun unknownLabel(props: Props, component: String): String {
            val name = props.str("component").ifEmpty { component }
            return "Unknown component $name"
        }

        /** A Select's trigger text: the chosen option labels or the placeholder. */
        fun selectLabel(props: Props): String {
            val options = props.list("options").filterIsInstance<JsonValue.Obj>()
            val chosen: List<String> = when (val v = props["value"]) {
                is JsonValue.Arr -> v.v.map { it.displayText }
                null, JsonValue.Null -> emptyList()
                else -> listOf(v.displayText)
            }
            val labels = options.filter { chosen.contains(it["value"]?.displayText ?: "") }.map { it["label"]?.displayText ?: "" }
            if (labels.isEmpty()) return props.str("placeholder").ifEmpty { "Choose" }
            return labels.joinToString(", ")
        }

        /** `"2026-10-14"` → `"Oct 14, 2026"`. */
        fun dateLabel(value: String): String? = DateModel.label(value)

        /** CSS `text-transform`. */
        fun applyTransform(text: String, transform: String?): String = when (transform) {
            "uppercase" -> text.uppercase()
            "lowercase" -> text.lowercase()
            "capitalize" -> text.split(' ').joinToString(" ") { w -> w.replaceFirstChar { it.titlecase() } }
            else -> text
        }
    }
}
