package at.exponential.ui.compose

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import at.exponential.ui.ExponentialUICatalog
import at.exponential.ui.ffi.zoomJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.fire
import at.exponential.ui.primitives.ZoomBounds
import at.exponential.ui.primitives.ZoomControls
import at.exponential.ui.primitives.ZoomFrame
import at.exponential.ui.primitives.ZoomIcons
import at.exponential.ui.primitives.ZoomInitial
import at.exponential.ui.primitives.ZoomLabels
import at.exponential.ui.primitives.ZoomLooks
import at.exponential.ui.primitives.ZoomMath
import at.exponential.ui.primitives.ZoomMenuItem
import at.exponential.ui.primitives.ZoomMotion
import at.exponential.ui.primitives.ZoomPanRoom
import at.exponential.ui.primitives.ZoomPart
import at.exponential.ui.primitives.ZoomPartLook
import at.exponential.ui.primitives.ZoomPartState
import at.exponential.ui.primitives.ZoomPillAppearance
import at.exponential.ui.primitives.ZoomPillMetrics
import at.exponential.ui.primitives.ZoomRest
import at.exponential.ui.primitives.ZoomView
import at.exponential.ui.primitives.ZoomableImageView
import at.exponential.ui.primitives.rememberZoomableImageState
import at.exponential.ui.theme.PartStyle
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.abs
import kotlin.math.max

// EXP-1586: the `ZoomableImage` native (catalog/style.json
// `zoomableImage`, whose $comment is the spec). The input routing and THE
// pill are the primitives' [ZoomableImageView]; every number is the eUI
// core's (`zoomJson`, replayed against fixtures/zoom.json by the tests);
// the looks are the theme's `ZoomableImage/*` and `Menu/*` recipes.

/**
 * THE zoom maths through the eUI core (`zoomJson`, the Rust `zoom`
 * module): one request per call, named like its fixtures/zoom.json group.
 * Hand it to the primitives' [ZoomableImageView] (the Exponential app does
 * for its lightbox). A request the core refuses (a NaN, a bad frame) leaves
 * the view unchanged.
 */
object CoreZoomMath : ZoomMath {
    private fun num(v: Float): JsonValue = JsonValue.Num(v.toDouble())

    private fun size(w: Float, h: Float): JsonValue = JsonValue.Obj(linkedMapOf("width" to num(w), "height" to num(h)))

    private fun point(p: Offset): JsonValue = JsonValue.Obj(linkedMapOf("x" to num(p.x), "y" to num(p.y)))

    /** The core's frame `{natural, viewport, initial, maxScale?}`. */
    fun frameJson(f: ZoomFrame): JsonValue = JsonValue.Obj(
        linkedMapOf(
            "natural" to size(f.naturalWidth, f.naturalHeight),
            "viewport" to size(f.viewportWidth, f.viewportHeight),
            "initial" to JsonValue.Str(f.initial.wire),
        ).apply { f.maxScale?.let { put("maxScale", num(it)) } },
    )

    /** A view `{scale, x, y}`. */
    fun viewJson(v: ZoomView): JsonValue = JsonValue.Obj(linkedMapOf("scale" to num(v.scale), "x" to num(v.x), "y" to num(v.y)))

    /** The request `{op, frame?, …fields}` as the core reads it. */
    fun request(op: String, frame: JsonValue?, fields: Map<String, JsonValue> = emptyMap()): String =
        JsonValue.Obj(
            linkedMapOf<String, JsonValue>("op" to JsonValue.Str(op)).apply {
                if (frame != null) put("frame", frame)
                putAll(fields)
            },
        ).orderedJson

    /** The core's answer to [request] (null when it refused it). */
    fun call(request: String): JsonValue? = try {
        val v = JsonValue.parse(zoomJson(request))
        if (v is JsonValue.Obj && v["error"] != null) null else v
    } catch (_: Throwable) {
        null
    }

    private fun op(op: String, frame: ZoomFrame?, fields: Map<String, JsonValue>): JsonValue? = call(request(op, frame?.let(::frameJson), fields))

