package com.exponential.app.ui.spike

import android.os.Handler
import android.os.Looper
import android.os.Trace
import android.util.Log
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlinx.coroutines.flow.MutableStateFlow
import org.json.JSONArray
import org.json.JSONObject
import uniffi.vapp_spike_ffi.AvailMode
import uniffi.vapp_spike_ffi.FfiSize
import uniffi.vapp_spike_ffi.FfiVisual
import uniffi.vapp_spike_ffi.Measure
import uniffi.vapp_spike_ffi.MeasureCall
import uniffi.vapp_spike_ffi.Surface
import uniffi.vapp_spike_ffi.benchTreeJson
import uniffi.vapp_spike_ffi.fixedIntrinsic
import uniffi.vapp_spike_ffi.kitchenSinkJson

private const val TAG = "VappSpike"

/** One tree node, props parsed ONCE. */
@Stable
class VappNodeModel(
    val index: Int,
    val id: String,
    val kind: String,
    val depth: Int,
    val parent: Int?,
    val isContainer: Boolean,
    val propsJson: String,
    val props: JSONObject,
) {
    fun str(key: String): String? = props.optString(key).takeIf { props.has(key) && it.isNotEmpty() }
    fun bool(key: String): Boolean = props.optBoolean(key, false)
    fun num(key: String, fallback: Double): Double = props.optDouble(key, fallback)
    fun strings(key: String): List<String> {
        val arr: JSONArray = props.optJSONArray(key) ?: return emptyList()
        return (0 until arr.length()).map { arr.optString(it) }
    }
}

/** The text style a frame carries (resolved by the core from variant + style). */
data class VappTextSpec(
    val fontSize: Float,
    val fontWeight: Int,
    val lineHeight: Float,
    val color: String?,
    val textAlign: String?,
)

/** One layout pass, published AFTER the pass. */
data class VappSample(
    val nodes: Int,
    val measureCalls: Int,
    val taffyNs: Long,
    val ffiNs: Long,
    val wallNs: Long,
    val buildNs: Long,
    val widthDp: Float,
    val heightDp: Float,
)

/** Identity of one measure request (the memo key). */
data class MeasureKey(
    val index: UInt,
    val knownWidth: Float?,
    val knownHeight: Float?,
    val availableWidth: Float?,
    val availableWidthMode: uniffi.vapp_spike_ffi.AvailMode,
) {
    constructor(c: MeasureCall) : this(c.index, c.knownWidth, c.knownHeight, c.availableWidth, c.availableWidthMode)
}

/** Frame of a node in PX, surface coordinates (for draw-phase clipping). */
data class PxRect(val x: Float, val y: Float, val w: Float, val h: Float)

@Stable
class VappSurfaceState(treeJson: String) {
    val surface: Surface = Surface(treeJson)
    val nodes: List<VappNodeModel> = surface.nodes().map {
        VappNodeModel(
            index = it.index.toInt(),
            id = it.id,
            kind = it.kind,
            depth = it.depth.toInt(),
            parent = it.parent?.toInt(),
            isContainer = it.isContainer,
            propsJson = it.propsJson,
            props = runCatching { JSONObject(it.propsJson) }.getOrDefault(JSONObject()),
        )
    }

    /** Pressed ids — read inside the measure block, so a change re-runs layout. */
    val pressed = mutableStateMapOf<String, Boolean>()

    /**
     * Text specs, seeded from the core's fixed-measure pass (font size/weight/line
     * height are style-derived, not size-derived, but only reach the host through
     * a PlacedFrame). Re-published after a real pass only if they changed.
     */
    var textSpecs: List<VappTextSpec> by mutableStateOf(emptyList())

    /** Written during measure, read in the draw phase (no recomposition). */
    val visuals: Array<FfiVisual?> = arrayOfNulls(nodes.size)
    val framesPx: Array<PxRect?> = arrayOfNulls(nodes.size)

    /** Read inside measure: bumping it forces one more layout pass (the bench taps). */
    var relayoutTick by mutableIntStateOf(0)

