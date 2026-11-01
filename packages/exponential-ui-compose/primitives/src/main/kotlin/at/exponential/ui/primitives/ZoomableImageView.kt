package at.exponential.ui.primitives

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.focusable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsHoveredAsState
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isAltPressed
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.isCtrlPressed
import androidx.compose.ui.input.pointer.isMetaPressed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupPositionProvider
import androidx.compose.ui.window.PopupProperties
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

// EXP-1586: the ZoomableImage primitive (catalog/style.json
// `zoomableImage`, whose $comment is the spec). The eUI painter draws the
// `ZoomableImage` native with it, and an app embeds it directly (the
// Exponential app's lightbox). Every number (fit, rest, clamps, steps, the
// rubber band, pinch, wheel, momentum, the menu, the percent label) comes
// from a [ZoomMath]: `:ui-compose`'s `CoreZoomMath` answers through the
// eUI core (`zoomJson`, the fixtures/zoom.json replay), so a gesture lands
// on the same view on every platform. This file only routes input.

/**
 * A view of the picture: [scale] = displayed dp per image pixel (1 = 100%),
 * [x] / [y] = the picture's top-left in the viewport (dp).
 */
@Immutable
data class ZoomView(val scale: Float, val x: Float, val y: Float)

/** The `initial` prop: the scale the picture opens (and rests) at. */
enum class ZoomInitial(val wire: String) {
    /** Fit, or fitWidth when the picture is TALL (w/h < 1/3). */
    Auto("auto"),
    /** The whole picture, never upscaled. */
    Fit("fit"),
    /** The width, never upscaled (a tall picture starts at its top). */
    FitWidth("fitWidth"),
    /** 100%. */
    Actual("actual");

    companion object {
        /** The value named [wire]; anything else = [Auto]. */
        fun of(wire: String?): ZoomInitial = entries.firstOrNull { it.wire == wire } ?: Auto
    }
}

/** The `controls` prop: when THE pill draws. */
enum class ZoomControls(val wire: String) {
    /** On appear, hidden after `autoHideMs` idle (kept while hovered or focused; a touch tap toggles it). */
    Auto("auto"),
    /** Always. */
    Always("always"),
    /** Never. */
    None("none");

    companion object {
        /** The value named [wire]; anything else = [Auto]. */
        fun of(wire: String?): ZoomControls = entries.firstOrNull { it.wire == wire } ?: Auto
    }
}

/**
 * The picture and its box: [naturalWidth] × [naturalHeight] image pixels
 * in a [viewportWidth] × [viewportHeight] dp viewport.
 */
@Immutable
data class ZoomFrame(
    val naturalWidth: Float,
    val naturalHeight: Float,
    val viewportWidth: Float,
    val viewportHeight: Float,
    val initial: ZoomInitial = ZoomInitial.Auto,
    /** null = the spec's (8). */
    val maxScale: Float? = null,
) {
    /** The viewport as a [Size] (dp). */
    val viewport: Size get() = Size(viewportWidth, viewportHeight)
}

/** The core's `rest` answer: the fit scale, the limits, the rest scale and view, TALL. */
@Immutable
data class ZoomRest(val fit: Float, val min: Float, val max: Float, val restScale: Float, val view: ZoomView, val tall: Boolean)

/** The pill's disabled ends: − at the min, + at the max. */
@Immutable
data class ZoomBounds(val atMin: Boolean, val atMax: Boolean)

/** The axes the picture can pan on (an axis without room hands a swipe to the host). */
@Immutable
data class ZoomPanRoom(val x: Boolean, val y: Boolean)

/** One row of the percent button's Menu: [scale] null = Fit (the rest view). */
@Immutable
data class ZoomMenuItem(val scale: Float?, val label: String, val checked: Boolean)

/**
 * THE zoom maths, one function per fixtures/zoom.json group. Points and
 * offsets are viewport dp; velocities dp per ms; [momentum]'s `t` ms after
 * the release. `:ui-compose`'s `CoreZoomMath` is the implementation (the eUI
 * core); never re-implement it.
 */
interface ZoomMath {
    fun rest(frame: ZoomFrame): ZoomRest
    fun clamp(view: ZoomView, frame: ZoomFrame): ZoomView
    fun zoomTo(view: ZoomView, scale: Float, anchor: Offset, frame: ZoomFrame): ZoomView
    /** ⌘/ctrl + wheel: [deltaY] in px ([lines] = in lines). */
    fun wheel(view: ZoomView, deltaY: Float, lines: Boolean, anchor: Offset, frame: ZoomFrame): ZoomView
    /** A plain wheel: pans by the deltas. */
    fun scroll(view: ZoomView, deltaX: Float, deltaY: Float, frame: ZoomFrame): ZoomView
    /** + ([direction] 1) / − (−1) about the viewport centre. */
    fun step(view: ZoomView, direction: Int, frame: ZoomFrame): ZoomView
    fun doubleTap(view: ZoomView, point: Offset, frame: ZoomFrame): ZoomView
    /** A key (`+ = - 0 1`); null = not a zoom key. */
    fun key(view: ZoomView, key: String, frame: ZoomFrame): ZoomView?
    /** A held drag from [start] (rubber-banded past the edges). */
    fun drag(start: ZoomView, dx: Float, dy: Float, frame: ZoomFrame): ZoomView
    /** A held pinch from [start] by [factor] about [anchor] (rubber-banded past the limits). */
    fun pinch(start: ZoomView, factor: Float, anchor: Offset, frame: ZoomFrame): ZoomView
    fun momentum(view: ZoomView, vx: Float, vy: Float, t: Float, frame: ZoomFrame): ZoomView
    fun momentumActive(vx: Float, vy: Float, t: Float): Boolean
    fun panRoom(view: ZoomView, frame: ZoomFrame): ZoomPanRoom
    /** A new viewport (from [from]): at rest = the new rest view, else the same scale about the centre. */
    fun resize(view: ZoomView, from: Size, frame: ZoomFrame): ZoomView
    fun percent(scale: Float): String
    fun menu(view: ZoomView, frame: ZoomFrame, fitLabel: String): List<ZoomMenuItem>
    fun bounds(view: ZoomView, frame: ZoomFrame): ZoomBounds
}

