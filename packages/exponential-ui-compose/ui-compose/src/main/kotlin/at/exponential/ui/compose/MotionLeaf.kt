package at.exponential.ui.compose

import android.graphics.Bitmap
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import at.exponential.ui.ffi.MotionPlayer
import at.exponential.ui.ffi.motionCompile
import at.exponential.ui.ffi.motionReadDotlottie
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/** EXP-1604: a loaded Motion: the core's player (the shared clock + the packed display list) and its decoded images. */
class MotionSource(val player: MotionPlayer) {
    val images: List<Bitmap?> = player.images().map { MotionPainter.decodeDataUrl(it) }

    companion object {
        /** The props that make the document (contract §1: `layers` + these). */
        val DOCUMENT_KEYS = listOf("size", "fps", "duration", "start", "time", "layers", "assets", "markers")

        /** The playback props (`set_playback`). */
        val PLAYBACK_KEYS = listOf("autoplay", "loop", "plays", "direction", "speed", "segment", "marker", "poster")

        /** Lottie JSON from `layers` (compiled) or `lottie`; null for `src` or nothing. */
        fun inlineLottie(props: Props): String? {
            if (props["layers"]?.array != null) {
                val doc = DOCUMENT_KEYS.mapNotNull { k -> props[k]?.let { k to it } }.toMap()
                return motionCompile(JsonValue.Obj(doc).json)
            }
            return props["lottie"]?.takeIf { it.obj != null }?.json
        }

        /**
         * The slot overrides (contract §2): every `$color.<token>` slot of the
         * file gets the theme's colour, `$mode` the mode, the node's `slots` last.
         */
        fun slots(lottie: String, props: Props, color: (String) -> String?, dark: Boolean): String {
            val out = LinkedHashMap<String, JsonValue>()
            for (sid in JsonValue.parse(lottie)["slots"]?.obj?.keys ?: emptySet()) {
                if (!sid.startsWith("\$color.")) continue
                val c = color(sid.removePrefix("\$color."))?.let(::rgba) ?: continue
                out[sid] = JsonValue.Obj(mapOf("p" to JsonValue.Obj(mapOf("a" to JsonValue.Num(0.0), "k" to JsonValue.Arr(c.map { JsonValue.Num(it) })))))
            }
            if (dark) out["\$mode"] = JsonValue.Str("dark")
            props["slots"]?.obj?.let { out.putAll(it) }
            return JsonValue.Obj(out).json
        }

        /** `#rgb`, `#rrggbb`, `#rrggbbaa` → [r, g, b, a] in 0…1. */
        fun rgba(hex: String): List<Double>? {
            var h = hex.trim()
            if (!h.startsWith("#")) return null
            h = h.substring(1)
            if (h.length == 3 || h.length == 4) h = h.map { "$it$it" }.joinToString("")
            if (h.length != 6 && h.length != 8) return null
            val v = h.toLongOrNull(16) ?: return null
            val n = if (h.length == 8) v else (v shl 8) or 0xff
            return listOf(24, 16, 8, 0).map { ((n shr it) and 0xff) / 255.0 }
        }

        fun playbackJson(props: Props): String = JsonValue.Obj(PLAYBACK_KEYS.mapNotNull { k -> props[k]?.let { k to it } }.toMap()).json

        /** An attached `.json` or `.lottie` (the first animation), ≤ 2 MB. */
        suspend fun fetch(url: String, headers: Map<String, String>): String? = withContext(Dispatchers.IO) {
            val conn = URL(url).openConnection() as HttpURLConnection
            try {
                headers.forEach { (k, v) -> conn.setRequestProperty(k, v) }
                val bytes = conn.inputStream.use { s -> s.readNBytes(2 * 1024 * 1024 + 1) }
                when {
                    bytes.size > 2 * 1024 * 1024 -> null
                    bytes.size > 1 && bytes[0] == 0x50.toByte() && bytes[1] == 0x4b.toByte() -> motionReadDotlottie(bytes)
                    else -> bytes.toString(Charsets.UTF_8)
                }
            } finally {
                conn.disconnect()
            }
        }
    }
}