    /** Read inside measure WITHOUT the width nudge: one more pass on a warm taffy cache. */
    var cachedTick by mutableIntStateOf(0)

    /** Bumped (posted) after a pass whose visuals changed, so draw re-runs. */
    var visualVersion by mutableIntStateOf(0)

    val sample = MutableStateFlow<VappSample?>(null)

    /** Leaves whose intrinsics threw; they fall back to the core's fixed measure. */
    val intrinsicFallbacks = mutableSetOf<String>()

    val buildNs: Long = surface.buildNs().toLong()

    /** Host-side per-pass measure memo (on by default; the bench can flip it). */
    val memoEnabled: Boolean get() = DevScreens.hostMemo

    private val main = Handler(Looper.getMainLooper())

    init {
        surface.setRounding(false)
        surface.setViewport(390f, 0f)
        textSpecs = specsOf(surface.layoutFixed().frames)
        // No-callback baseline on a throwaway surface: the core's FixedMeasure
        // runs inside Rust, so layout_ns here = taffy without any JNA round trip.
        Surface(treeJson).use { probe ->
            probe.setRounding(false)
            probe.setViewport(411f, 0f)
            repeat(3) { run ->
                probe.setViewport(if (run % 2 == 0) 411f else 410.99f, 0f)
                val r = probe.layoutFixed()
                Log.i(TAG, "fixed_baseline run=$run nodes=${nodes.size} calls=${r.measureCalls} taffy_ns=${r.layoutNs}")
            }
        }
    }

    fun post(block: () -> Unit) {
        main.post(block)
    }

    fun specsOf(frames: List<uniffi.vapp_spike_ffi.PlacedFrame>): List<VappTextSpec> {
        val out = arrayOfNulls<VappTextSpec>(nodes.size)
        for (f in frames) {
            out[f.index.toInt()] = VappTextSpec(
                fontSize = f.fontSize,
                fontWeight = f.fontWeight.toInt(),
                lineHeight = f.lineHeight,
                color = f.visual.color,
                textAlign = f.visual.textAlign,
            )
        }
        return out.map { it ?: VappTextSpec(14f, 400, 20f, null, null) }
    }

    fun close() {
        surface.close()
    }
}

/** [bench] = node count of the synthetic bench tree, null = the kitchen sink. */
fun vappTreeJson(bench: Int?, rtl: Boolean): String {
    val json = if (bench != null && bench > 0) benchTreeJson(bench.toUInt()) else kitchenSinkJson()
    if (!rtl) return json
    // The core reads the surface direction from the ROOT style only.
    val root = JSONObject(json)
    val style = root.optJSONObject("style") ?: JSONObject().also { root.put("style", it) }
    style.put("direction", "rtl")
    return root.toString()
}

@Composable
fun rememberVappSurfaceState(bench: Int?, rtl: Boolean): VappSurfaceState {
    val state = remember(bench, rtl) { VappSurfaceState(vappTreeJson(bench, rtl)) }
    DisposableEffect(state) { onDispose { state.close() } }
    return state
}

/**
 * The taffy-driven surface: ONE Layout whose children are every node of the
 * tree (containers included, as their own background boxes), in pre-order, so
 * composition order = paint order = semantics order.
 */