/**
 * The live view of a [ZoomableImageView]: read [scale] / [atRest] /
 * [bounds]; drive it with [zoomTo], [step], [reset]. Animations run on
 * the view's composition (until it is composed, changes snap).
 */
@Stable
class ZoomableImageState(val math: ZoomMath) {
    /** The current view (while a gesture is held: the rubber-banded one). */
    var view: ZoomView by mutableStateOf(ZoomView(1f, 0f, 0f))
        private set

    /** The picture and viewport (null until the view is laid out). */
    var frame: ZoomFrame? by mutableStateOf(null)
        private set

    /** The core's rest answer for [frame]. */
    var rest: ZoomRest? by mutableStateOf(null)
        private set

    /** 1 = 100%. */
    val scale: Float get() = view.scale

    /** At the rest (opening) scale. */
    val atRest: Boolean get() = rest?.let { abs(view.scale - it.restScale) < 1e-4f } ?: true

    /** The pill's disabled ends. */
    val bounds: ZoomBounds get() = frame?.let { math.bounds(view, it) } ?: ZoomBounds(atMin = true, atMax = true)

    /** The pill's label (`round(scale × 100)%`). */
    val percent: String get() = math.percent(view.scale)

    internal var scope: CoroutineScope? = null
    internal var spec: AnimationSpec<Float> = tween(180, easing = STANDARD_EASE)
    internal var reduced: Boolean = false
    internal var onSettled: ((ZoomView, Boolean) -> Unit)? = null

    /** Bumped on every settled zoom (the pill's auto-hide timer restarts). */
    internal var activity by mutableIntStateOf(0)
    private var job: Job? = null
    private var settledScale: Float? = null

    /**
     * A new frame. [reset] (a new picture) or the first frame = the rest
     * view without animation; a new viewport = the core's resize; a new
     * initial / max = the clamped view.
     */
    fun setFrame(next: ZoomFrame, reset: Boolean = false) {
        val prev = frame
        if (prev == next && !reset) return
        frame = next
        rest = math.rest(next)
        val newPicture = prev == null || reset || prev.naturalWidth != next.naturalWidth || prev.naturalHeight != next.naturalHeight
        cancel()
        view = when {
            newPicture -> rest!!.view
            prev!!.viewport != next.viewport -> math.resize(view, prev.viewport, next)
            else -> math.clamp(view, next)
        }
        // A frame change is not a user's zoom: no `zoom` event.
        settledScale = view.scale
    }

    /** Stop any animation or momentum (a new touch). */
    fun cancel() {
        job?.cancel()
        job = null
    }

    internal fun snapTo(next: ZoomView) {
        view = next
    }

    /**
     * Go to [target]: animated on the theme's standard motion unless
     * [animated] is false (reduced motion snaps), then settle: [report] =
     * the view's `onZoom` fires when the scale changed.
     */
    fun animateTo(target: ZoomView, animated: Boolean = true, report: Boolean = true) {
        cancel()
        val s = scope
        if (s == null || reduced || !animated || target == view) {
            view = target
            settle(report)
            return
        }
        val from = view
        job = s.launch {
            animate(0f, 1f, animationSpec = spec) { t, _ -> view = interpolate(from, target, t) }
            settle(report)
        }
    }

    /**
     * Zoom to [scale] about [anchor] (viewport dp; null = the centre),
     * animated unless [animated] is false; [report] = fire `onZoom` (false
     * for a scale the host set itself).
     */
    fun zoomTo(scale: Float, anchor: Offset? = null, animated: Boolean = true, report: Boolean = true) {
        val f = frame ?: return
        animateTo(math.zoomTo(view, scale, anchor ?: Offset(f.viewportWidth / 2f, f.viewportHeight / 2f), f), animated, report)
    }

    /** One preset step in ([direction] 1) or out (−1) about the centre, animated. */
    fun step(direction: Int) {
        val f = frame ?: return
        animateTo(math.step(view, direction, f))
    }

    /** Back to the rest view, animated. */
    fun reset() {
        val r = rest ?: return
        animateTo(r.view)
    }

    /** A zoom key (`+ = - 0 1`): true when it was one. */
    fun key(key: String): Boolean {
        val f = frame ?: return false
        val next = math.key(view, key, f) ?: return false
        animateTo(next)
        return true
    }

    /** Release momentum at (vx, vy) dp/ms from the clamped view (the core's decay). */
    internal fun fling(vx: Float, vy: Float) {
        val f = frame ?: return
        val s = scope
        val base = math.clamp(view, f)
        cancel()
        if (s == null || !math.momentumActive(vx, vy, 0f)) {
            view = base
            settle()
            return
        }
        job = s.launch {
            val t0 = withFrameNanos { it }
            while (true) {
                val now = withFrameNanos { it }
                val t = (now - t0) / 1_000_000f
                view = math.momentum(base, vx, vy, t, f)
                if (!math.momentumActive(vx, vy, t)) break
            }
            settle()
        }
    }