/**
 * EXP-1604: `Motion` (contract §3–6): a `Canvas` painting the core's display
 * list for the clock's frame, one frame per `withFrameNanos` (≤ 60 fps); the
 * poster under reduced motion, `autoplay: false`, while the app is not
 * resumed, off-screen (not composed) and past the surface's first 8
 * animating Motions; a bound `progress` scrubs; `play: false` freezes.
 */
@Composable
internal fun MotionLeaf(cx: LeafContext) {
    val model = cx.model
    val props = cx.props
    val dark = cx.dark
    val theme = model.effectiveTheme
    val docKey = remember(props) { JsonValue.Obj((MotionSource.DOCUMENT_KEYS + listOf("lottie", "src", "slots")).mapNotNull { k -> props[k]?.let { k to it } }.toMap()).json }
    val playback = remember(props) { MotionSource.playbackJson(props) }
    val source by produceState<MotionSource?>(null, docKey, dark, theme) {
        value = try {
            var lottie = MotionSource.inlineLottie(props)
            if (lottie == null) {
                val src = props.str("src")
                val request = if (src.isEmpty()) null else model.mediaRequest(src)
                if (request != null) lottie = MotionSource.fetch(request.url, request.headers)
            }
            lottie?.let { l ->
                val mode = model.mode
                val player = MotionPlayer(l, MotionSource.slots(l, props, { theme?.colorHex(it, mode) }, dark))
                MotionSource(player)
            }
        } catch (_: Exception) {
            // A load failure paints the box empty (contract §1), never a crash.
            null
        }
    }
    source?.player?.setPlayback(playback)

    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var resumed by remember { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ -> resumed = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }

    val progress = props.num("progress")
    val play = props["play"]?.bool ?: true
    val still = model.reducedMotion || props["autoplay"]?.bool == false || !resumed || !model.motionMayAnimate(cx.index)
    val elapsed = remember { mutableDoubleStateOf(0.0) }
    var done by remember(source, still) { mutableStateOf(false) }
    LaunchedEffect(still) { if (still) elapsed.doubleValue = 0.0 }
    val ticking = source != null && progress == null && !still && play && !done
    if (ticking) {
        LaunchedEffect(source) {
            // Resume where `play: false` froze it.
            var last = -1L
            while (true) {
                val now = withFrameNanos { it }
                if (last >= 0) elapsed.doubleValue += (now - last) / 1e6
                last = now
            }
        }
    }
    val fit = props.str("fit").ifEmpty { "contain" }
    Canvas(Modifier.fillMaxSize()) {
        val s = source ?: return@Canvas
        val frame = when {
            progress != null -> s.player.progressFrame(progress.coerceIn(0.0, 1.0))
            still -> s.player.posterFrame()
            else -> s.player.tick(elapsed.doubleValue).also { if (it.done && ticking) done = true }.frame
        }
        val list = MotionList.decode(s.player.sample(frame)) ?: return@Canvas
        drawIntoCanvas { MotionPainter.paint(it.nativeCanvas, list, size.width, size.height, fit, s.images) }
    }
}

/**
 * At most 8 Motions animate per surface, in tree order (contract §3): the
 * ones that would play (no `progress`, `autoplay` not false).
 */
internal fun SurfaceModel.motionMayAnimate(index: Int): Boolean {
    var before = 0
    for (n in nodes) {
        if (n.component != "Motion" || n.index >= index) continue
        if (n.props["progress"]?.number == null && n.props["autoplay"]?.bool != false) before++
        if (before >= 8) return false
    }
    return true
}

/** The composition [w, h] of `size` or the `lottie`'s `w` / `h`; within [wrap]. */
internal fun motionBox(props: Props, wrap: Float?): androidx.compose.ui.geometry.Size {
    val size = props["size"]?.array?.mapNotNull { it.number } ?: emptyList()
    val w = (if (size.size == 2) size[0] else props["lottie"]?.get("w")?.number ?: 0.0).toFloat()
    val h = (if (size.size == 2) size[1] else props["lottie"]?.get("h")?.number ?: 0.0).toFloat()
    if (w <= 0f || h <= 0f) return androidx.compose.ui.geometry.Size.Zero
    val width = minOf(w, maxOf(0f, wrap ?: w))
    return androidx.compose.ui.geometry.Size(width, width * h / w)
}