@Composable
fun VappSurface(state: VappSurfaceState, modifier: Modifier = Modifier) {
    Layout(
        content = {
            state.nodes.forEach { node -> key(node.id) { VappNode(node, state) } }
        },
        // The traversal group: its children's traversalIndex (= node index) wins
        // over the geometric sort, so TalkBack reads fixture pre-order.
        modifier = modifier.semantics { isTraversalGroup = true },
    ) { measurables, constraints ->
        val t0 = System.nanoTime()
        Trace.beginSection("vapp.layout")
        try {
            val d = density
            val surface = state.surface
            surface.setRounding(false)
            // A bench tap (odd tick) nudges the width by 0.01 dp: taffy caches every
            // leaf measurement and the FFI has no mark_dirty, so an unchanged
            // viewport re-runs with ZERO measure calls. The nudge = a full
            // re-measure pass (the steady-state number worth reading).
            val tick = state.relayoutTick
            @Suppress("UNUSED_VARIABLE") val cached = state.cachedTick
            val widthDp = (if (constraints.hasBoundedWidth) constraints.maxWidth / d else 390f) -
                (if (tick % 2 == 1) 0.01f else 0f)
            surface.setViewport(widthDp, 0f)
            surface.setPressed(state.pressed.keys.toList())

            fun fallback(i: Int): FfiSize {
                val n = state.nodes[i]
                return fixedIntrinsic(n.kind, n.propsJson)
            }

            // Per-pass memo: taffy asks the same leaf the same question several
            // times (flex + grid probing), and every Compose intrinsic query
            // re-runs text layout. Keyed on the exact request.
            val memo = HashMap<MeasureKey, FfiSize>()
            var hostHits = 0
            // Time spent INSIDE the Kotlin callback (intrinsics); taffy_ns minus
            // this = the core + the JNA callback round trips.
            var hostNs = 0L
            val measure = object : Measure {
                override fun measure(call: MeasureCall): FfiSize {
                    val h0 = System.nanoTime()
                    try {
                        return measureMemo(call)
                    } finally {
                        hostNs += System.nanoTime() - h0
                    }
                }

                fun measureMemo(call: MeasureCall): FfiSize {
                    if (state.memoEnabled) {
                        val key = MeasureKey(call)
                        memo[key]?.let { hostHits++; return it }
                        return measureUncached(call).also { memo[key] = it }
                    }
                    return measureUncached(call)
                }

                fun measureUncached(call: MeasureCall): FfiSize {
                    val i = call.index.toInt()
                    val m: IntrinsicMeasurable = measurables[i]
                    return try {
                        val wPx: Int = when {
                            call.knownWidth != null -> (call.knownWidth!! * d).roundToInt()
                            call.availableWidthMode == AvailMode.MIN_CONTENT ->
                                m.minIntrinsicWidth(Constraints.Infinity)
                            call.availableWidthMode == AvailMode.MAX_CONTENT ->
                                m.maxIntrinsicWidth(Constraints.Infinity)
                            else -> min(
                                m.maxIntrinsicWidth(Constraints.Infinity),
                                ((call.availableWidth ?: Float.MAX_VALUE / d) * d).roundToInt().coerceAtLeast(0),
                            )
                        }
                        val width = call.knownWidth ?: (wPx / d)
                        val height = call.knownHeight ?: (m.minIntrinsicHeight(wPx) / d)
                        FfiSize(width, height)
                    } catch (e: IllegalStateException) {
                        // SubcomposeLayout / LazyList leaves refuse intrinsics.
                        val id = state.nodes[i].id
                        if (state.intrinsicFallbacks.add(id)) {
                            Log.w(TAG, "intrinsics unsupported for $id (${state.nodes[i].kind}): ${e.message}")
                        }
                        val f = fallback(i)
                        FfiSize(call.knownWidth ?: f.width, call.knownHeight ?: f.height)
                    }
                }
            }

            val tFfi = System.nanoTime()
            val result = surface.layout(measure)
            val ffiNs = System.nanoTime() - tFfi

            val placeables = arrayOfNulls<Placeable>(measurables.size)
            val xs = IntArray(measurables.size)
            val ys = IntArray(measurables.size)
            val alphas = FloatArray(measurables.size) { 1f }
            var visualsChanged = false
            for (f in result.frames) {
                val i = f.index.toInt()
                if (i >= measurables.size) continue
                val wPx = max(0, (f.width * d).roundToInt())
                val hPx = max(0, (f.height * d).roundToInt())
                placeables[i] = measurables[i].measure(Constraints.fixed(wPx, hPx))
                xs[i] = (f.x * d).roundToInt()
                ys[i] = (f.y * d).roundToInt()
                alphas[i] = f.visual.opacity ?: 1f
                if (state.visuals[i] != f.visual) {
                    state.visuals[i] = f.visual
                    visualsChanged = true
                }
                state.framesPx[i] = PxRect(xs[i].toFloat(), ys[i].toFloat(), wPx.toFloat(), hPx.toFloat())
            }
            val specs = state.specsOf(result.frames)
            val surfaceW = (result.surfaceWidth * d).roundToInt().coerceIn(constraints.minWidth, constraints.maxWidth)
            val surfaceH = (result.surfaceHeight * d).roundToInt().coerceAtLeast(constraints.minHeight)
            val wallNs = System.nanoTime() - t0
            val sample = VappSample(
                nodes = state.nodes.size,
                measureCalls = result.measureCalls.toInt(),
                taffyNs = result.layoutNs.toLong(),
                ffiNs = ffiNs,
                wallNs = wallNs,
                buildNs = state.buildNs,
                widthDp = widthDp,
                heightDp = result.surfaceHeight,
            )
            Log.i(
                TAG,
                "nodes=${sample.nodes} calls=${sample.measureCalls} taffy_ns=${sample.taffyNs} " +
                    "ffi_ns=${sample.ffiNs} wall_ns=${sample.wallNs} build_ns=${sample.buildNs} " +
                    "width_dp=${"%.1f".format(widthDp)} height_dp=${"%.1f".format(result.surfaceHeight)} " +
                    "density=$d memo=${state.memoEnabled} host_hits=$hostHits host_ns=$hostNs " +
                    "fallbacks=${state.intrinsicFallbacks}",
            )
            // Published after the pass, never from inside it.
            state.post {
                state.sample.value = sample
                if (visualsChanged) state.visualVersion++
                if (specs != state.textSpecs) state.textSpecs = specs
            }
            layout(surfaceW, surfaceH) {
                for (f in result.frames) {
                    val i = f.index.toInt()
                    val p = placeables[i] ?: continue
                    // `place`, not `placeRelative`: taffy already mirrored under RTL.
                    if (alphas[i] < 1f) {
                        val a = alphas[i]
                        p.placeWithLayer(xs[i], ys[i]) { alpha = a }
                    } else {
                        p.place(xs[i], ys[i])
                    }
                }
            }
        } finally {
            Trace.endSection()
        }
    }
}