    /** A gesture ended: report a zoom change once ([report] false = take it silently). */
    internal fun settle(report: Boolean = true) {
        val prev = settledScale
        if (prev != null && abs(prev - view.scale) < 1e-4f) return
        settledScale = view.scale
        if (!report) return
        activity += 1
        onSettled?.invoke(view, atRest)
    }

    companion object {
        /** The neutral theme's `ease.standard`. */
        val STANDARD_EASE = CubicBezierEasing(0.2f, 0f, 0f, 1f)

        /**
         * [from] → [to] at [t]: the scale linearly, the offsets about the
         * point both views agree on (a zoom stays anchored), linearly when
         * the scale does not change.
         */
        fun interpolate(from: ZoomView, to: ZoomView, t: Float): ZoomView {
            val s = from.scale + (to.scale - from.scale) * t
            val r = if (from.scale > 0f) to.scale / from.scale else 1f
            if (abs(1f - r) < 1e-4f) return ZoomView(s, from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t)
            fun axis(a: Float, b: Float): Float {
                val fixed = (b - a * r) / (1f - r)
                return fixed - (fixed - a) * (s / from.scale)
            }
            return ZoomView(s, axis(from.x, to.x), axis(from.y, to.y))
        }
    }
}

/** A [ZoomableImageState] answered by [math], kept across recompositions. */
@Composable
fun rememberZoomableImageState(math: ZoomMath): ZoomableImageState = remember(math) { ZoomableImageState(math) }

/** A drawn piece of the pill (and its Menu) a [ZoomLooks] styles. */
enum class ZoomPart { Pill, ZoomOut, Percent, ZoomIn, Divider, Fit, Menu, MenuItem, MenuCheck }

/** The interaction state a [ZoomPart] is drawn in. */
@Immutable
data class ZoomPartState(val hovered: Boolean = false, val pressed: Boolean = false, val disabled: Boolean = false, val selected: Boolean = false)

/**
 * How one [ZoomPart] looks: its box (background, border, radius, shadow
 * elevation, opacity, fixed size, horizontal padding) and its content
 * colour + text size (the percent and the menu rows).
 */
@Immutable
data class ZoomPartLook(
    val background: Color? = null,
    val content: Color = Color.Unspecified,
    val borderColor: Color? = null,
    val borderWidth: Dp = 0.dp,
    val radius: Dp = 0.dp,
    val elevation: Dp = 0.dp,
    val opacity: Float = 1f,
    val width: Dp? = null,
    val height: Dp? = null,
    val paddingHorizontal: Dp = 0.dp,
    val padding: Dp = 0.dp,
    val gap: Dp = 0.dp,
    val fontSize: TextUnit = 14.sp,
    val lineHeight: TextUnit = 20.sp,
    val fontWeight: FontWeight = FontWeight.Medium,
)

/** The looks of the pill and its Menu: the eUI painter resolves its recipes; [fromTokens] is an app's default. */
fun interface ZoomLooks {
    fun look(part: ZoomPart, state: ZoomPartState): ZoomPartLook

    companion object {
        /** The neutral recipe in [tokens]' colours. */
        fun fromTokens(tokens: PrimitiveTokens): ZoomLooks = ZoomLooks { part, s ->
            val lit = s.hovered || s.pressed || (s.selected && part != ZoomPart.MenuItem)
            val button = ZoomPartLook(
                background = if (lit) tokens.accent else null,
                content = tokens.foreground,
                radius = 999.dp,
                opacity = if (s.disabled) 0.5f else 1f,
                width = 28.dp,
                height = 28.dp,
            )
            when (part) {
                ZoomPart.Pill -> ZoomPartLook(background = tokens.card.takeIf { it.alpha > 0f } ?: tokens.background.takeIf { it.alpha > 0f } ?: Color(0xF2FFFFFF), borderColor = tokens.border, borderWidth = tokens.hairline, radius = 999.dp, elevation = 8.dp, padding = 4.dp, gap = 2.dp)
                ZoomPart.ZoomOut, ZoomPart.ZoomIn, ZoomPart.Fit -> button
                ZoomPart.Percent -> button.copy(width = null, paddingHorizontal = 12.dp)
                ZoomPart.Divider -> ZoomPartLook(background = tokens.border, width = 1.dp, height = 16.dp)
                ZoomPart.Menu -> ZoomPartLook(background = tokens.card.takeIf { it.alpha > 0f } ?: Color(0xFFFFFFFF), content = tokens.foreground, borderColor = tokens.border, borderWidth = tokens.hairline, radius = tokens.radiusMd, elevation = 6.dp, padding = 4.dp)
                ZoomPart.MenuItem -> ZoomPartLook(background = if (s.hovered || s.pressed) tokens.accent else null, content = tokens.foreground, radius = tokens.radiusSm, height = tokens.rowHeight, paddingHorizontal = 8.dp, gap = 8.dp, fontWeight = FontWeight.Normal)
                ZoomPart.MenuCheck -> ZoomPartLook(content = tokens.foreground, width = 16.dp, height = 16.dp)
            }
        }
    }
}

/** The pill's glyphs, each drawn in its content colour at its size; [Default] draws simple strokes. */
@Immutable
class ZoomIcons(
    val zoomOut: @Composable (Color, Dp) -> Unit = { c, s -> StrokeGlyph(c, s, ZoomGlyph.Minus) },
    val zoomIn: @Composable (Color, Dp) -> Unit = { c, s -> StrokeGlyph(c, s, ZoomGlyph.Plus) },
    val fit: @Composable (Color, Dp) -> Unit = { c, s -> StrokeGlyph(c, s, ZoomGlyph.Fit) },
    val check: @Composable (Color, Dp) -> Unit = { c, s -> StrokeGlyph(c, s, ZoomGlyph.Check) },
) {
    companion object {
        val Default = ZoomIcons()
    }
}