    private fun parseView(v: JsonValue?): ZoomView? {
        val s = v?.get("scale")?.number ?: return null
        return ZoomView(s.toFloat(), (v["x"]?.number ?: 0.0).toFloat(), (v["y"]?.number ?: 0.0).toFloat())
    }

    private fun viewOp(op: String, from: ZoomView, frame: ZoomFrame, fields: Map<String, JsonValue> = emptyMap()): ZoomView =
        parseView(op(op, frame, linkedMapOf("from" to viewJson(from)).apply { putAll(fields) })) ?: from

    override fun rest(frame: ZoomFrame): ZoomRest {
        val r = op("rest", frame, emptyMap())
        val limits = r?.get("limits")
        val fit = (r?.get("fit")?.number ?: 1.0).toFloat()
        val view = parseView(r?.get("view")) ?: ZoomView(fit, 0f, 0f)
        return ZoomRest(
            fit = fit,
            min = (limits?.get("min")?.number ?: fit.toDouble()).toFloat(),
            max = (limits?.get("max")?.number ?: fit.toDouble()).toFloat(),
            restScale = (r?.get("restScale")?.number ?: view.scale.toDouble()).toFloat(),
            view = view,
            tall = r?.get("tall")?.bool ?: false,
        )
    }

    override fun clamp(view: ZoomView, frame: ZoomFrame): ZoomView = parseView(op("clamp", frame, mapOf("view" to viewJson(view)))) ?: view

    override fun zoomTo(view: ZoomView, scale: Float, anchor: Offset, frame: ZoomFrame): ZoomView =
        viewOp("zoomTo", view, frame, mapOf("scale" to num(scale), "anchor" to point(anchor)))

    override fun wheel(view: ZoomView, deltaY: Float, lines: Boolean, anchor: Offset, frame: ZoomFrame): ZoomView =
        viewOp("wheel", view, frame, mapOf("deltaY" to num(deltaY), "mode" to JsonValue.Str(if (lines) "line" else "pixel"), "anchor" to point(anchor)))

    override fun scroll(view: ZoomView, deltaX: Float, deltaY: Float, frame: ZoomFrame): ZoomView =
        viewOp("scroll", view, frame, mapOf("deltaX" to num(deltaX), "deltaY" to num(deltaY)))

    override fun step(view: ZoomView, direction: Int, frame: ZoomFrame): ZoomView =
        viewOp("step", view, frame, mapOf("direction" to JsonValue.Num(if (direction < 0) -1.0 else 1.0)))

    override fun doubleTap(view: ZoomView, point: Offset, frame: ZoomFrame): ZoomView = viewOp("doubleTap", view, frame, mapOf("point" to point(point)))

    override fun key(view: ZoomView, key: String, frame: ZoomFrame): ZoomView? =
        parseView(op("keys", frame, mapOf("from" to viewJson(view), "key" to JsonValue.Str(key))))

    override fun drag(start: ZoomView, dx: Float, dy: Float, frame: ZoomFrame): ZoomView = viewOp("drag", start, frame, mapOf("dx" to num(dx), "dy" to num(dy)))

    override fun pinch(start: ZoomView, factor: Float, anchor: Offset, frame: ZoomFrame): ZoomView =
        viewOp("pinch", start, frame, mapOf("factor" to num(factor), "anchor" to point(anchor)))

    override fun momentum(view: ZoomView, vx: Float, vy: Float, t: Float, frame: ZoomFrame): ZoomView =
        viewOp("momentum", view, frame, mapOf("vx" to num(vx), "vy" to num(vy), "t" to num(t)))

    override fun momentumActive(vx: Float, vy: Float, t: Float): Boolean =
        op("momentumActive", null, mapOf("vx" to num(vx), "vy" to num(vy), "t" to num(t)))?.bool ?: false

    override fun panRoom(view: ZoomView, frame: ZoomFrame): ZoomPanRoom {
        val r = op("panRoom", frame, mapOf("from" to viewJson(view)))
        return ZoomPanRoom(r?.get("x")?.bool ?: false, r?.get("y")?.bool ?: false)
    }

    override fun resize(view: ZoomView, from: Size, frame: ZoomFrame): ZoomView =
        viewOp("resize", view, frame, mapOf("fromViewport" to size(from.width, from.height)))

