package at.exponential.ui.compose

import androidx.compose.animation.core.CubicBezierEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.absoluteOffset
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import at.exponential.ui.ffi.qrMatrixJson
import at.exponential.ui.ffi.segmentedBadgeText
import at.exponential.ui.ffi.voiceWaveBarsJson
import at.exponential.ui.json.JsonValue
import at.exponential.ui.json.Props
import at.exponential.ui.json.flag
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.shownText
import at.exponential.ui.json.str
import at.exponential.ui.model.SurfaceModel
import at.exponential.ui.model.announce
import at.exponential.ui.model.fire
import at.exponential.ui.theme.ResolvedTextStyle
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

// VAPP-162 (docs/round-5-contract.md): the round-5 natives of the Compose
// painter: QrCode (§6, the core's matrix), QrScanner (§7, the camera is the
// HOST's through [LocalQrScanner]), the VoiceWave `bars` meter (§5) and the
// Segmented count badge (§3).

// ---------------------------------------------------------------------------
// QrCode (§6)
// ---------------------------------------------------------------------------

/** catalog/style.json `qrCode` / `qrScanner` (the numbers the painters share). */
internal object Round5Style {
    const val QR_QUIET_ZONE = 4
    const val QR_LABEL_GAP = 8f
    const val QR_DEFAULT_SIZE = 160f
    const val SCANNER_MAX_WIDTH = 280f
    const val SCANNER_HINT_GAP = 12f
    const val CORNER_LENGTH = 24f
    const val CORNER_STROKE = 3f
    const val FEED_ALPHA = 0.06f
    const val SCAN_LINE_HEIGHT = 2f
    const val SCAN_LINE_INSET = 16f
    const val SWEEP_MS = 2000
    const val PLACEHOLDER_ICON = "ui-scan"
    const val PLACEHOLDER_ICON_SIZE = 32f
}

/** The core's QR matrix (`qrMatrixJson`): `size` rows of '1' (dark) / '0', no quiet zone. */
internal class QrMatrix(val size: Int, val version: Int, val mask: Int, val rows: List<String>) {
    fun dark(x: Int, y: Int): Boolean = rows.getOrNull(y)?.getOrNull(x) == '1'

    companion object {
        /** The matrix of [value] at [level] (an unknown level is M); null = no matrix (empty, too long). */
        fun of(value: String, level: String): QrMatrix? = runCatching { parse(JsonValue.parse(qrMatrixJson(value, level))) }.getOrNull()

        fun parse(v: JsonValue): QrMatrix? {
            val rows = v["rows"]?.array?.mapNotNull { it.string } ?: return null
            return QrMatrix((v["size"]?.number ?: rows.size.toDouble()).toInt(), (v["version"]?.number ?: 0.0).toInt(), (v["mask"]?.number ?: 0.0).toInt(), rows)
        }
    }
}

/** THE SWAP (§6): of the plate and module colours the lighter fills the plate, the darker the modules. */
internal fun qrColours(plate: Color, module: Color): Pair<Color, Color> =
    if (plate.luminance() < module.luminance()) module to plate else plate to module

/** A `$string.<key>` the reducer left in place (or nothing) → the built-in string. */
internal fun stringProp(model: SurfaceModel, raw: String, fallbackKey: String): String = when {
    raw.isEmpty() -> model.string(fallbackKey)
    raw.startsWith("\$string.") -> model.string(raw.removePrefix("\$string."))
    else -> raw
}

/**
 * Paint [m]'s dark modules on a plate of [edgePx] px at [origin] (px), quiet
 * zone [Round5Style.QR_QUIET_ZONE]: every run of dark modules in a row is one
 * rect whose EDGES snap to device pixels (the module itself stays
 * fractional, so the outer size is exact).
 */
internal fun DrawScope.drawQrModules(m: QrMatrix, origin: Offset, edgePx: Float, color: Color) {
    val n = m.size + 2 * Round5Style.QR_QUIET_ZONE
    if (n <= 0) return
    val module = edgePx / n
    fun snapX(k: Int) = (origin.x + k * module).roundToInt().toFloat()
    fun snapY(k: Int) = (origin.y + k * module).roundToInt().toFloat()
    val q = Round5Style.QR_QUIET_ZONE
    for (y in 0 until m.size) {
        val row = m.rows.getOrNull(y) ?: continue
        val top = snapY(y + q)
        val bottom = snapY(y + q + 1)
        var x = 0
        while (x < row.length) {
            if (row[x] != '1') {
                x += 1
                continue
            }
            var end = x
            while (end < row.length && row[end] == '1') end += 1
            val left = snapX(x + q)
            val right = snapX(end + q)
            drawRect(color, Offset(left, top), Size(right - left, bottom - top))
            x = end
        }
    }
}

