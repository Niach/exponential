package at.exponential.ui.compose

import android.graphics.Bitmap
import androidx.compose.animation.core.withInfiniteAnimationFrameNanos
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.hoverable
import androidx.compose.foundation.interaction.HoverInteraction
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.input.pointer.pointerInput
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import at.exponential.ui.ffi.MotionPlayer
import at.exponential.ui.ffi.UiException
import at.exponential.ui.ffi.motionCompile
import at.exponential.ui.ffi.motionReadDotlottie
import at.exponential.ui.ffi.motionReadDotlottieMachine
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.dispatch
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.net.HttpURLConnection
import java.net.URL

/**
 * EXP-1604: a loaded Motion: the core's player (the shared clock + the packed display list) and its decoded images.
 * EXP-1605: `machine` = its state machine (`stateMachine`, else a `.lottie`'s first).
 */
class MotionSource(val player: MotionPlayer, val machine: String? = null) {
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

        /** An attached `.json` or `.lottie` (the first animation and state machine), ≤ 2 MB. */
        suspend fun fetch(url: String, headers: Map<String, String>): Pair<String, String?>? = withContext(Dispatchers.IO) {
            val conn = URL(url).openConnection() as HttpURLConnection
            try {
                headers.forEach { (k, v) -> conn.setRequestProperty(k, v) }
                val bytes = conn.inputStream.use { s -> s.readNBytes(2 * 1024 * 1024 + 1) }
                when {
                    bytes.size > 2 * 1024 * 1024 -> null
                    bytes.size > 1 && bytes[0] == 0x50.toByte() && bytes[1] == 0x4b.toByte() ->
                        motionReadDotlottie(bytes) to motionReadDotlottieMachine(bytes).ifEmpty { null }
                    else -> bytes.toString(Charsets.UTF_8) to null
                }
            } finally {
                conn.disconnect()
            }
        }
    }
}

/**
 * EXP-1604: `Motion` (contract §3–6): a `Canvas` painting the core's display
 * list for the clock's frame, one frame per display frame
 * (`withInfiniteAnimationFrameNanos`: tests treat it as idle; ≤ 60 fps); the
 * poster under reduced motion, `autoplay: false`, while the app is not
 * resumed, off-screen (not composed) and past the surface's first 8
 * animating Motions; a bound `progress` scrubs; `play: false` freezes.
 * EXP-1605: a state machine ([MotionDriver]) takes the pointer and drives
 * the playback; a pressable Motion is the node's clickable (NodeView), whose
 * press / pointer down / up reach the machine through the model.
 */
@Composable
internal fun MotionLeaf(cx: LeafContext) {
    val model = cx.model
    val props = cx.props
    val dark = cx.dark
    val theme = model.effectiveTheme
    val docKey = remember(props) { JsonValue.Obj((MotionSource.DOCUMENT_KEYS + listOf("lottie", "src", "slots", "stateMachine")).mapNotNull { k -> props[k]?.let { k to it } }.toMap()).json }
    val playback = remember(props) { MotionSource.playbackJson(props) }
    val source by produceState<MotionSource?>(null, docKey, dark, theme) {
        value = try {
            var lottie = MotionSource.inlineLottie(props)
            // README › Interactivity §1: `stateMachine`, else the .lottie's first.
            var machine = props["stateMachine"]?.takeIf { it.obj != null }?.json
            if (lottie == null) {
                val src = props.str("src")
                val request = if (src.isEmpty()) null else model.mediaRequest(src)
                if (request != null) MotionSource.fetch(request.url, request.headers)?.let { (l, m) ->
                    lottie = l
                    machine = machine ?: m
                }
            }
            lottie?.let { l ->
                val mode = model.mode
                val player = MotionPlayer(l, MotionSource.slots(l, props, { theme?.colorHex(it, mode) }, dark))
                MotionSource(player, machine)
            }
        } catch (_: Exception) {
            // A load failure paints the box empty (contract §1), never a crash.
            null
        }
    }
    val driver = remember(cx.node.id) { MotionDriver(cx.node.id) }
    // A running machine owns the playback (its state's).
    source?.player?.let { if (!(driver.running && driver.player === it)) it.setPlayback(playback) }
    DisposableEffect(driver, model) {
        driver.attach(model)
        driver.appeared()
        onDispose { driver.detach() }
    }
    LaunchedEffect(source) {
        driver.start(source?.player, source?.machine)
        driver.setInputs(props["inputs"])
    }
    val inputs = props["inputs"]
    LaunchedEffect(inputs?.json) { driver.setInputs(inputs) }

    val lifecycle = LocalLifecycleOwner.current.lifecycle
    var resumed by remember { mutableStateOf(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) }
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, _ -> resumed = lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer) }
    }

    val progress = props.num("progress")
    val play = props["play"]?.bool ?: true
    // A running machine's state owns autoplay (the clock holds its poster).
    val still = model.reducedMotion || (!driver.running && props["autoplay"]?.bool == false) || !resumed || !model.motionMayAnimate(cx.index)
    val elapsed = driver.elapsed
    LaunchedEffect(still) { if (still) driver.rewind() }
    val ticking = source != null && progress == null && !still && play && !driver.done
    if (ticking) {
        val generation = driver.generation
        LaunchedEffect(source, generation) {
            // Resume where `play: false` froze it.
            val player = source?.player ?: return@LaunchedEffect
            var last = -1L
            while (true) {
                val now = withInfiniteAnimationFrameNanos { it }
                if (last >= 0) elapsed.doubleValue += (now - last) / 1e6
                last = now
                if (player.tick(elapsed.doubleValue).done) {
                    driver.finished(generation)
                    break
                }
            }
        }
    }
    val hovers = remember(driver) { MutableInteractionSource() }
    LaunchedEffect(hovers) {
        hovers.interactions.collect { i ->
            when (i) {
                is HoverInteraction.Enter -> driver.hover(true)
                is HoverInteraction.Exit -> driver.hover(false)
            }
        }
    }
    // A machine without `on.press` (no clickable around it) takes the pointer itself.
    val pointer = if (driver.running && !cx.node.pressable) {
        Modifier.pointerInput(driver) {
            detectTapGestures(
                onPress = {
                    driver.post("pointerDown")
                    tryAwaitRelease()
                    driver.post("pointerUp")
                },
                onTap = { driver.post("click") },
            )
        }
    } else {
        Modifier
    }
    val fit = props.str("fit").ifEmpty { "contain" }
    Canvas(Modifier.fillMaxSize().hoverable(hovers).then(pointer)) {
        val s = source ?: return@Canvas
        val frame = when {
            progress != null -> s.player.progressFrame(progress.coerceIn(0.0, 1.0))
            still -> s.player.posterFrame()
            else -> s.player.tick(elapsed.doubleValue).frame
        }
        val list = MotionList.decode(s.player.sample(frame)) ?: return@Canvas
        drawIntoCanvas { MotionPainter.paint(it.nativeCanvas, list, size.width, size.height, fit, s.images) }
    }
}

