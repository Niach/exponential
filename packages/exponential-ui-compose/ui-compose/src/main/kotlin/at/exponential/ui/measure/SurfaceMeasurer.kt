package at.exponential.ui.measure

import androidx.compose.ui.geometry.Size
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
import at.exponential.ui.json.str
import at.exponential.ui.paint.ChartModel
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
) {
    /** From the facade's record. */
    constructor(c: FfiControlBox) : this(c.paddingHorizontal, c.paddingVertical, c.borderWidth, c.gap, c.minWidth, c.minHeight, c.width, c.height)

    /** The horizontal and vertical insets around the content. */
    val insets: Pair<Float, Float> get() = 2 * (paddingHorizontal + borderWidth) to 2 * (paddingVertical + borderWidth)

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
class LeafRequest(l: FfiLeaf) {
    val index: Int = l.index.toInt()
    val id: String = l.id
    val component: String = l.component
    val part: String? = l.part
    val props: Props = JsonValue.parse(l.propsJson).obj ?: emptyMap()
    val text: String = l.text
    val textStyle: ResolvedTextStyle = ResolvedTextStyle(l.textStyle)
    val control: ControlBox = ControlBox(l.control)
    val lines: Int? = l.lines?.toInt()
}

/**
 * The painter's [Measurer]: answers the core's BATCHED questions with the
 * Compose [TextShaper], the recipe boxes and the extension painters. Every
 * answer is the BORDER box of the control (the leaf's `ControlBox` padding
 * and border added around the content; fixed / minimum sizes win), the
 * gpui painter's rules. Called synchronously from inside `Surface.layout`
 * on the thread that runs the pass (the main thread).
 */
class SurfaceMeasurer(
    val theme: ThemeHandle?,
    val mode: Mode,
    val extensions: ExtensionRegistry,
    /** Extension node id → its kind (`FfiLeaf` carries no kind). */
    val kinds: Map<String, String>,
    val generation: Long,
    val shaper: TextShaper,
) : Measurer {
    /** How many answers this measurer gave (stats). */
    var calls = 0
        private set

    override fun measureId(): ULong = (MEASURE_IDENTITY + generation).toULong()

    override fun measureIntrinsics(leaves: List<FfiLeaf>): List<FfiIntrinsics> = leaves.map { raw ->
        val leaf = LeafRequest(raw)
        val maxC = answer(leaf, null)
        val minC = answer(leaf, 0f)
        FfiIntrinsics(min(minC.width, maxC.width), maxC.width, maxC.height)
    }

    override fun measureHeights(leaves: List<FfiLeaf>, requests: List<FfiHeightRequest>): List<Float> {
        val byIndex = HashMap<UInt, LeafRequest>()
        for (l in leaves) byIndex[l.index] = LeafRequest(l)
        return requests.map { r ->
            val leaf = byIndex[r.index] ?: return@map 0f
            answer(leaf, r.width).height
        }
    }

    // rules

    private fun part(component: String, part: String, props: Props, states: List<String> = emptyList()): PartStyle =
        theme?.part(component, part, props, states, mode) ?: PartStyle.EMPTY

    private fun spacing(name: String): Float = theme?.spacing(name) ?: GeometrySpacing.value(name)

    private fun control(name: String, fallback: Float): Float = theme?.control(name, fallback) ?: fallback

    private fun line(text: String, ts: ResolvedTextStyle): Float = shaper.maxContent(text, ts)

    /** Text plus fixed-width chrome beside it, one line. */
    private fun textWithChrome(text: String, ts: ResolvedTextStyle, chrome: Float, wrap: Float?): Size {
        val w = line(text, ts) + chrome
        val used = when {
            wrap == null -> w
            wrap <= 0f -> chrome
            else -> min(w, wrap)
        }
        return Size(used, ts.lineHeight)
    }

    /** The border box of `leaf` at `wrap` (null = max-content, 0 = min-content). */
    fun answer(leaf: LeafRequest, wrap: Float?): Size {
        calls += 1
        if (leaf.component == "Extension") {
            val kind = kinds[leaf.id] ?: return Size.Zero
            val painter = extensions.painter(kind) ?: return Size.Zero
            return painter.measure(ExtensionLeaf(leaf, theme, mode), wrap) ?: Size.Zero
        }
        return measureLeaf(leaf, wrap)
    }

    /** The per-component rules (everything but extensions). */
    fun measureLeaf(leaf: LeafRequest, wrap: Float?): Size {
        val c = leaf.control
        val inner = c.innerWrap(wrap)
        val props = leaf.props
        val ts = leaf.textStyle
        val p = leaf.part
        val content: Size = when (leaf.component) {
            "Text" -> when (p) {
                "tab" -> {
                    val icon = if (props["icon"]?.string != null) 16f + 4f else 0f
                    val count = props["count"]?.displayText ?: ""
                    val countW = if (count.isEmpty()) 0f else 4f + line(count, ts) + 12f
                    textWithChrome(props.str("text"), ts, icon + countW, inner)
                }
                "trigger" -> {
                    var text = props.str("text")
                    props["count"]?.let { text += " · ${it.displayText}" }
                    textWithChrome(text, ts, 16f + max(c.gap, 8f), inner)
                }
                "item" -> {
                    val icon = if (props["icon"]?.string != null) 16f + max(c.gap, 8f) else 0f
                    textWithChrome(props.str("text"), ts, icon, inner)
                }
                else -> shaper.measure(props.str("text"), ts, inner, leaf.lines)
            }
            "Markdown" -> markdown(leaf, inner)
            "Button", "Toggle" -> button(leaf)
            "Link" -> {
                val label = props.str("label").ifEmpty { props.str("href") }
                Size(line(label, ts), ts.lineHeight)
            }
            "Icon" -> Size(16f, 16f)
            "Avatar" -> Size(32f, 32f)
            "Image", "Video" -> media(props, inner, Size(320f, 180f))
            "AudioPlayer" -> {
                val track = if (props.str("title").isEmpty()) 0f else ts.lineHeight + spacing("xs")
                val w = when {
                    inner == null -> 300f
                    inner <= 0f -> 160f
                    else -> inner
                }
                Size(w, track + 40f)
            }
            "Spinner" -> Size(20f, 20f)
            "Ring" -> Size(32f, 32f)
            "Skeleton" -> skeleton(props, inner)
            "Chart" -> chart(leaf, inner)
            "Composer" -> composer(leaf, inner)
            "TreeGuides" -> Size(max((props.num("depth") ?: 0.0).toFloat(), 0f) * 16f, ts.lineHeight)
            "ToggleGroup" -> toggleGroup(leaf)
            "Unknown" -> {
                val label = part("Unknown", "label", props)
                val lts = ResolvedTextStyle(label.px("fontSize") ?: 12f, 400, label.px("lineHeight") ?: 16f, label.fontFamily)
                shaper.measure(unknownLabel(props, leaf.component), lts, inner, null)
            }
            "Box" -> if (p == "indicator") {
                val n = max((props.num("count") ?: 0.0).toFloat(), 0f)
                val dot = part("Carousel", "indicator", props).width ?: 8f
                val gap = spacing("xs")
                Size(n * dot + max(n - 1f, 0f) * gap, dot + spacing("sm"))
            } else Size.Zero
            "Input" -> when (p) {
                "field" -> Size(160f, ts.lineHeight)
                null -> Size(160f, 36f)
                else -> Size.Zero
            }
            "Textarea" -> when (p) {
                "field" -> Size(160f, max((props.num("rows") ?: 3.0).toFloat(), 1f) * ts.lineHeight)
                null -> Size(160f, 72f)
                else -> Size.Zero
            }
            "Select", "DatePicker" -> when (p) {
                "field" -> return trigger(leaf, leaf.component)
                null -> Size(160f, 36f)
                else -> Size.Zero
            }
            "Checkbox" -> if (p == "box" || p == null) Size(16f, 16f) else Size.Zero
            "Radio" -> if (p == "dot" || p == null) Size(16f, 16f) else Size.Zero
            "Switch" -> when (p) {
                "track" -> Size(32f, 20f)
                null -> Size(44f, 24f)
                else -> Size.Zero
            }
            "Slider" -> when (p) {
                "track" -> Size(160f, part("Slider", "thumb", props).height ?: control("slider", 16f))
                null -> Size(160f, 16f)
                else -> Size.Zero
            }
            else -> Size.Zero
        }
        return c.borderBox(content)
    }

    private fun markdown(leaf: LeafRequest, wrap: Float?): Size {
        val blocks = Markdown.parse(leaf.props.str("text"))
        val ts = leaf.textStyle
        val styles = MarkdownPainter.styles(theme, mode, ts, leaf.props)
        val md = MarkdownShaper(shaper, theme?.monoFamily)
        val out = when {
            wrap == null -> {
                val w = Markdown.maxContentWidth(blocks, styles, md)
                Size(w, Markdown.layout(blocks, styles, w, md).height)
            }
            wrap <= 0f -> {
                val mw = Markdown.minContentWidth(blocks, styles, md)
                Size(mw, Markdown.layout(blocks, styles, mw, md).height)
            }
            else -> Size(wrap, Markdown.layout(blocks, styles, wrap, md).height)
        }
        return Size(out.width, max(out.height, if (blocks.isEmpty()) 0f else ts.lineHeight))
    }

    private fun button(leaf: LeafRequest): Size {
        val props = leaf.props
        val ts = leaf.textStyle
        val label = props.str("label")
        val hasIcon = props.str("icon").isNotEmpty() || props.flag("loading")
        val iconOnly = props.str("size") == "icon"
        val icon = if (hasIcon || iconOnly) (part(leaf.component, "icon", props).width ?: control("iconSm", 16f)) else 0f
        val labelW = if (iconOnly || label.isEmpty()) 0f else line(label, ts)
        val gap = if (hasIcon && labelW > 0f) leaf.control.gap else 0f
        val w = (if (hasIcon || iconOnly) icon else 0f) + gap + labelW
        return Size(w, max(ts.lineHeight, if (hasIcon) icon else 0f))
    }

    private fun media(props: Props, wrap: Float?, defaultSize: Size): Size {
        val ratio = props.num("aspectRatio")?.toFloat()?.takeIf { it > 0f } ?: (defaultSize.width / defaultSize.height)
        val fixedW = props.px("width")
        val fixedH = props.px("height")
        val w = when {
            fixedW != null -> fixedW
            wrap == null -> defaultSize.width
            wrap <= 0f -> 0f
            else -> min(wrap, max(defaultSize.width, wrap))
        }
        val h = fixedH ?: if (w > 0f) w / ratio else defaultSize.width / ratio
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

    private fun chart(leaf: LeafRequest, wrap: Float?): Size {
        val props = leaf.props
        val w = when {
            wrap == null -> 320f
            wrap <= 0f -> 0f
            else -> wrap
        }
        var h = (props.num("height") ?: 200.0).toFloat()
        val gap = spacing("xs")
        if (props.str("title").isNotEmpty()) h += leaf.textStyle.lineHeight + gap
        if (ChartModel.legend(props).isNotEmpty()) h += (part("Chart", "legend", props).px("lineHeight") ?: 16f) + gap
        return Size(w, h)
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
        var text = props.str("value").ifEmpty { props.str("placeholder") }
        if (text.isEmpty()) text = "Message"
        val fieldTs = ResolvedTextStyle(fs, ts.fontWeight, lh, ts.fontFamily)
        val w = when {
            wrap == null -> 320f
            wrap <= 0f -> 120f
            else -> wrap
        }
        val th = shaper.measure(text, fieldTs, max(w, 1f), null).height
        return Size(w, min(max(th, minH), 200f) + gap + send)
    }

    private fun toggleGroup(leaf: LeafRequest): Size {
        val props = leaf.props
        val items = props.list("items")
        val item = part("ToggleGroup", "item", props)
        val pad = item.px("paddingHorizontal") ?: item.px("padding") ?: 12f
        val border = item.px("borderWidth") ?: 0f
        val h = item.height ?: 36f
        val ts = ResolvedTextStyle(item.px("fontSize") ?: leaf.textStyle.fontSize, (item.props["fontWeight"]?.number ?: 500.0).toInt(), leaf.textStyle.lineHeight, item.fontFamily)
        var w = 0f
        for ((i, it) in items.withIndex()) {
            val label = it["label"]?.displayText ?: ""
            val hasIcon = it["icon"]?.string != null
            val lw = if (label.isEmpty()) 0f else line(label, ts)
            val iw = if (hasIcon) 16f else 0f
            val innerGap = if (hasIcon && lw > 0f) 6f else 0f
            w += 2 * (pad + border) + iw + innerGap + lw
            if (i > 0) w += leaf.control.gap
        }
        return Size(w, h)
    }

    /** Select / DatePicker `.field`: the TRIGGER recipe (the field has none). */
    private fun trigger(leaf: LeafRequest, component: String): Size {
        val props = leaf.props
        val t = part(component, "trigger", props)
        val fs = t.px("fontSize") ?: leaf.textStyle.fontSize
        val lh = t.px("lineHeight") ?: leaf.textStyle.lineHeight
        val pad = t.px("paddingHorizontal") ?: t.px("padding") ?: 12f
        val border = t.px("borderWidth") ?: 0f
        val h = t.height ?: (lh + 16f)
        val ts = ResolvedTextStyle(fs, 400, lh, t.fontFamily)
        val label = if (component == "Select") selectLabel(props)
        else dateLabel(props.str("value")) ?: props.str("placeholder").ifEmpty { "Pick a date" }
        val w = max(line(label, ts) + 2 * (pad + border) + 16f + spacing("sm"), 160f)
        return Size(w, h)
    }

    companion object {
        /** The identity the core keys its memo on ("Compose"; a font change = a new generation). */
        const val MEASURE_IDENTITY: Long = 0x436F6D706F7365L

        /** The text an `Unknown` placeholder shows. */
        fun unknownLabel(props: Props, component: String): String {
            val name = props.str("component").ifEmpty { component }
            return "Unknown component $name"
        }

        /** A Select's trigger text: the chosen option labels or the placeholder. */
        fun selectLabel(props: Props): String {
            val options = props.list("options")
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
    }
}