/** The QrCode label's text style (the `QrCode/label` recipe: caption, muted). */
internal fun qrLabelStyle(model: SurfaceModel, props: Props): ResolvedTextStyle {
    val p = model.part("QrCode", "label", props)
    return ResolvedTextStyle(p.px("fontSize") ?: 12f, (p.props["fontWeight"]?.number ?: 400.0).toInt(), p.px("lineHeight") ?: 16f, p.fontFamily)
}

/**
 * A `QrCode` (§6): the `size` px plate (the plate recipe's fill and
 * radius) centred at the top of the leaf, the core's matrix inside a
 * 4-module quiet zone, dark on light ALWAYS ([qrColours]); an empty value
 * (or one too long) = the empty plate. The label is a muted caption
 * centred under it (and the image's accessibility label, `nodeSemantics`).
 */
@Composable
internal fun QrCodeLeaf(cx: LeafContext) {
    val props = cx.props
    val edge = max((props.num("size") ?: Round5Style.QR_DEFAULT_SIZE.toDouble()).toFloat(), 0f)
    val value = props.shownText("value")
    val level = props.str("level").ifEmpty { "M" }
    val matrix = remember(value, level) { if (value.isEmpty()) null else QrMatrix.of(value, level) }
    val plate = cx.part("QrCode", "plate", props)
    val plateFill = plate.style.background ?: Color.White
    val moduleFill = cx.part("QrCode", "module", props).color ?: Color.Black
    val (light, dark) = qrColours(plateFill, moduleFill)
    val radius = plate.style.radius
    val label = props.shownText("label")
    val gap = cx.style.gap.takeIf { it > 0f } ?: Round5Style.QR_LABEL_GAP
    LeafFrame {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(gap.dp)) {
            Canvas(Modifier.size(edge.dp)) {
                val e = edge * density
                drawRoundRect(light, size = Size(e, e), cornerRadius = CornerRadius(radius * density))
                if (matrix != null) drawQrModules(matrix, Offset.Zero, e, dark)
            }
            if (label.isNotEmpty()) {
                val ts = qrLabelStyle(cx.model, props)
                val ink = cx.part("QrCode", "label", props).color ?: cx.ink
                BasicText(cx.shown(label), Modifier.fillMaxWidth(), style = cx.composeTextStyle(ts = ts, color = ink, align = TextAlign.Center))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// QrScanner (§7): the host camera
// ---------------------------------------------------------------------------

/**
 * VAPP-162: THE Compose camera hook of a `QrScanner` (round-5 contract §7;
 * never catalog/host.json): the HOST owns the camera (CameraX stays in the
 * app, not this library). Provide one through [LocalQrScanner] around the
 * surface; without one every scanner paints its placeholder (the scan icon
 * and `Camera unavailable`).
 */
interface QrScannerHost {
    /** False = no camera now (no permission, no device): the placeholder shows. */
    val available: Boolean get() = true

    /**
     * The live feed, filling [modifier] (the viewfinder square, clipped to
     * its radius). Decode codes of [formats] (default `["qr"]`) and call
     * [report] with every decoded string ON THE MAIN THREAD; the renderer
     * dispatches `scanned {value}` once per distinct value (until the value
     * changes or the scanner remounts). Leaving the composition (unmount,
     * `disabled`) = stop the camera.
     */
    @Composable
    fun Feed(formats: List<String>, report: (String) -> Unit, modifier: Modifier)
}

/** The host camera every QrScanner below paints its feed with (null = the placeholder). */
val LocalQrScanner = staticCompositionLocalOf<QrScannerHost?> { null }

/** The formats a scanner asks for (the catalog default `["qr"]`). */
internal fun qrScanFormats(props: Props): List<String> = props.list("formats").mapNotNull { it.string?.takeIf(String::isNotEmpty) }.ifEmpty { listOf("qr") }

/** One reporter per mounted scanner: forwards a value only when it differs from the last one reported. */
internal class QrScanReporter(private val report: (String) -> Unit) {
    private var last: String? = null

    fun accept(value: String) {
        if (value.isEmpty() || value == last) return
        last = value
        report(value)
    }
}

/** The viewfinder edge (dp) of a scanner [width] dp wide. */
internal fun scannerViewfinder(width: Float): Float = max(0f, min(width, Round5Style.SCANNER_MAX_WIDTH))

/** One corner bracket (two strokes, round caps) whose outer edge follows the viewfinder [radius] (dp). */
private fun DrawScope.drawCorner(corner: Int, vf: Float, radius: Float, color: Color) {
    val d = density
    val l = Round5Style.CORNER_LENGTH * d
    val h = Round5Style.CORNER_STROKE * d / 2f
    val r = max(0f, min(l - h, radius * d - h))
    val e = vf * d
    // corner: 0 top-left, 1 top-right, 2 bottom-left, 3 bottom-right.
    val right = corner == 1 || corner == 3
    val bottom = corner == 2 || corner == 3
    fun fx(x: Float) = if (right) e - x else x
    fun fy(y: Float) = if (bottom) e - y else y
    val path = Path().apply {
        moveTo(fx(h), fy(l))
        lineTo(fx(h), fy(h + r))
        if (r > 0f) quadraticTo(fx(h), fy(h), fx(h + r), fy(h))
        lineTo(fx(l), fy(h))
    }
    drawPath(path, color, style = Stroke(width = Round5Style.CORNER_STROKE * d, cap = StrokeCap.Round, join = StrokeJoin.Round))
}

/**
 * A `QrScanner` (§7): the viewfinder CHROME. A square (min(width, 280))
 * centred at the top, clipped to the viewfinder radius: the host's feed
 * ([LocalQrScanner]) or, without one, the `feed` colour at 6 % alpha with
 * the scan icon and `Camera unavailable`; four corner brackets; while a feed
 * shows the 2 dp scan line sweeps top → bottom → top over 2 s (reduced
 * motion: static at the centre). `disabled` stops the camera (the
 * placeholder) at the root's disabled opacity. The `hint` is a muted line
 * under the viewfinder. A decoded value fires `scanned {value}` once and is
 * announced politely.
 */
@Composable
internal fun QrScannerLeaf(cx: LeafContext) {
    val model = cx.model
    val props = cx.props
    val index = cx.index
    val disabled = props.flag("disabled") || model.isDisabled(index)
    val host = LocalQrScanner.current
    val formats = qrScanFormats(props)
    val camera = host != null && host.available && !disabled
    val vf = scannerViewfinder(cx.size.width)
    val radius = cx.part("QrScanner", "viewfinder", props).style.radius
    val feedColour = (cx.part("QrScanner", "feed", props).style.background ?: model.color("foreground") ?: cx.ink).let { it.copy(alpha = it.alpha * Round5Style.FEED_ALPHA) }
    val cornerColour = cx.part("QrScanner", "corner", props).color ?: model.color("primary") ?: cx.ink
    val lineColour = cx.part("QrScanner", "scanLine", props).style.background ?: model.color("primary") ?: cx.ink
    val label = stringProp(model, props.str("label"), "scanQrCode")
    val unavailable = model.string("cameraUnavailable")
    val hint = props.shownText("hint")
    val gap = cx.style.gap.takeIf { it > 0f } ?: Round5Style.SCANNER_HINT_GAP
    val latestIndex by rememberUpdatedState(index)
    // One reporter per mount (and per re-enable): a remount reports a held code again.
    val reporter = remember(cx.node.id, camera) {
        QrScanReporter { value ->
            model.announce(value)
            model.fire(latestIndex, "scanned", JsonValue.Obj(mapOf("value" to JsonValue.Str(value))))
        }
    }
    LeafFrame {
        Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(gap.dp)) {
            Box(
                Modifier
                    .size(vf.dp)
                    .clip(RoundedCornerShape(radius.dp))
                    .clearAndSetSemantics {
                        contentDescription = if (camera) label else "$label, $unavailable"
                        role = Role.Image
                    },
            ) {
                if (camera) {
                    host!!.Feed(formats, reporter::accept, Modifier.fillMaxSize())
                } else {
                    Box(Modifier.fillMaxSize().background(feedColour))
                }
                Canvas(Modifier.fillMaxSize()) {
                    for (corner in 0 until 4) drawCorner(corner, vf, radius, cornerColour)
                }
                if (camera) {
                    ScanLine(vf, lineColour, still = model.reducedMotion)
                } else {
                    val muted = cx.part("QrScanner", "placeholderIcon", props).color ?: model.color("mutedForeground") ?: cx.ink
                    val caption = cx.part("QrScanner", "placeholder", props)
                    val cts = ResolvedTextStyle(caption.px("fontSize") ?: 12f, 400, caption.px("lineHeight") ?: 16f, caption.fontFamily)
                    Column(
                        Modifier.fillMaxSize(),
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(cx.spacing("xs").dp, Alignment.CenterVertically),
                    ) {
                        IconView(Round5Style.PLACEHOLDER_ICON, Round5Style.PLACEHOLDER_ICON_SIZE, muted, model)
                        BasicText(unavailable, style = cx.composeTextStyle(ts = cts, color = caption.color ?: muted, align = TextAlign.Center))
                    }
                }
            }
            if (hint.isNotEmpty()) {
                val hp = cx.part("QrScanner", "hint", props)
                val hts = ResolvedTextStyle(hp.px("fontSize") ?: 14f, 400, hp.px("lineHeight") ?: 20f, hp.fontFamily)
                BasicText(cx.shown(hint), Modifier.fillMaxWidth(), style = cx.composeTextStyle(ts = hts, color = hp.color ?: cx.ink, align = TextAlign.Center))
            }
        }
    }
}

/** The scan line: 2 dp, inset 16 dp, sweeping top → bottom → top (ease standard, alternate); [still] = at the centre. */
@Composable
private fun ScanLine(vf: Float, color: Color, still: Boolean) {
    val travel = max(0f, vf - Round5Style.SCAN_LINE_HEIGHT)
    val y = if (still) {
        travel / 2f
    } else {
        val t = rememberInfiniteTransition(label = "scan")
        val f by t.animateFloat(0f, 1f, infiniteRepeatable(tween(Round5Style.SWEEP_MS, easing = CubicBezierEasing(0.2f, 0f, 0f, 1f)), RepeatMode.Reverse), label = "sweep")
        travel * f
    }
    Box(
        Modifier
            .absoluteOffset(Round5Style.SCAN_LINE_INSET.dp, y.dp)
            .width(max(0f, vf - 2 * Round5Style.SCAN_LINE_INSET).dp)
            .height(Round5Style.SCAN_LINE_HEIGHT.dp)
            .background(color, RoundedCornerShape((Round5Style.SCAN_LINE_HEIGHT / 2).dp)),
    )
}

// ---------------------------------------------------------------------------
// VoiceWave `bars` (§5)
// ---------------------------------------------------------------------------

/** One bar of the core's meter (dp from the meter's top-left). */
internal data class VoiceWaveBar(val x: Float, val y: Float, val width: Float, val height: Float, val radius: Float)

/** One frame of the `bars` meter from the core (`voiceWaveBarsJson`). */
internal class VoiceWaveBarsSpec(val width: Float, val height: Float, val phase: Float, val bars: List<VoiceWaveBar>) {
    companion object {
        /** The facade's input: `{size, activity, level, bands, elapsed, reducedMotion}` ([size] = sm | md | lg | xl). */
        fun input(size: String, activity: String, level: Double, bands: List<Double>, elapsed: Double, reducedMotion: Boolean): String =
            JsonValue.Obj(
                linkedMapOf(
                    "size" to JsonValue.Str(size),
                    "activity" to JsonValue.Str(activity),
                    "level" to JsonValue.Num(level),
                    "bands" to JsonValue.Arr(bands.map { JsonValue.Num(it) }),
                    "elapsed" to JsonValue.Num(elapsed),
                    "reducedMotion" to JsonValue.Bool(reducedMotion),
                ),
            ).orderedJson

        fun of(inputJson: String): VoiceWaveBarsSpec? = try {
            parse(JsonValue.parse(voiceWaveBarsJson(inputJson)))
        } catch (_: Throwable) {
            null
        }

        fun parse(v: JsonValue): VoiceWaveBarsSpec? {
            val bars = v["bars"]?.array ?: return null
            fun n(o: JsonValue, k: String) = (o[k]?.number ?: 0.0).toFloat()
            return VoiceWaveBarsSpec(n(v, "width"), n(v, "height"), n(v, "phase"), bars.map { VoiceWaveBar(n(it, "x"), n(it, "y"), n(it, "width"), n(it, "height"), n(it, "radius")) })
        }
    }
}

/** Paint [spec]'s bars at [origin] (px) in [color]. */
internal fun DrawScope.drawVoiceWaveBars(spec: VoiceWaveBarsSpec, origin: Offset, color: Color) {
    val d = density
    for (b in spec.bars) {
        drawRoundRect(color, Offset(origin.x + b.x * d, origin.y + b.y * d), Size(b.width * d, b.height * d), CornerRadius(b.radius * d))
    }
}

// ---------------------------------------------------------------------------
// Segmented count badge (§3)
// ---------------------------------------------------------------------------

/** A `bar` item's badge text (the core's: `99+` above 99); null = no badge. */
internal fun segmentedBadge(option: JsonValue): String? {
    val n = option["count"]?.number ?: return null
    return runCatching { segmentedBadgeText(n) }.getOrNull()
}

/** The accessibility name of an item: the label, then the count ("Chats, 3"). */
internal fun segmentedItemLabel(label: String, badge: String?): String = if (badge == null) label else "$label, $badge"