    override fun percent(scale: Float): String = op("percent", null, mapOf("scale" to num(scale)))?.string ?: ""

    override fun menu(view: ZoomView, frame: ZoomFrame, fitLabel: String): List<ZoomMenuItem> =
        op("menu", frame, mapOf("from" to viewJson(view), "fitLabel" to JsonValue.Str(fitLabel)))?.array?.map {
            ZoomMenuItem(it["scale"]?.takeIf { s -> s is JsonValue.Num }?.number?.toFloat(), it["label"]?.string ?: "", it["checked"]?.bool ?: false)
        } ?: emptyList()

    override fun bounds(view: ZoomView, frame: ZoomFrame): ZoomBounds {
        val r = op("bounds", frame, mapOf("from" to viewJson(view)))
        return ZoomBounds(r?.get("atMin")?.bool ?: true, r?.get("atMax")?.bool ?: true)
    }

    /** A picture taller than 1 / `tallAspect` its width (the core's `rest.tall`). */
    fun isTall(naturalWidth: Float, naturalHeight: Float): Boolean =
        naturalWidth > 0f && naturalHeight > 0f && rest(ZoomFrame(naturalWidth, naturalHeight, 1f, 1f)).tall
}

/**
 * style.json `zoomableImage.pill` + `autoHideMs` (the core keeps them in
 * its spec; the tests hold these to style.json).
 */
internal object ZoomSpec {
    const val PILL_HEIGHT = 36f
    const val PILL_INSET = 16f
    const val PILL_PADDING = 4f
    const val PILL_GAP = 2f
    const val PERCENT_MIN_WIDTH = 56f
    const val DIVIDER_HEIGHT = 16f
    const val AUTO_HIDE_MS = 2000L
}

/**
 * The box the measurer answers (style.json `zoomableImage` THE BOX; the
 * core lays it out `width: 100%` + `aspectRatio`, `zoomable_aspect_ratio`):
 * as wide as offered ([wrap]; max-content `mediaIntrinsicWidth`,
 * min-content 0), as tall as width × natural height / width (no taller
 * than wide for a TALL picture), else 16:9.
 */
internal fun zoomableBox(props: Props, wrap: Float?, intrinsicWidth: Float, aspect: Float): Size {
    val nw = props.num("naturalWidth")?.toFloat()?.takeIf { it > 0f }
    val nh = props.num("naturalHeight")?.toFloat()?.takeIf { it > 0f }
    val w = when {
        wrap == null -> intrinsicWidth
        wrap <= 0f -> 0f
        else -> wrap
    }
    val h = if (nw != null && nh != null) {
        val r = w * nh / nw
        if (CoreZoomMath.isTall(nw, nh)) minOf(r, w) else r
    } else {
        w / aspect
    }
    return Size(w, h)
}

/** The icon of a built-in slot (`ZoomableImage.zoomIn` → `ui-add`). */
internal fun builtinIcon(slot: String, fallback: String): String =
    ExponentialUICatalog.builtinIconSlots.indexOf(slot).takeIf { it >= 0 }?.let { ExponentialUICatalog.builtinIconNames.getOrNull(it) } ?: fallback

/** A recipe part as the primitive's look (dp, the content colour falling back to [ink]). */
internal fun PartStyle.zoomLook(ink: Color, base: ZoomPartLook = ZoomPartLook()): ZoomPartLook {
    val s = style
    val pad = s.padding
    return ZoomPartLook(
        background = s.background?.takeIf { it.alpha > 0f },
        content = color ?: ink,
        borderColor = s.borderColor,
        borderWidth = s.borderWidth.dp,
        radius = s.radius.dp,
        elevation = (s.shadows.maxOfOrNull { it.blur } ?: 0f).let { (it / 2f).coerceIn(0f, 16f) }.dp,
        opacity = s.opacity ?: 1f,
        width = width?.dp ?: base.width,
        height = height?.dp ?: base.height,
        paddingHorizontal = (pad?.getOrNull(1) ?: s.paddingHorizontal).dp,
        padding = (pad?.getOrNull(0) ?: s.paddingVertical).dp,
        gap = s.gap.dp,
        fontSize = s.fontSize?.sp ?: base.fontSize,
        lineHeight = s.lineHeight?.sp ?: base.lineHeight,
        fontWeight = s.fontWeight?.let { FontWeight(it) } ?: base.fontWeight,
    )
}

