package at.exponential.ui.compose

import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import at.exponential.ui.ffi.voiceWaveJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.NodeInfo
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.primitives.VoiceWaveRing
import at.exponential.ui.primitives.drawVoiceWave
import kotlin.math.min

/** One ring of the core's answer: its recipe part and its points (dp). */
internal class VoiceWaveRingSpec(
    val part: String,
    val points: FloatArray,
    val fillAlpha: Float,
    val strokeAlpha: Float,
    val strokeWidth: Float,
)

/**
 * NEW-77: one frame of the `VoiceWave` native from the core
 * (`voiceWaveJson`, catalog/style.json `voiceWave`): three closed waves,
 * the centre child's edge (0 = not painted) and the phase.
 */
internal class VoiceWaveSpec(val rings: List<VoiceWaveRingSpec>, val childSize: Float, val phase: Float) {
    companion object {
        val ACTIVITIES = setOf("idle", "listening", "thinking", "speaking", "muted")

        /** Does [activity] move (the phase advances with time)? */
        fun animates(activity: String): Boolean = activity == "listening" || activity == "thinking" || activity == "speaking"

        /** The facade's input: `{size, activity, level, bands, elapsed, reducedMotion}`. */
        fun input(size: Float, activity: String, level: Double, bands: List<Double>, elapsed: Double, reducedMotion: Boolean): String =
            JsonValue.Obj(
                linkedMapOf(
                    "size" to JsonValue.Num(size.toDouble()),
                    "activity" to JsonValue.Str(activity),
                    "level" to JsonValue.Num(level),
                    "bands" to JsonValue.Arr(bands.map { JsonValue.Num(it) }),
                    "elapsed" to JsonValue.Num(elapsed),
                    "reducedMotion" to JsonValue.Bool(reducedMotion),
                ),
            ).orderedJson

        /** The frame for [inputJson]; null when the core refuses it. */
        fun of(inputJson: String): VoiceWaveSpec? = try {
            parse(JsonValue.parse(voiceWaveJson(inputJson)))
        } catch (_: Throwable) {
            null
        }

        fun parse(v: JsonValue): VoiceWaveSpec? {
            val rings = v["rings"]?.array ?: return null
            return VoiceWaveSpec(
                rings = rings.map { r ->
                    val pts = r["points"]?.array ?: emptyList()
                    val flat = FloatArray(pts.size * 2)
                    pts.forEachIndexed { i, p ->
                        val xy = p.array
                        flat[2 * i] = (xy?.getOrNull(0)?.number ?: 0.0).toFloat()
                        flat[2 * i + 1] = (xy?.getOrNull(1)?.number ?: 0.0).toFloat()
                    }
                    VoiceWaveRingSpec(
                        part = r["part"]?.string ?: "outer",
                        points = flat,
                        fillAlpha = (r["fillAlpha"]?.number ?: 0.0).toFloat(),
                        strokeAlpha = (r["strokeAlpha"]?.number ?: 0.0).toFloat(),
                        strokeWidth = (r["strokeWidth"]?.number ?: 1.0).toFloat(),
                    )
                },
                childSize = (v["childSize"]?.number ?: 0.0).toFloat(),
                phase = (v["phase"]?.number ?: 0.0).toFloat(),
            )
        }
    }
}

/** The props a VoiceWave paints with, catalog defaults filled (activity idle, size md, level 0). */
internal data class VoiceWaveProps(val activity: String, val size: String, val level: Double, val bands: List<Double>) {
    /** The recipe props (`activity` + `size`). */
    val recipeProps: Props get() = mapOf("activity" to JsonValue.Str(activity), "size" to JsonValue.Str(size))

    companion object {
        fun of(props: Props): VoiceWaveProps = VoiceWaveProps(
            activity = props.str("activity").takeIf { it in VoiceWaveSpec.ACTIVITIES } ?: "idle",
            size = props.str("size").ifEmpty { "md" },
            level = props.num("level") ?: 0.0,
            bands = props.list("bands").mapNotNull { it.number },
        )
    }
}

/**
 * A `VoiceWave` (NEW-77): the three waves the core computes for the box
 * edge of the root recipe, each filled / stroked in its part colour
 * (`VoiceWave/outer|mid|inner`), animated (elapsed seconds, one core frame
 * per display frame) while listening, thinking or speaking unless reduced
 * motion; the root recipe's opacity is the box's (`paintedBox`). The ONE
 * child sits centred in a `childSize` square (not painted at `sm`).
 */
@Composable
internal fun VoiceWaveContent(index: Int, node: NodeInfo, model: SurfaceModel, size: Size) {
    val p = VoiceWaveProps.of(node.props)
    val mode = model.mode
    val edge = model.part("VoiceWave", "root", p.recipeProps).width ?: min(size.width, size.height)
    val colors = remember(p.activity, p.size, mode, model.effectiveTheme) {
        listOf("outer", "mid", "inner").associateWith { part ->
            model.part("VoiceWave", part, p.recipeProps).color ?: model.color("primary") ?: model.ink(index)
        }
    }
    val still = model.reducedMotion || !VoiceWaveSpec.animates(p.activity)
    val elapsed = remember(node.id) { mutableFloatStateOf(0f) }
    if (!still) {
        LaunchedEffect(node.id, p.activity) {
            var start = -1L
            while (true) {
                val now = withInfiniteAnimationFrameNanos { it }
                if (start < 0) start = now
                elapsed.floatValue = (now - start) / 1e9f
            }
        }
    }
    val rest = remember(edge, p, model.reducedMotion) {
        VoiceWaveSpec.of(VoiceWaveSpec.input(edge, p.activity, p.level, p.bands, 0.0, model.reducedMotion))
    }
    if (rest == null) {
        SideEffect { model.paintFailed(node.id, "VoiceWave: the core refused the wave") }
        return
    }
    val origin = Offset((size.width - edge) / 2f, (size.height - edge) / 2f)
    Canvas(Modifier.fillMaxSize()) {
        // Read in the draw phase: a new frame redraws, never recomposes.
        val t = if (still) 0f else elapsed.floatValue
        val spec = if (t == 0f) rest else VoiceWaveSpec.of(VoiceWaveSpec.input(edge, p.activity, p.level, p.bands, t.toDouble(), model.reducedMotion)) ?: rest
        val rings = spec.rings.map { r -> VoiceWaveRing(r.points, colors[r.part] ?: Color.Unspecified, r.fillAlpha, r.strokeAlpha, r.strokeWidth) }
        drawVoiceWave(rings, Offset(origin.x * density, origin.y * density))
    }
    // At `sm` (childSize 0) the child is composed at an empty box: nothing
    // shows, yet the surface's tree stays whole (catalog suite: every
    // visible node painted).
    val child = rest.childSize.coerceAtLeast(0f)
    val kids = (model.children.getOrNull(index) ?: emptyList()).filter { model.node(it)?.hidden == false }
    if (kids.isEmpty()) return
    val box = Rect(Offset((size.width - child) / 2f, (size.height - child) / 2f), Size(child, child))
    val ink = model.ink(index)
    val childInk = model.part("VoiceWave", "child", p.recipeProps).color
    CompositionLocalProvider(LocalInkSwap provides childInk?.let { ink to it }) {
        FrameLayout(size = size, frames = kids.associateWith { box }) {
            for (k in kids) {
                key(k) { NodeView(k, if (child > 0f) Modifier.frameIndex(k) else Modifier.frameIndex(k).clipToBounds()) }
            }
        }
    }
}
