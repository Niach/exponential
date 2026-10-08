package at.exponential.ui.theme

import androidx.compose.ui.graphics.Color
import at.exponential.ui.ffi.FfiVisual
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.px
import at.exponential.ui.json.str
import at.exponential.ui.primitives.parseHexColor

/** One shadow layer of a resolved `boxShadow` (dp). */
data class ShadowLayer(val x: Float, val y: Float, val blur: Float, val spread: Float, val color: Color)

/** A linear gradient painted over the background (CSS angle: 0 = up, 90 = right; stop offsets 0..1). */
data class GradientPaint(val angle: Float, val stops: List<Pair<Float, Color>>)

/** One paint-only transform function, applied about the box centre in source order. */
sealed class TransformOp {
    data class Translate(val x: Float, val y: Float) : TransformOp()
    data class Scale(val factor: Float) : TransformOp()
    data class Rotate(val degrees: Float) : TransformOp()
}

/** A node's keyframe animation (round 2 §2): the set, its resolved timing (JSON for the core) and the reduced-motion flag. */
data class AnimationSpec(val name: String, val timingJson: String, val reduced: Boolean, val infinite: Boolean)

/**
 * The painted box of a node: the core's RESOLVED `Visual` as Compose
 * values (dp). Colours are hex strings in the core; null = not set.
 */