/** The pill + Menu looks from the theme: `ZoomableImage/<part>` keyed on `controls`, the Menu's `content` / `item` / `check`. */
internal fun zoomLooks(model: SurfaceModel, controls: String, ink: Color): ZoomLooks {
    val props: Props = mapOf("controls" to JsonValue.Str(controls))
    val menuProps: Props = mapOf("openOn" to JsonValue.Str("press"))
    return ZoomLooks { part, s ->
        val states = buildList {
            if (s.hovered) add("hover")
            if (s.pressed) add("pressed")
            if (s.disabled) add("disabled")
            if (s.selected) add("selected")
        }
        when (part) {
            ZoomPart.Pill -> model.part("ZoomableImage", "pill", props).zoomLook(ink)
            ZoomPart.ZoomOut -> model.part("ZoomableImage", "zoomOut", props, states).zoomLook(ink, ZoomPartLook(width = 28.dp, height = 28.dp))
            ZoomPart.Percent -> model.part("ZoomableImage", "percent", props, states).zoomLook(ink, ZoomPartLook(height = 28.dp))
            ZoomPart.ZoomIn -> model.part("ZoomableImage", "zoomIn", props, states).zoomLook(ink, ZoomPartLook(width = 28.dp, height = 28.dp))
            ZoomPart.Divider -> model.part("ZoomableImage", "divider", props).zoomLook(ink, ZoomPartLook(width = 1.dp, height = ZoomSpec.DIVIDER_HEIGHT.dp))
            ZoomPart.Fit -> model.part("ZoomableImage", "fit", props, states).zoomLook(ink, ZoomPartLook(width = 28.dp, height = 28.dp))
            ZoomPart.Menu -> model.part("Menu", "content", menuProps).zoomLook(ink)
            ZoomPart.MenuItem -> model.part("Menu", "item", menuProps, states).zoomLook(ink, ZoomPartLook(height = 32.dp, fontWeight = FontWeight.Normal))
            ZoomPart.MenuCheck -> model.part("Menu", "check", menuProps).zoomLook(ink, ZoomPartLook(width = 16.dp, height = 16.dp))
        }
    }
}

/**
 * `ZoomableImage`: the policed picture (its natural size from the props,
 * else the decoded bitmap's) in the primitives' [ZoomableImageView] on the
 * core's maths; the theme's pill with the host's built-in icons and the
 * surface's strings; `zoom {scale, atFit}` after each settled zoom change,
 * `tap` on a single tap; a `scale` the author (or a binding) sets zooms
 * there about the centre. While the picture loads (or fails) the picture's
 * area is tinted with the image glyph and the alt text over it, and the
 * pill is not drawn (nothing to zoom yet).
 */