/** The built-in strings (catalog/strings.json; a host overrides them). */
@Immutable
data class ZoomLabels(
    val zoomIn: String = "Zoom in",
    val zoomOut: String = "Zoom out",
    val zoomToFit: String = "Zoom to fit",
    val zoomLevel: String = "Zoom level",
    val zoomFit: String = "Fit",
)

/** The pill's geometry (style.json `zoomableImage.pill`) and the auto-hide delay (`autoHideMs`). */
@Immutable
data class ZoomPillMetrics(
    val height: Dp = 36.dp,
    val inset: Dp = 16.dp,
    val padding: Dp = 4.dp,
    val gap: Dp = 2.dp,
    val percentMinWidth: Dp = 56.dp,
    val dividerHeight: Dp = 16.dp,
    val autoHideMs: Long = 2000L,
    val iconSize: Dp = 16.dp,
)

/** Everything the pill draws with. */
@Immutable
data class ZoomPillAppearance(
    val looks: ZoomLooks,
    val icons: ZoomIcons = ZoomIcons.Default,
    val labels: ZoomLabels = ZoomLabels(),
    val metrics: ZoomPillMetrics = ZoomPillMetrics(),
) {
    companion object {
        /** The default look in the ambient [LocalPrimitiveTokens]. */
        @Composable
        fun default(): ZoomPillAppearance = ZoomPillAppearance(ZoomLooks.fromTokens(LocalPrimitiveTokens.current))
    }
}

/** The motion of animated zooms: [spec] (the theme's `motion.standard`); [reduced] = snap. */
@Immutable
data class ZoomMotion(val spec: AnimationSpec<Float> = tween(180, easing = ZoomableImageState.STANDARD_EASE), val reduced: Boolean = false)

/** The Compose limit for one side of a layout (px); a larger picture lays out smaller and scales back up. */
private const val MAX_LAYOUT_PX = 32_000f

/**
 * A picture that zooms and pans (EXP-1586, the eUI `ZoomableImage`).
 *
 * [content] is laid out at the picture's [naturalSize] (image pixels = dp
 * at 100%, e.g. an `Image(bitmap, contentScale = FillBounds)` or a strip of
 * tiles) and drawn at the [state]'s view, clipped to this view's bounds
 * (sized by [modifier]). It opens at the rest view ([initial]); a new
 * [contentKey] (a new picture) goes back to it without animation; a resize
 * re-fits at rest, else keeps the scale about the centre.
 *
 * Input: pinch about the fingers (rubber band past the limits, sprung back
 * on release); one finger pans only along an axis with pan room (a
 * horizontal swipe without x room is left to the host, e.g. a
 * `HorizontalPager`), with the core's momentum on release; double tap:
 * rest → max(100%, 2 × rest) about the point, zoomed → rest; a single tap
 * fires [onTap] (and on touch toggles an `auto` pill); ctrl/⌘ + wheel
 * zooms about the pointer, a plain wheel pans when zoomed; focused, the
 * keys `+ = - 0 1`. THE pill ([controls]) sits bottom-centre: − / the
 * percent (opening the preset Menu) / + / divider / fit.
 *
 * [onZoom] fires after each settled zoom change with the scale (1 = 100%)
 * and whether it is the rest view. [overlay] paints over the picture,
 * under the pill (a placeholder while the picture loads).
 */