/**
 * EXP-1605 (eui-motion README › Interactivity): a Motion's state machine on
 * the core's interpreter. Interactions go in (`sm_post`), inputs on change
 * (`sm_set_inputs`); every state change restarts the clock at 0;
 * `on.state` per entered state, `on.complete` once per state (its play
 * finished or it is final), `on.hover`, `on.visible` once.
 */
class MotionDriver(val id: String) {
    private var model: SurfaceModel? = null
    var player: MotionPlayer? = null
        private set
    /** A state machine drives the playback. */
    var running by mutableStateOf(false)
        private set
    /** The play finished: the last frame holds until the clock restarts. */
    var done by mutableStateOf(false)
        private set
    /** Bumped on every restart (a stale `done` report is dropped). */
    var generation by mutableIntStateOf(0)
        private set
    /** The ms since the clock started. */
    val elapsed = mutableDoubleStateOf(0.0)
    private var inputs: String? = null
    private var machine: String? = null
    /** No state change since the machine started (a reload of it stays quiet). */
    private var pristine = false
    private var completeSent = false
    private var seen = false

    fun attach(model: SurfaceModel) {
        this.model = model
        model.motionDrivers[id] = this
    }

    fun detach() {
        val m = model ?: return
        if (m.motionDrivers[id] === this) m.motionDrivers.remove(id)
    }

    /** A loaded player (null = nothing) and its machine (null = plain playback). */
    fun start(player: MotionPlayer?, machine: String?) {
        // A reload (theme, mode) of an untouched machine does not announce its initial state again.
        val quiet = pristine && machine != null && machine == this.machine
        this.player = player
        running = player != null && machine != null && player.setStateMachine(machine)
        if (player != null) this.machine = machine
        pristine = running
        inputs = null
        rewind()
        drain(quiet)
    }

    /** An interaction (click, pointerDown, pointerUp, pointerEnter, pointerExit, complete). */
    fun post(kind: String) {
        val p = player ?: return
        if (!running) return
        if (p.smPost(kind, "")) {
            pristine = false
            rewind()
        }
        drain()
    }

    /** The `inputs` prop (literal or bound), written on change. */
    fun setInputs(value: JsonValue?) {
        val p = player ?: return
        if (!running || value?.obj == null) return
        val json = value.json
        if (json == inputs) return
        inputs = json
        if (p.smSetInputs(json)) {
            pristine = false
            rewind()
        }
        drain()
    }

    /** The clock reported `done` for the play of [generation]. */
    fun finished(generation: Int) {
        if (generation != this.generation || done) return
        done = true
        if (!completeSent) {
            completeSent = true
            fire("complete")
        }
        post("complete")
    }

    fun hover(inside: Boolean) {
        post(if (inside) "pointerEnter" else "pointerExit")
        if (inside) fire("hover")
    }

    /** First composed: `on.visible`, once. */
    fun appeared() {
        if (seen) return
        seen = true
        fire("visible")
    }

    /** Restart the clock at 0. */
    fun rewind() {
        elapsed.doubleValue = 0.0
        done = false
        completeSent = false
        generation += 1
    }

    /** The states entered since the last read, and a final state. */
    private fun drain(quiet: Boolean = false) {
        val p = player ?: return
        if (!running) return
        val status = JsonValue.parse(p.smStatus())
        if (!quiet) status["entered"]?.array?.forEach { s ->
            s.string?.let { fire("state", JsonValue.Obj(mapOf("state" to JsonValue.Str(it)))) }
        }
        if (status["final"]?.bool == true && !completeSent) {
            completeSent = true
            fire("complete")
        }
    }

    /** The node's `on.<event>` through the core (nothing without a handler). */
    private fun fire(event: String, payload: JsonValue? = null) {
        val m = model ?: return
        val i = m.indexOf(id) ?: return
        val events = try {
            m.surface.event(i.toUInt(), event, payload?.json)
        } catch (_: UiException) {
            return
        }
        if (events.isNotEmpty()) m.dispatch(events)
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