@Composable
internal fun ZoomableImageLeaf(cx: LeafContext) {
    val model = cx.model
    val props = cx.props
    val src = props.str("src")
    val alt = props.str("alt")
    val picture = rememberLeafImage(cx, src)
    val nw = props.num("naturalWidth")?.toFloat()?.takeIf { it > 0f } ?: picture?.width?.toFloat()
    val nh = props.num("naturalHeight")?.toFloat()?.takeIf { it > 0f } ?: picture?.height?.toFloat()
    val natural = if (nw != null && nh != null) Size(nw, nh) else Size(max(cx.size.width, 1f), max(cx.size.height, 1f))
    val controls = ZoomControls.of(props.str("controls").ifEmpty { null })
    val initial = ZoomInitial.of(props.str("initial").ifEmpty { null })
    val maxScale = props.num("maxScale")?.toFloat()?.takeIf { it > 0f }
    val state = rememberZoomableImageState(CoreZoomMath)
    val ink = cx.ink
    val muted = cx.themeColor("mutedForeground") ?: ink
    val pillPart = model.part("ZoomableImage", "pill", mapOf("controls" to JsonValue.Str(controls.wire)))
    val appearance = remember(model.theme, model.mode, controls, ink, model.settings) {
        ZoomPillAppearance(
            looks = zoomLooks(model, controls.wire, ink),
            icons = ZoomIcons(
                zoomOut = { c, s -> IconView(builtinIcon("ZoomableImage.zoomOut", "ui-minus"), s.value, c, model) },
                zoomIn = { c, s -> IconView(builtinIcon("ZoomableImage.zoomIn", "ui-add"), s.value, c, model) },
                fit = { c, s -> IconView(builtinIcon("ZoomableImage.fit", "ui-fullscreen"), s.value, c, model) },
                check = { c, s -> IconView(builtinIcon("Menu.check", "ui-check"), s.value, c, model) },
            ),
            labels = ZoomLabels(
                zoomIn = model.string("zoomIn"),
                zoomOut = model.string("zoomOut"),
                zoomToFit = model.string("zoomToFit"),
                zoomLevel = model.string("zoomLevel"),
                zoomFit = model.string("zoomFit"),
            ),
            metrics = ZoomPillMetrics(
                height = ZoomSpec.PILL_HEIGHT.dp,
                inset = ZoomSpec.PILL_INSET.dp,
                padding = (pillPart.style.padding?.firstOrNull() ?: ZoomSpec.PILL_PADDING).dp,
                gap = (pillPart.style.gap.takeIf { it > 0f } ?: ZoomSpec.PILL_GAP).dp,
                percentMinWidth = ZoomSpec.PERCENT_MIN_WIDTH.dp,
                dividerHeight = ZoomSpec.DIVIDER_HEIGHT.dp,
                autoHideMs = ZoomSpec.AUTO_HIDE_MS,
            ),
        )
    }
    val motion = remember(model.theme, model.reducedMotion) {
        val e = model.theme?.ease("standard") ?: listOf(0.2f, 0f, 0f, 1f)
        ZoomMotion(tween((model.theme?.motion("standard") ?: 180f).toInt(), easing = CubicBezierEasing(e[0], e[1], e[2], e[3])), model.reducedMotion)
    }
    val radius = model.part("ZoomableImage", "image", mapOf("controls" to JsonValue.Str(controls.wire))).style.radius
    // `scale` set by the author / its binding: zoom there about the centre
    // (the first one is where the picture opens, without animation).
    val external = props.num("scale")?.toFloat()?.takeIf { it > 0f }
    var applied by remember(cx.node.id) { mutableStateOf(false) }
    val laidOut = state.frame != null
    LaunchedEffect(external, laidOut) {
        if (state.frame == null) return@LaunchedEffect
        if (external != null && abs(external - state.scale) >= 1e-4f) state.zoomTo(external, animated = applied, report = false)
        applied = true
    }
    val index = cx.index
    ZoomableImageView(
        naturalSize = natural,
        state = state,
        modifier = Modifier.fillMaxSize(),
        initial = initial,
        maxScale = maxScale,
        // style.json zoomableImage: while the picture has not loaded (pending
        // or failed) there is nothing to zoom, so the pill is not drawn.
        controls = if (picture == null) ZoomControls.None else controls,
        contentKey = src,
        contentDescription = alt.ifEmpty { null },
        onZoom = { s, rest ->
            model.fire(index, "zoom", JsonValue.Obj(linkedMapOf("scale" to JsonValue.Num(s.toDouble()), "atFit" to JsonValue.Bool(rest))))
        },
        onTap = { model.fire(index, "tap") },
        appearance = appearance,
        motion = motion,
        pictureRadius = radius.dp,
        overlay = if (picture != null) null else ({
            Column(
                Modifier.fillMaxSize().clearAndSetSemantics {},
                verticalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterVertically),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                GlyphView(Glyph.Image, 20f, muted.copy(alpha = muted.alpha * 0.6f))
                if (alt.isNotEmpty()) BasicText(
                    alt,
                    style = cx.composeTextStyle(ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily), color = muted.copy(alpha = muted.alpha * 0.6f)),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }),
    ) {
        if (picture != null) {
            Image(picture, null, Modifier.fillMaxSize(), contentScale = ContentScale.FillBounds)
        } else {
            androidx.compose.foundation.layout.Box(Modifier.fillMaxSize().background(muted.copy(alpha = muted.alpha * 0.08f)))
        }
    }
}