@Composable
fun ZoomableImageView(
    naturalSize: Size,
    state: ZoomableImageState,
    modifier: Modifier = Modifier,
    initial: ZoomInitial = ZoomInitial.Auto,
    maxScale: Float? = null,
    controls: ZoomControls = ZoomControls.Auto,
    contentKey: Any? = null,
    contentDescription: String? = null,
    onZoom: ((scale: Float, atRest: Boolean) -> Unit)? = null,
    onTap: (() -> Unit)? = null,
    appearance: ZoomPillAppearance = ZoomPillAppearance.default(),
    motion: ZoomMotion = ZoomMotion(),
    pictureRadius: Dp = 0.dp,
    overlay: (@Composable BoxScope.() -> Unit)? = null,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    val zoomCallback by rememberUpdatedState(onZoom)
    val tapCallback by rememberUpdatedState(onTap)
    SideEffect {
        state.scope = scope
        state.spec = motion.spec
        state.reduced = motion.reduced
        state.onSettled = { v, rest -> zoomCallback?.invoke(v.scale, rest) }
    }
    var viewport by remember { mutableStateOf(Size.Zero) }
    // A new picture: back to rest (no animation) on the next frame.
    var lastKey by remember { mutableStateOf<Any?>(Unit) }
    if (viewport.width > 0f && viewport.height > 0f && naturalSize.width > 0f && naturalSize.height > 0f) {
        val next = ZoomFrame(naturalSize.width, naturalSize.height, viewport.width, viewport.height, initial, maxScale)
        val reset = lastKey != contentKey
        if (reset || state.frame != next) {
            state.setFrame(next, reset)
            lastKey = contentKey
        }
    }
    val density = LocalDensity.current
    var pillShown by remember(controls) { mutableStateOf(controls != ZoomControls.None) }
    var pillHeld by remember { mutableStateOf(false) }
    var hovering by remember { mutableStateOf(false) }
    var focused by remember { mutableStateOf(false) }
    var poke by remember { mutableIntStateOf(0) }
    val currentControls by rememberUpdatedState(controls)
    fun show() {
        if (currentControls == ZoomControls.None) return
        pillShown = true
        poke += 1
    }
    LaunchedEffect(controls, pillShown, pillHeld, hovering, focused, poke, state.activity) {
        if (controls == ZoomControls.Auto && pillShown && !pillHeld && !focused) {
            delay(appearance.metrics.autoHideMs)
            pillShown = false
        }
    }
    LaunchedEffect(state.activity) { if (state.activity > 0) show() }

    Box(
        modifier
            .clipToBounds()
            .onSizeChanged { with(density) { viewport = Size(it.width.toDp().value, it.height.toDp().value) } },
    ) {
        val frame = state.frame
        if (frame != null) {
            val f = min(1f, MAX_LAYOUT_PX / (max(frame.naturalWidth, frame.naturalHeight) * density.density))
            val radiusPx = with(density) { pictureRadius.toPx() }
            Box(
                Modifier
                    .wrapContentSize(Alignment.TopStart, unbounded = true)
                    .requiredSize((frame.naturalWidth * f).dp, (frame.naturalHeight * f).dp)
                    .graphicsLayer {
                        val v = state.view
                        transformOrigin = TransformOrigin(0f, 0f)
                        translationX = v.x * density.density
                        translationY = v.y * density.density
                        scaleX = v.scale / f
                        scaleY = v.scale / f
                        if (radiusPx > 0f && v.scale > 0f) {
                            shape = RoundedCornerShape(radiusPx * f / v.scale)
                            clip = true
                        }
                    },
            ) { content() }
        }
        // The input surface (and the picture's accessibility node) over the whole viewport.
        Box(
            Modifier
                .fillMaxSize()
                .testTag("zoomable-image")
                .semantics {
                    role = Role.Image
                    if (contentDescription != null) this.contentDescription = contentDescription
                    stateDescription = state.percent
                }
                .onFocusChanged {
                    focused = it.isFocused
                    if (it.isFocused) show()
                }
                .onKeyEvent { e ->
                    if (e.type != KeyEventType.KeyDown || e.isCtrlPressed || e.isMetaPressed || e.isAltPressed) return@onKeyEvent false
                    val k = when (e.key) {
                        Key.Plus, Key.Equals, Key.NumPadAdd -> "+"
                        Key.Minus, Key.NumPadSubtract -> "-"
                        Key.Zero, Key.NumPad0 -> "0"
                        Key.One, Key.NumPad1 -> "1"
                        else -> null
                    } ?: return@onKeyEvent false
                    state.key(k).also { if (it) show() }
                }
                .focusable()
                .pointerInput(state) { zoomGestures(state, onTap = { touch -> if (touch && currentControls == ZoomControls.Auto) { if (pillShown) pillShown = false else show() }; tapCallback?.invoke() }) }
                .pointerInput(state, "mouse") {
                    // ⌘/ctrl-wheel zooms report once the wheel rests.
                    var wheelSettle: Job? = null
                    awaitPointerEventScope {
                        while (true) {
                            val e = awaitPointerEvent()
                            val change = e.changes.firstOrNull() ?: continue
                            when (e.type) {
                                PointerEventType.Enter -> if (change.type == PointerType.Mouse) { hovering = true; show() }
                                PointerEventType.Exit -> if (change.type == PointerType.Mouse) hovering = false
                                PointerEventType.Move -> if (change.type == PointerType.Mouse) show()
                                PointerEventType.Scroll -> {
                                    val f = state.frame ?: continue
                                    val d = change.scrollDelta
                                    // A wheel step is 64 dp (Compose's own mouse-wheel step); the core does the rest.
                                    val zoom = e.keyboardModifiers.isCtrlPressed || e.keyboardModifiers.isMetaPressed
                                    val anchor = Offset(change.position.x / density.density, change.position.y / density.density)
                                    val next = if (zoom) {
                                        state.math.wheel(state.view, d.y * WHEEL_STEP_DP, false, anchor, f)
                                    } else {
                                        state.math.scroll(state.view, d.x * WHEEL_STEP_DP, d.y * WHEEL_STEP_DP, f)
                                    }
                                    if (next != state.view) {
                                        state.cancel()
                                        state.snapTo(next)
                                        change.consume()
                                        if (zoom) {
                                            wheelSettle?.cancel()
                                            wheelSettle = state.scope?.launch {
                                                delay(WHEEL_SETTLE_MS)
                                                state.settle()
                                            }
                                        } else {
                                            show()
                                        }
                                    }
                                }
                                else -> Unit
                            }
                        }
                    }
                },
        )
        overlay?.invoke(this)
        if (controls != ZoomControls.None && frame != null) {
            AnimatedVisibility(
                visible = pillShown || controls == ZoomControls.Always,
                modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = appearance.metrics.inset),
                enter = if (motion.reduced) fadeIn(snap()) else fadeIn(),
                exit = if (motion.reduced) fadeOut(snap()) else fadeOut(),
            ) {
                ZoomPill(state, appearance, onHeld = { pillHeld = it })
            }
        }
    }
}

/** [ZoomableImageView] of a [Painter] (natural size = its intrinsic size unless given). */
@Composable
fun ZoomableImageView(
    painter: Painter,
    contentDescription: String?,
    state: ZoomableImageState,
    modifier: Modifier = Modifier,
    naturalSize: Size = painter.intrinsicSize,
    initial: ZoomInitial = ZoomInitial.Auto,
    maxScale: Float? = null,
    controls: ZoomControls = ZoomControls.Auto,
    onZoom: ((scale: Float, atRest: Boolean) -> Unit)? = null,
    onTap: (() -> Unit)? = null,
    appearance: ZoomPillAppearance = ZoomPillAppearance.default(),
    motion: ZoomMotion = ZoomMotion(),
    pictureRadius: Dp = 0.dp,
) = ZoomableImageView(
    naturalSize = naturalSize,
    state = state,
    modifier = modifier,
    initial = initial,
    maxScale = maxScale,
    controls = controls,
    contentKey = painter,
    contentDescription = contentDescription,
    onZoom = onZoom,
    onTap = onTap,
    appearance = appearance,
    motion = motion,
    pictureRadius = pictureRadius,
) { Image(painter, null, Modifier.fillMaxSize(), contentScale = ContentScale.FillBounds) }

