package at.exponential.ui.compose

import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontSynthesis
import androidx.compose.ui.text.font.FontWeight
import at.exponential.ui.ffi.CanvasPlayer
import at.exponential.ui.ffi.UiException
import at.exponential.ui.ffi.canvasToDrawing
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.dispatch
import at.exponential.ui.theme.Mode
import at.exponential.ui.theme.ThemeHandle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlin.math.hypot

/**
 * Round 8 (docs/round-8-contract.md §1): a compiled `Canvas`: the core's
 * [CanvasPlayer] over uniffi (ONE static frame of Motion's display list +
 * the `text` ops, packed VERSION 2). Compiled once per `size` / `shapes`
 * ([document]), sampled once per theme and mode ([frame]); never played.
 */
class CanvasSource(val player: CanvasPlayer) {
    private var sampledKey: Any? = null
    private var list: MotionList? = null

    /** The frame in this theme and mode (null = an empty frame). */
    fun frame(theme: ThemeHandle?, mode: Mode): MotionList? {
        val key = Pair(theme, mode)
        if (key != sampledKey) {
            sampledKey = key
            val slots = MotionSource.themeSlots(player.slotIds(), { theme?.colorHex(it, mode) }, mode == Mode.Dark)
            list = MotionList.decode(player.sample(slots))
        }
        return list
    }

    companion object {
        /** The document the engine compiles: the node's `size` and `shapes`. */
        fun document(props: Props): String = JsonValue.Obj(mapOf("size" to (props["size"] ?: JsonValue.Null), "shapes" to (props["shapes"] ?: JsonValue.Null))).json

        /** The compiled drawing; null when the engine refuses it (the box paints nothing, never a crash). */
        fun compile(document: String): CanvasSource? = try {
            CanvasSource(CanvasPlayer(document))
        } catch (_: UiException) {
            null
        }
    }
}

/**
 * The `Canvas` leaf: [MotionPainter] paints the frame under `fit` (shapes
 * clipped to the composition, then the `text` ops in the theme's
 * `$type.family.sans` / `.mono` at their CSS weight). Semantics: an image
 * named by `label` (Semantics.kt). With `pointer: true`: `press {x, y}` on
 * a tap / click released inside, `move {x, y}` while hovered or dragging
 * (at most once per frame, the latest point), `leave` when the mouse exits
 * or the finger lifts; x / y = `canvasToDrawing` (the fit inverted, drawing
 * units). Without it the leaf takes no pointer at all (inert, an image).
 */
@Composable
internal fun CanvasLeaf(cx: LeafContext) {
    val model = cx.model
    val props = cx.props
    val document = remember(props) { CanvasSource.document(props) }
    val source = remember(document) { CanvasSource.compile(document) }
    val theme = model.effectiveTheme
    val mode = model.mode
    val list = remember(source, theme, mode) { source?.frame(theme, mode) }
    val fit = props.str("fit").ifEmpty { "contain" }
    val typefaces = rememberCanvasTypefaces(model, theme)
    val pointer = if (props["pointer"]?.bool == true && list != null) {
        val scope = rememberCoroutineScope()
        val sender = remember(cx.node.id) { CanvasPointer(cx.node.id) }
        sender.model = model
        Modifier.pointerInput(list.width, list.height, fit) {
            val drawing = Pair(list.width.toDouble(), list.height.toDouble())
            val slop = viewConfiguration.touchSlop
            fun box() = Pair(size.width.toDouble(), size.height.toDouble())
            awaitPointerEventScope {
                var down: Offset? = null
                while (true) {
                    val e = awaitPointerEvent()
                    val c = e.changes.firstOrNull() ?: continue
                    val (bw, bh) = box()
                    fun at(p: Offset) = canvasToDrawing(p.x.toDouble(), p.y.toDouble(), drawing.first, drawing.second, bw, bh, fit)
                    when (e.type) {
                        PointerEventType.Press -> {
                            down = c.position
                            sender.move(scope, at(c.position))
                        }
                        PointerEventType.Move -> {
                            val d = down
                            if (d != null && hypot(c.position.x - d.x, c.position.y - d.y) > slop) down = null
                            sender.move(scope, at(c.position))
                        }
                        PointerEventType.Release -> {
                            val d = down
                            down = null
                            val inside = c.position.x >= 0f && c.position.y >= 0f && c.position.x <= size.width && c.position.y <= size.height
                            sender.flush()
                            if (d != null && inside && !c.isConsumed) sender.send("press", at(c.position))
                            if (c.type != PointerType.Mouse) sender.send("leave", null)
                        }
                        PointerEventType.Exit -> {
                            sender.flush()
                            sender.send("leave", null)
                        }
                        else -> Unit
                    }
                }
            }
        }
    } else {
        Modifier
    }
    Canvas(Modifier.fillMaxSize().then(pointer)) {
        val l = list ?: return@Canvas
        drawIntoCanvas { MotionPainter.paint(it.nativeCanvas, l, size.width, size.height, fit, typefaces = typefaces) }
    }
}

/**
 * The theme's sans / mono families as Android typefaces at a CSS weight
 * (through Compose's font resolver, the faces the text leaves use);
 * the platform faces when a family does not resolve to one.
 */
@Composable
private fun rememberCanvasTypefaces(model: SurfaceModel, theme: ThemeHandle?): MotionPainter.Typefaces {
    val resolver = LocalFontFamilyResolver.current
    val shaper = model.textShaper()
    return remember(resolver, shaper, theme) {
        val sans: FontFamily = shaper.family(theme?.sansFamily)
        val mono: FontFamily = shaper.family(theme?.monoFamily ?: "ui-monospace")
        val cache = HashMap<Pair<Boolean, Int>, Typeface?>()
        MotionPainter.Typefaces { isMono, weight ->
            cache.getOrPut(isMono to weight) {
                val family = if (isMono) mono else sans
                val resolved = runCatching { resolver.resolve(family, FontWeight(weight.coerceIn(1, 1000)), FontStyle.Normal, FontSynthesis.All).value }.getOrNull()
                resolved as? Typeface
            }
        }
    }
}

/**
 * A pointer Canvas's events through the core (`Surface.event`: it drops
 * them without `pointer: true` and fires the node's `on.<event>`). Moves
 * coalesce to one per frame (the latest point; a pending one goes out
 * before a press or a leave).
 */
private class CanvasPointer(val id: String) {
    var model: SurfaceModel? = null
    private var pending: List<Double>? = null
    private var scheduled = false

    fun move(scope: CoroutineScope, at: List<Double>) {
        if (at.size != 2) return
        pending = at
        if (scheduled) return
        scheduled = true
        scope.launch {
            androidx.compose.runtime.withFrameNanos { }
            scheduled = false
            flush()
        }
    }

    fun flush() {
        val p = pending ?: return
        pending = null
        send("move", p)
    }

    fun send(name: String, at: List<Double>?) {
        val m = model ?: return
        if (at != null && at.size != 2) return
        val i = m.indexOf(id) ?: return
        val payload = at?.let { JsonValue.Obj(mapOf("x" to JsonValue.Num(it[0]), "y" to JsonValue.Num(it[1]))).json }
        val events = try {
            m.surface.event(i.toUInt(), name, payload)
        } catch (_: UiException) {
            return
        }
        if (events.isNotEmpty()) m.dispatch(events)
    }
}
