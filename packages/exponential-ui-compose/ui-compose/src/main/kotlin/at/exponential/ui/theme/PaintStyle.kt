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

/**
 * The painted box of a node: the core's RESOLVED `Visual` as Compose
 * values (dp). Colours are hex strings in the core; null = not set.
 */
data class PaintStyle(
    val background: Color? = null,
    val color: Color? = null,
    val borderWidth: Float = 0f,
    val borderColor: Color? = null,
    val radius: Float = 0f,
    val opacity: Float? = null,
    val shadows: List<ShadowLayer> = emptyList(),
    val fontSize: Float? = null,
    val fontWeight: Int? = null,
    val lineHeight: Float? = null,
    val fontFamily: String? = null,
    val textAlign: String? = null,
    /** The recipe padding on a MEASURED leaf (the measurer counted it). */
    val paddingHorizontal: Float = 0f,
    val paddingVertical: Float = 0f,
    val gap: Float = 0f,
    /** `native: true` on the part's recipe. */
    val native: Boolean = false,
    val overflowHidden: Boolean = false,
    val overflowScroll: Boolean = false,
) {
    /** From the facade's resolved visual. */
    constructor(v: FfiVisual) : this(
        background = v.backgroundColor?.let(::parseHexColor),
        color = v.color?.let(::parseHexColor),
        borderWidth = v.borderWidth ?: 0f,
        borderColor = v.borderColor?.let(::parseHexColor),
        radius = v.borderRadius ?: 0f,
        opacity = v.opacity,
        shadows = parseShadows(v.boxShadowJson),
        fontSize = v.fontSize,
        fontWeight = v.fontWeight?.toInt(),
        lineHeight = v.lineHeight,
        fontFamily = v.fontFamily,
        textAlign = v.textAlign,
        paddingHorizontal = v.paddingHorizontal ?: 0f,
        paddingVertical = v.paddingVertical ?: 0f,
        gap = v.gap ?: 0f,
        native = v.native,
        overflowHidden = v.overflowHidden,
        overflowScroll = v.overflowScroll,
    )

    /** The inset of the content box inside a measured leaf (padding + border): (horizontal, vertical). */
    val inset: Pair<Float, Float> get() = (paddingHorizontal + borderWidth) to (paddingVertical + borderWidth)

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