/** [ZoomableImageView] of an [ImageBitmap] (natural size = its pixel size unless given). */
@Composable
fun ZoomableImageView(
    bitmap: ImageBitmap,
    contentDescription: String?,
    state: ZoomableImageState,
    modifier: Modifier = Modifier,
    naturalSize: Size = Size(bitmap.width.toFloat(), bitmap.height.toFloat()),
    initial: ZoomInitial = ZoomInitial.Auto,
    maxScale: Float? = null,
    controls: ZoomControls = ZoomControls.Auto,
    onZoom: ((scale: Float, atRest: Boolean) -> Unit)? = null,
    onTap: (() -> Unit)? = null,
    appearance: ZoomPillAppearance = ZoomPillAppearance.default(),
    motion: ZoomMotion = ZoomMotion(),
    pictureRadius: Dp = 0.dp,
) = ZoomableImageView(
    naturalSize = naturalSize,
    state = state,
    modifier = modifier,
    initial = initial,
    maxScale = maxScale,
    controls = controls,
    contentKey = bitmap,
    contentDescription = contentDescription,
    onZoom = onZoom,
    onTap = onTap,
    appearance = appearance,
    motion = motion,
    pictureRadius = pictureRadius,
) { Image(bitmap, null, Modifier.fillMaxSize(), contentScale = ContentScale.FillBounds) }

/** dp per wheel unit (Compose's mouse-wheel step on Android). */
private const val WHEEL_STEP_DP = 64f

/** How long the wheel rests before a wheel zoom reports (`onZoom`). */
private const val WHEEL_SETTLE_MS = 150L

private enum class Drag { None, Pan, Pinch, Host }

/**
 * Touch and mouse-button input: pinch (2+ pointers, always consumed), a
 * one-pointer pan only along an axis with room (otherwise nothing is
 * consumed, so the host's pager or scroller takes the swipe), the release
 * (spring back when rubber-banded, else the core's momentum) and taps
 * (a double tap zooms, a single one waits out the double-tap timeout).
 */
private suspend fun androidx.compose.ui.input.pointer.PointerInputScope.zoomGestures(state: ZoomableImageState, onTap: (touch: Boolean) -> Unit) {
    val d = density
    var lastTap: Pair<Long, Offset>? = null
    var pendingTap: Job? = null
    coroutineScopeOf { scope ->
        awaitEachGesture {
            val down = awaitFirstDown(requireUnconsumed = false)
            state.cancel()
            val touch = down.type != PointerType.Mouse
            var mode = Drag.None
            var start = state.view
            var origin = down.position
            val tracker = VelocityTracker()
            tracker.addPosition(down.uptimeMillis, down.position)
            var pinchStart = state.view
            var c0 = Offset.Zero
            var span0 = 0f
            var pointers = 1
            var moved = false
            var upAt = down.uptimeMillis
            var upPos = down.position
            while (true) {
                val event = awaitPointerEvent()
                val pressed = event.changes.filter { it.pressed }
                if (pressed.isEmpty()) {
                    event.changes.firstOrNull()?.let { upAt = it.uptimeMillis; upPos = it.position }
                    break
                }
                val f = state.frame
                if (pressed.size >= 2 && f != null) {
                    val c = Offset(pressed.map { it.position.x }.average().toFloat(), pressed.map { it.position.y }.average().toFloat())
                    val span = pressed.map { (it.position - c).getDistance() }.average().toFloat()
                    if (mode != Drag.Pinch || pressed.size != pointers) {
                        pinchStart = state.view
                        c0 = c
                        span0 = span
                    }
                    mode = Drag.Pinch
                    pointers = pressed.size
                    moved = true
                    val factor = if (span0 > 0f) span / span0 else 1f
                    val shifted = ZoomView(pinchStart.scale, pinchStart.x + (c.x - c0.x) / d, pinchStart.y + (c.y - c0.y) / d)
                    state.snapTo(state.math.pinch(shifted, factor, Offset(c.x / d, c.y / d), f))
                    event.changes.forEach { it.consume() }
                    continue
                }
                val ch = pressed[0]
                if (mode == Drag.Pinch) {
                    // Back to one finger: pan on from here.
                    mode = Drag.Pan
                    start = state.view
                    origin = ch.position
                    tracker.resetTracking()
                    pointers = 1
                }
                tracker.addPosition(ch.uptimeMillis, ch.position)
                if (mode == Drag.Host || f == null) continue
                val delta = ch.position - origin
                if (mode == Drag.None) {
                    if (delta.getDistance() < viewConfiguration.touchSlop) continue
                    moved = true
                    if (ch.isConsumed) {
                        mode = Drag.Host
                        continue
                    }
                    val room = state.math.panRoom(state.view, f)
                    val horizontal = abs(delta.x) > abs(delta.y)
                    if ((horizontal && room.x) || (!horizontal && room.y)) {
                        mode = Drag.Pan
                        start = state.view
                        origin = ch.position
                    } else {
                        mode = Drag.Host
                        continue
                    }
                }
                val now = ch.position - origin
                state.snapTo(state.math.drag(start, now.x / d, now.y / d, f))
                ch.consume()
            }
            val f = state.frame
            when {
                f == null -> Unit
                mode == Drag.Pan || mode == Drag.Pinch -> {
                    val clamped = state.math.clamp(state.view, f)
                    val off = abs(clamped.scale - state.view.scale) > 1e-4f || abs(clamped.x - state.view.x) > 0.01f || abs(clamped.y - state.view.y) > 0.01f
                    if (off || mode == Drag.Pinch) {
                        state.animateTo(clamped)
                    } else {
                        val v = tracker.calculateVelocity()
                        // px/s → dp/ms
                        state.fling(v.x / d / 1000f, v.y / d / 1000f)
                    }
                }
                !moved && mode == Drag.None -> {
                    val prev = lastTap
                    val point = Offset(upPos.x / d, upPos.y / d)
                    if (prev != null && upAt - prev.first <= viewConfiguration.doubleTapTimeoutMillis && (upPos - prev.second).getDistance() <= viewConfiguration.touchSlop * 4f) {
                        pendingTap?.cancel()
                        lastTap = null
                        state.animateTo(state.math.doubleTap(state.view, point, f))
                    } else {
                        lastTap = upAt to upPos
                        pendingTap?.cancel()
                        pendingTap = scope.launch {
                            delay(viewConfiguration.doubleTapTimeoutMillis)
                            onTap(touch)
                        }
                    }
                }
            }
        }
    }
}