data class PaintStyle(
    val background: Color? = null,
    val gradient: GradientPaint? = null,
    val color: Color? = null,
    val borderWidth: Float = 0f,
    /** Per-side widths `[top, right, bottom, left]` when a side differs (else [borderWidth] everywhere). */
    val borderWidths: List<Float>? = null,
    val borderColor: Color? = null,
    /** `solid | dashed | dotted`. */
    val borderStyle: String? = null,
    val radius: Float = 0f,
    /** Per-corner radii `[topLeft, topRight, bottomRight, bottomLeft]` when a corner is set. */
    val cornerRadii: List<Float>? = null,
    val opacity: Float? = null,
    val shadows: List<ShadowLayer> = emptyList(),
    val fontSize: Float? = null,
    val fontWeight: Int? = null,
    val lineHeight: Float? = null,
    val fontFamily: String? = null,
    /** PHYSICAL (`left | right | center | justify`). */
    val textAlign: String? = null,
    val letterSpacing: Float? = null,
    /** `none | underline | line-through`. */
    val textDecoration: String? = null,
    /** `none | uppercase | lowercase | capitalize`. */
    val textTransform: String? = null,
    /** `normal | italic`. */
    val fontStyle: String? = null,
    /** The recipe padding on a MEASURED leaf (the measurer counted it). */
    val paddingHorizontal: Float = 0f,
    val paddingVertical: Float = 0f,
    /** A leaf's padding `[top, right, bottom, left]` when known. */
    val padding: List<Float>? = null,
    val gap: Float = 0f,
    /** `native: true` on the part's recipe. */
    val native: Boolean = false,
    val overflowHidden: Boolean = false,
    val overflowScroll: Boolean = false,
    val overflowX: String? = null,
    val overflowY: String? = null,
    val transform: List<TransformOp> = emptyList(),
    /** Colour / opacity / transform changes animate over this many ms with [transitionEasing] (cubic bezier). */
    val transitionMs: Float? = null,
    val transitionEasing: List<Float>? = null,
    /** `visibility: hidden`: the box stays, nothing paints, out of the accessibility tree. */
    val visibilityHidden: Boolean = false,
    /** `pointerEvents: none`: presses pass through. */
    val pointerEventsNone: Boolean = false,
    val cursor: String? = null,
    /** A Chart's series colours. */
    val seriesColors: List<Color>? = null,
    /** Round 2: a leaf's direction (the bidi paragraph direction of its text). */
    val direction: String? = null,
    /** Round 2: `backdropBlur` in dp (painted as the background alone, see the README). */
    val backdropBlur: Float? = null,
    /** Round 2: the keyframe animation. */
    val animation: AnimationSpec? = null,
    /** Round 2: `position: sticky` (the offsets come with the pass). */
    val sticky: Boolean = false,
) {
    /** From the facade's resolved visual. */
    constructor(v: FfiVisual) : this(
        background = v.backgroundColor?.let(::parseHexColor),
        gradient = parseGradient(v.backgroundGradientJson),
        color = v.color?.let(::parseHexColor),
        borderWidth = v.borderWidth ?: 0f,
        borderWidths = v.borderWidths?.takeIf { it.size == 4 },
        borderColor = v.borderColor?.let(::parseHexColor),
        borderStyle = v.borderStyle,
        radius = v.borderRadius ?: 0f,
        cornerRadii = v.cornerRadii?.takeIf { it.size == 4 },
        opacity = v.opacity,
        shadows = parseShadows(v.boxShadowJson),
        fontSize = v.fontSize,
        fontWeight = v.fontWeight?.toInt(),
        lineHeight = v.lineHeight,
        fontFamily = v.fontFamily,
        textAlign = v.textAlign,
        letterSpacing = v.letterSpacing,
        textDecoration = v.textDecoration,
        textTransform = v.textTransform,
        fontStyle = v.fontStyle,
        paddingHorizontal = v.paddingHorizontal ?: 0f,
        paddingVertical = v.paddingVertical ?: 0f,
        padding = v.padding?.takeIf { it.size == 4 },
        gap = v.gap ?: 0f,
        native = v.native,
        overflowHidden = v.overflowHidden,
        overflowScroll = v.overflowScroll,
        overflowX = v.overflowX,
        overflowY = v.overflowY,
        transform = parseTransform(v.transformJson),
        transitionMs = v.transitionMs,
        transitionEasing = v.transitionEasing?.takeIf { it.size == 4 },
        visibilityHidden = v.visibilityHidden,
        pointerEventsNone = v.pointerEventsNone,
        cursor = v.cursor,
        seriesColors = v.seriesColors?.mapNotNull(::parseHexColor),
        direction = v.direction,
        backdropBlur = v.backdropBlur,
        animation = parseAnimation(v.animationJson),
        sticky = v.sticky,
    )

    /** The widths `[top, right, bottom, left]` the border paints with. */
    val sides: List<Float> get() = borderWidths ?: List(4) { borderWidth }

    /** Any border to paint? */
    val hasBorder: Boolean get() = borderColor != null && sides.any { it > 0f }

    /** The radii `[topLeft, topRight, bottomRight, bottomLeft]`. */
    val corners: List<Float> get() = cornerRadii ?: List(4) { radius }

    /** Any rounded corner? */
    val rounded: Boolean get() = corners.any { it > 0f }

    /** The inset of the content box inside a measured leaf (padding + border): (horizontal, vertical). */
    val inset: Pair<Float, Float> get() = (paddingHorizontal + borderWidth) to (paddingVertical + borderWidth)

    /** The inset of each side `[top, right, bottom, left]` (padding + border) of a measured leaf. */
    val insets: List<Float>
        get() {
            val b = sides
            val p = padding ?: listOf(paddingVertical, paddingHorizontal, paddingVertical, paddingHorizontal)
            return List(4) { p[it] + b[it] }
        }

    /** A right-to-left leaf. */
    val rtl: Boolean get() = direction == "rtl"

    companion object {
        /** The empty style. */
        val EMPTY = PaintStyle()

        private val SHADOW_FALLBACK = Color(0f, 0f, 0f, 0.2f)

        internal fun parseShadows(json: String?): List<ShadowLayer> {
            if (json == null) return emptyList()
            val layers = JsonValue.parse(json).array ?: return emptyList()
            return layers.mapNotNull { l ->
                val o = l.obj ?: return@mapNotNull null
                ShadowLayer(
                    x = (o["x"]?.number ?: 0.0).toFloat(),
                    y = (o["y"]?.number ?: 0.0).toFloat(),
                    blur = (o["blur"]?.number ?: 0.0).toFloat(),
                    spread = (o["spread"]?.number ?: 0.0).toFloat(),
                    color = o["color"]?.string?.let(::parseHexColor) ?: SHADOW_FALLBACK,
                )
            }
        }

        internal fun parseGradient(json: String?): GradientPaint? {
            val o = json?.let(JsonValue::parse) ?: return null
            val stops = o["stops"]?.array?.mapNotNull { s ->
                val c = s["color"]?.string?.let(::parseHexColor) ?: return@mapNotNull null
                (s["offset"]?.number ?: 0.0).toFloat() to c
            } ?: return null
            if (stops.isEmpty()) return null
            return GradientPaint((o["angle"]?.number ?: 180.0).toFloat(), stops)
        }

        internal fun parseTransform(json: String?): List<TransformOp> {
            val ops = json?.let(JsonValue::parse)?.array ?: return emptyList()
            return ops.mapNotNull { o ->
                fun n(k: String) = (o[k]?.number ?: 0.0).toFloat()
                when (o["op"]?.string) {
                    "translate" -> TransformOp.Translate(n("x"), n("y"))
                    "scale" -> TransformOp.Scale((o["factor"]?.number ?: 1.0).toFloat())
                    "rotate" -> TransformOp.Rotate(n("degrees"))
                    else -> null
                }
            }
        }

        internal fun parseAnimation(json: String?): AnimationSpec? {
            val o = json?.let(JsonValue::parse) ?: return null
            val name = o["name"]?.string ?: return null
            val timing = o["timing"] ?: return null
            return AnimationSpec(name, timing.json, o["reduced"]?.bool ?: false, timing["iterations"]?.string == "infinite")
        }
    }
}

/**
 * A part's resolved look: its [PaintStyle] plus the flat style map
 * (`width`, `height`, `native`…), the painter's view of the core's
 * `resolve_recipe`.
 */
data class PartStyle(val style: PaintStyle, val props: Props) {
    /** A length of the style map in dp. */
    fun px(key: String): Float? = props.px(key)

    /** `width` (dp). */
    val width: Float? get() = px("width")

    /** `height` (dp). */
    val height: Float? get() = px("height")

    /** `native` as the recipe says it: null when the recipe is silent. */
    val native: Boolean? get() = props["native"]?.bool

    /** The text colour. */
    val color: Color? get() = style.color

    /** The `fontFamily` NAME, when set. */
    val fontFamily: String? get() = props.str("fontFamily").ifEmpty { null }

    companion object {
        /** No look (geometry mode, a failed resolve). */
        val EMPTY = PartStyle(PaintStyle(), emptyMap())
    }
}