// --- colours -----------------------------------------------------------------

private val paletteTokens: Map<String, Color> by lazy {
    val p = com.exponential.app.ui.theme.DesignTokens.Palette
    mapOf(
        "background" to p.Background, "foreground" to p.Foreground, "card" to p.Card,
        "cardforeground" to p.CardForeground, "popover" to p.Popover, "primary" to p.Primary,
        "primaryforeground" to p.PrimaryForeground, "secondary" to p.Secondary,
        "secondaryforeground" to p.SecondaryForeground, "muted" to p.Muted,
        "mutedforeground" to p.MutedForeground, "accent" to p.Accent,
        "accentforeground" to p.AccentForeground, "destructive" to p.Destructive,
        "border" to p.Border, "input" to p.Input, "ring" to p.Ring,
    )
}

private val semanticTokens: Map<String, Color> by lazy {
    val s = com.exponential.app.ui.theme.DesignTokens.Semantic
    mapOf(
        "neutral" to s.Neutral, "yellow" to s.Yellow, "green" to s.Green, "red" to s.Red,
        "orange" to s.Orange, "blue" to s.Blue,
    )
}

/** `$palette.x` / `$semantic.x` → DesignTokens; `#rrggbb` → parsed. */
fun vappColor(raw: String?): Color? {
    if (raw.isNullOrBlank()) return null
    if (raw.startsWith("$")) {
        val parts = raw.removePrefix("$").split('.', limit = 2)
        if (parts.size != 2) return null
        val name = parts[1].replace("-", "").lowercase()
        return when (parts[0]) {
            "palette" -> paletteTokens[name]
            "semantic" -> semanticTokens[name]
            else -> null
        }
    }
    return runCatching { Color(android.graphics.Color.parseColor(raw)) }.getOrNull()
}