/** Runs [block] with a scope for the pending single tap (cancelled with the pointer input). */
private suspend fun coroutineScopeOf(block: suspend (CoroutineScope) -> Unit) = kotlinx.coroutines.coroutineScope { block(this) }

/**
 * THE pill: one capsule of − / the percent (a button opening the preset
 * Menu) / + / a hairline divider / fit (selected while at rest). − is
 * disabled at the min, + at the max. [onHeld] = hovered or focused inside
 * (an `auto` pill stays).
 */
@Composable
fun ZoomPill(state: ZoomableImageState, appearance: ZoomPillAppearance, modifier: Modifier = Modifier, onHeld: (Boolean) -> Unit = {}) {
    val looks = appearance.looks
    val m = appearance.metrics
    val labels = appearance.labels
    val pill = looks.look(ZoomPart.Pill, ZoomPartState())
    val bounds = state.bounds
    val atRest = state.atRest
    var menuOpen by remember { mutableStateOf(false) }
    val pillInteractions = remember { MutableInteractionSource() }
    val pillHovered by pillInteractions.collectIsHoveredAsState()
    LaunchedEffect(pillHovered, menuOpen) { onHeld(pillHovered || menuOpen) }
    val shape = RoundedCornerShape(pill.radius)
    Row(
        modifier
            .testTag("zoom-pill")
            .hoverable(pillInteractions)
            .then(if (pill.elevation > 0.dp) Modifier.shadow(pill.elevation, shape, clip = false) else Modifier)
            .then(if (pill.background != null) Modifier.background(pill.background, shape) else Modifier)
            .then(if (pill.borderColor != null && pill.borderWidth > 0.dp) Modifier.border(pill.borderWidth, pill.borderColor, shape) else Modifier)
            .height(m.height)
            .padding(horizontal = m.padding, vertical = m.padding),
        horizontalArrangement = Arrangement.spacedBy(m.gap),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PillButton(ZoomPart.ZoomOut, looks, labels.zoomOut, disabled = bounds.atMin, onClick = { state.step(-1) }) { c -> appearance.icons.zoomOut(c, m.iconSize) }
        Box {
            PillButton(ZoomPart.Percent, looks, labels.zoomLevel, value = state.percent, minWidth = m.percentMinWidth, onClick = { menuOpen = !menuOpen }) { c ->
                val look = looks.look(ZoomPart.Percent, ZoomPartState())
                BasicText(
                    state.percent,
                    style = TextStyle(color = c, fontSize = look.fontSize, lineHeight = look.lineHeight, fontWeight = look.fontWeight, textAlign = TextAlign.Center, fontFeatureSettings = "tnum"),
                    maxLines = 1,
                )
            }
            if (menuOpen) ZoomMenu(state, appearance, onDismiss = { menuOpen = false })
        }
        PillButton(ZoomPart.ZoomIn, looks, labels.zoomIn, disabled = bounds.atMax, onClick = { state.step(1) }) { c -> appearance.icons.zoomIn(c, m.iconSize) }
        val divider = looks.look(ZoomPart.Divider, ZoomPartState())
        Spacer(Modifier.width(divider.width ?: 1.dp).height(divider.height ?: m.dividerHeight).background(divider.background ?: Color.Transparent))
        PillButton(ZoomPart.Fit, looks, labels.zoomToFit, selected = atRest, onClick = { state.reset() }) { c -> appearance.icons.fit(c, m.iconSize) }
    }
}

@Composable
private fun PillButton(
    part: ZoomPart,
    looks: ZoomLooks,
    label: String,
    disabled: Boolean = false,
    selected: Boolean = false,
    value: String? = null,
    minWidth: Dp = 0.dp,
    onClick: () -> Unit,
    content: @Composable (Color) -> Unit,
) {
    val interactions = remember { MutableInteractionSource() }
    val hovered by interactions.collectIsHoveredAsState()
    val pressed by interactions.collectIsPressedAsState()
    val look = looks.look(part, ZoomPartState(hovered = hovered && !disabled, pressed = pressed && !disabled, disabled = disabled, selected = selected))
    val shape = RoundedCornerShape(look.radius)
    Box(
        Modifier
            .testTag("zoom-${part.name.lowercase()}")
            .then(if (look.width != null) Modifier.width(look.width) else Modifier.defaultMinSize(minWidth = minWidth))
            .height(look.height ?: 28.dp)
            .alpha(look.opacity)
            .then(if (look.background != null) Modifier.background(look.background, shape) else Modifier)
            .hoverable(interactions, enabled = !disabled)
            .clickable(interactions, indication = null, enabled = !disabled, onClick = onClick)
            .semantics {
                contentDescription = label
                role = Role.Button
                if (value != null) stateDescription = value
                if (part == ZoomPart.Fit) this.selected = selected
            }
            .padding(horizontal = look.paddingHorizontal),
        contentAlignment = Alignment.Center,
    ) {
        content(look.content)
    }
}

/**
 * The percent button's Menu (Fit, 50 / 100 / 200 / 400 % inside the
 * limits, from the core's `menu`), drawn by THIS primitive in a popup above
 * the pill (never the platform's dropdown): a pick zooms about the centre
 * (Fit = the rest view).
 */
@Composable
private fun ZoomMenu(state: ZoomableImageState, appearance: ZoomPillAppearance, onDismiss: () -> Unit) {
    val f = state.frame ?: return
    val items = remember(state.view, f, appearance.labels.zoomFit) { state.math.menu(state.view, f, appearance.labels.zoomFit) }
    val looks = appearance.looks
    val menu = looks.look(ZoomPart.Menu, ZoomPartState())
    val gap = with(LocalDensity.current) { 6.dp.roundToPx() }
    val shape = RoundedCornerShape(menu.radius)
    Popup(popupPositionProvider = AbovePosition(gap), onDismissRequest = onDismiss, properties = PopupProperties(focusable = true)) {
        Column(
            Modifier
                .testTag("zoom-menu")
                .then(if (menu.elevation > 0.dp) Modifier.shadow(menu.elevation, shape, clip = false) else Modifier)
                .then(if (menu.background != null) Modifier.background(menu.background, shape) else Modifier)
                .then(if (menu.borderColor != null && menu.borderWidth > 0.dp) Modifier.border(menu.borderWidth, menu.borderColor, shape) else Modifier)
                .padding(menu.padding)
                .width(IntrinsicSize.Max),
        ) {
            for (item in items) {
                val interactions = remember(item.label) { MutableInteractionSource() }
                val hovered by interactions.collectIsHoveredAsState()
                val pressed by interactions.collectIsPressedAsState()
                val look = looks.look(ZoomPart.MenuItem, ZoomPartState(hovered = hovered, pressed = pressed, selected = item.checked))
                val check = looks.look(ZoomPart.MenuCheck, ZoomPartState())
                Row(
                    Modifier
                        .testTag("zoom-menu-${item.label}")
                        .height(look.height ?: 32.dp)
                        .fillMaxWidth()
                        .then(if (look.background != null) Modifier.background(look.background, RoundedCornerShape(look.radius)) else Modifier)
                        .hoverable(interactions)
                        .clickable(interactions, indication = null) {
                            onDismiss()
                            val s = item.scale
                            if (s == null) state.reset() else state.zoomTo(s)
                        }
                        .semantics {
                            role = Role.Checkbox
                            selected = item.checked
                        }
                        .padding(horizontal = look.paddingHorizontal),
                    horizontalArrangement = Arrangement.spacedBy(look.gap),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Box(Modifier.size(check.width ?: 16.dp), contentAlignment = Alignment.Center) {
                        if (item.checked) appearance.icons.check(check.content.takeIf { it != Color.Unspecified } ?: look.content, check.width ?: 16.dp)
                    }
                    BasicText(item.label, style = TextStyle(color = look.content, fontSize = look.fontSize, lineHeight = look.lineHeight, fontWeight = look.fontWeight, fontFeatureSettings = "tnum"), maxLines = 1)
                }
            }
        }
    }
}

/** Centred above the anchor ([gap] px), below it when there is no room; clamped into the window. */
private class AbovePosition(private val gap: Int) : PopupPositionProvider {
    override fun calculatePosition(anchorBounds: IntRect, windowSize: IntSize, layoutDirection: LayoutDirection, popupContentSize: IntSize): IntOffset {
        val x = (anchorBounds.center.x - popupContentSize.width / 2).coerceIn(0, max(0, windowSize.width - popupContentSize.width))
        val above = anchorBounds.top - gap - popupContentSize.height
        val y = if (above >= 0) above else anchorBounds.bottom + gap
        return IntOffset(x, y)
    }
}

internal enum class ZoomGlyph { Minus, Plus, Fit, Check }

/** The default glyphs: 1.5 dp round strokes. */
@Composable
internal fun StrokeGlyph(color: Color, size: Dp, glyph: ZoomGlyph) {
    Canvas(Modifier.size(size)) {
        val w = 1.5.dp.toPx()
        val s = this.size.minDimension
        val a = s * 0.2f
        val b = s * 0.8f
        val m = s / 2f
        fun line(x0: Float, y0: Float, x1: Float, y1: Float) = drawLine(color, Offset(x0, y0), Offset(x1, y1), w, StrokeCap.Round)
        when (glyph) {
            ZoomGlyph.Minus -> line(a, m, b, m)
            ZoomGlyph.Plus -> {
                line(a, m, b, m)
                line(m, a, m, b)
            }
            ZoomGlyph.Fit -> {
                val k = s * 0.22f
                line(a, a, a + k, a); line(a, a, a, a + k)
                line(b, a, b - k, a); line(b, a, b, a + k)
                line(a, b, a + k, b); line(a, b, a, b - k)
                line(b, b, b - k, b); line(b, b, b, b - k)
            }
            ZoomGlyph.Check -> {
                line(s * 0.2f, s * 0.52f, s * 0.42f, s * 0.74f)
                line(s * 0.42f, s * 0.74f, s * 0.8f, s * 0.3f)
            }
        }
    }
}
