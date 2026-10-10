package at.exponential.ui.compose

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.media3.ui.PlayerView
import at.exponential.ui.catalog.CatalogConstants.TREE_GUIDE_BRIDGE
import at.exponential.ui.catalog.CatalogConstants.TREE_GUIDE_COLUMN
import at.exponential.ui.catalog.CatalogConstants.TREE_GUIDE_RADIUS
import at.exponential.ui.json.Props
import at.exponential.ui.json.list
import at.exponential.ui.json.num
import at.exponential.ui.json.str
import at.exponential.ui.model.carouselPage
import at.exponential.ui.paint.DateModel
import at.exponential.ui.primitives.AvatarFallback
import at.exponential.ui.primitives.AvatarView
import at.exponential.ui.primitives.RingView
import at.exponential.ui.primitives.Rgba
import at.exponential.ui.theme.ResolvedTextStyle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.InputStream
import at.exponential.ui.host.MediaLimits
import at.exponential.ui.host.MediaRequest
import java.net.HttpURLConnection
import java.net.URL
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * The media loader the leaves share (no image library). It loads ONLY a
 * policed [MediaRequest] (the host's media policy already allowed it):
 * `http(s)` (its headers sent; redirects followed by hand, each hop passed
 * through the policy again) and `data:` URIs; never a local file or content
 * URI. Every load enforces [MediaLimits] (catalog/host.json `media.limits`):
 * Content-Length up front and the body as it streams, connect + read
 * timeouts and a deadline over the whole request, width × height read from
 * the header BEFORE decoding. Decoded with `BitmapFactory` on the IO
 * dispatcher, downsampled to ≤ 2048 px, cached per URL (an LRU of 48
 * pictures); failures are remembered so a broken URL is not refetched per
 * frame.
 */
object LeafImages {
    private const val MAX_SIDE = 2048
    private const val MAX_REDIRECTS = 5
    private val cache = LruCache<String, ImageBitmap>(48)
    private val failed = HashSet<String>()

    /** Why a load failed (each paints the fallback, like a 404). */
    enum class Failure { Denied, Http, TooLarge, Timeout, TooManyPixels, Undecodable }

    /** A failed load and its [reason]. */
    class LoadFailure(val reason: Failure, message: String) : java.io.IOException(message)

    /** A cached picture (null = not loaded yet or failed). */
    fun cached(url: String): ImageBitmap? = cache.get(url)

    /** Did `url` fail to load? */
    fun hasFailed(url: String): Boolean = synchronized(failed) { failed.contains(url) }

    /** Drop every cached picture and failure (memory pressure, a host reset). */
    fun clear() {
        cache.evictAll()
        synchronized(failed) { failed.clear() }
    }

    /**
     * Load a policed request (cache first); null when it fails. [police]
     * re-checks a redirect's target (null = denied).
     */
    suspend fun load(
        request: MediaRequest,
        limits: MediaLimits = MediaLimits.contract,
        police: (String) -> MediaRequest? = { null },
    ): ImageBitmap? {
        val url = request.url
        cache.get(url)?.let { return it }
        if (hasFailed(url)) return null
        val bitmap = withContext(Dispatchers.IO) {
            runCatching { decode(fetch(request, limits, police), limits) }.getOrNull()
        }
        if (bitmap == null) {
            synchronized(failed) { failed.add(url) }
            return null
        }
        cache.put(url, bitmap)
        return bitmap
    }

    /** The bytes of a policed request within [limits] (blocking). Throws [LoadFailure]. */
    fun fetch(request: MediaRequest, limits: MediaLimits = MediaLimits.contract, police: (String) -> MediaRequest? = { null }): ByteArray {
        val deadline = System.nanoTime() + limits.timeoutMs * 1_000_000
        var current = request
        repeat(MAX_REDIRECTS + 1) {
            val scheme = current.url.substringBefore(':', "").lowercase()
            when (scheme) {
                "data" -> return dataBytes(current.url, limits)
                "http", "https" -> {}
                else -> throw LoadFailure(Failure.Denied, "scheme $scheme")
            }
            val remaining = ((deadline - System.nanoTime()) / 1_000_000).coerceAtLeast(1).coerceAtMost(Int.MAX_VALUE.toLong()).toInt()
            val conn = URL(current.url).openConnection() as HttpURLConnection
            try {
                conn.connectTimeout = remaining
                conn.readTimeout = remaining
                conn.instanceFollowRedirects = false
                for ((k, v) in current.headers) conn.setRequestProperty(k, v)
                val code = try {
                    conn.responseCode
                } catch (e: java.net.SocketTimeoutException) {
                    throw LoadFailure(Failure.Timeout, "timed out")
                }
                if (code in 300..399) {
                    val location = conn.getHeaderField("Location") ?: throw LoadFailure(Failure.Http, "redirect without Location")
                    val next = URL(URL(current.url), location).toString()
                    current = police(next) ?: throw LoadFailure(Failure.Denied, "redirect to $next")
                    return@repeat
                }
                if (code !in 200..299) throw LoadFailure(Failure.Http, "HTTP $code")
                val length = conn.contentLengthLong
                if (length > limits.maxBytes) throw LoadFailure(Failure.TooLarge, "Content-Length $length > ${limits.maxBytes}")
                return conn.inputStream.use { readCapped(it, limits.maxBytes, deadline) }
            } finally {
                conn.disconnect()
            }
        }
        throw LoadFailure(Failure.Http, "too many redirects")
    }

    /** Read [input] to the end: more than [maxBytes] or past [deadline] (nanoTime) fails. */
    fun readCapped(input: InputStream, maxBytes: Long, deadline: Long = Long.MAX_VALUE): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        val buf = ByteArray(16 * 1024)
        var total = 0L
        while (true) {
            val n = try {
                input.read(buf)
            } catch (e: java.net.SocketTimeoutException) {
                throw LoadFailure(Failure.Timeout, "timed out")
            }
            if (n < 0) break
            total += n
            if (total > maxBytes) throw LoadFailure(Failure.TooLarge, "body over ${maxBytes} bytes")
            if (System.nanoTime() > deadline) throw LoadFailure(Failure.Timeout, "timed out")
            out.write(buf, 0, n)
        }
        return out.toByteArray()
    }

    /** A `data:[mime][;base64],payload` URI's bytes (within the byte cap). */
    private fun dataBytes(url: String, limits: MediaLimits): ByteArray {
        val comma = url.indexOf(',')
        if (comma < 0) throw LoadFailure(Failure.Undecodable, "data uri without a comma")
        val meta = url.substring(5, comma)
        val payload = url.substring(comma + 1)
        val bytes = if (meta.endsWith(";base64", ignoreCase = true)) {
            if (payload.length / 4L * 3 > limits.maxBytes) throw LoadFailure(Failure.TooLarge, "data uri over ${limits.maxBytes} bytes")
            runCatching { java.util.Base64.getMimeDecoder().decode(payload) }.getOrElse { throw LoadFailure(Failure.Undecodable, "bad base64") }
        } else {
            java.net.URLDecoder.decode(payload.replace("+", "%2B"), "UTF-8").toByteArray(Charsets.ISO_8859_1)
        }
        if (bytes.size > limits.maxBytes) throw LoadFailure(Failure.TooLarge, "data uri over ${limits.maxBytes} bytes")
        return bytes
    }

    /**
     * The width × height the image header claims (BitmapFactory
     * `inJustDecodeBounds`: nothing decoded); null when it is no image.
     */
    fun bounds(bytes: ByteArray): Pair<Int, Int>? {
        val opts = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)
        return if (opts.outWidth <= 0 || opts.outHeight <= 0) null else opts.outWidth to opts.outHeight
    }

    /** Check the header's pixel count against [limits] BEFORE any decode. Throws [LoadFailure]. */
    fun checkPixels(width: Int, height: Int, limits: MediaLimits) {
        if (width.toLong() * height.toLong() > limits.maxPixels) {
            throw LoadFailure(Failure.TooManyPixels, "${width}×$height over ${limits.maxPixels} pixels")
        }
    }

    /** Decode within the limits: the header first (refused over `maxPixels`), then a downsampled decode. */
    fun decode(bytes: ByteArray, limits: MediaLimits = MediaLimits.contract): ImageBitmap {
        val (w, h) = bounds(bytes) ?: throw LoadFailure(Failure.Undecodable, "no image header")
        checkPixels(w, h, limits)
        var sample = 1
        while (max(w, h) / sample > MAX_SIDE) sample *= 2
        val opts = BitmapFactory.Options().apply { inSampleSize = sample }
        val bmp = BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts) ?: throw LoadFailure(Failure.Undecodable, "decode failed")
        return bmp.asImageBitmap()
    }
}

/**
 * The picture at `src`: the surface's policed media request (the host's
 * `resolveUrl`, then its media policy; denied = null, the fallback paints),
 * loading in the background; null while loading / without one.
 */
@Composable
internal fun rememberLeafImage(cx: LeafContext, src: String): ImageBitmap? = rememberPolicedImage(cx.model, src)

/** [rememberLeafImage] for any surface painter (markdown images too). */
@Composable
internal fun rememberPolicedImage(model: at.exponential.ui.model.SurfaceModel, src: String): ImageBitmap? {
    if (src.isEmpty()) return null
    val request = remember(src, model.host) { model.mediaRequest(src) } ?: return null
    val image by produceState(LeafImages.cached(request.url), request) {
        value = LeafImages.load(request, police = { model.mediaRequest(it) })
    }
    return image
}

/** `Icon`: the host icon by name (tone-tinted), else the placeholder circle, centred. */
@Composable
internal fun IconLeaf(cx: LeafContext) {
    val name = cx.props.str("name").ifEmpty { "ui-icon-placeholder" }
    val tone = cx.props.str("tone")
    val color = if (tone.isEmpty()) cx.ink else cx.tone(tone) ?: cx.ink
    val r = cx.inner
    LeafFrame {
        IconView(name, max(1f, min(r.width, r.height)), color, cx.model, Modifier.align(Alignment.Center), rtl = cx.rtl)
    }
}

/** `Avatar`: the picture when `src` loads, else tinted initials (the `Avatar/fallback` recipe for an empty seed). */
@Composable
internal fun AvatarLeaf(cx: LeafContext) {
    val name = cx.props.str("name")
    val seed = cx.props.str("seed").ifEmpty { name }
    val fallback = cx.part("Avatar", "fallback")
    val size = min(cx.size.width, cx.size.height)
    val fs = fallback.px("fontSize")
    val picture = rememberLeafImage(cx, cx.props.str("src"))
    LeafFrame {
        AvatarView(
            name = name,
            modifier = Modifier.align(Alignment.Center),
            seed = seed,
            size = size.dp,
            dark = cx.dark,
            fill = if (seed.isEmpty()) fallback.style.background else null,
            ink = if (seed.isEmpty()) fallback.color else null,
            fontSize = fs?.sp ?: androidx.compose.ui.unit.TextUnit.Unspecified,
            contentDescription = null,
            picture = picture?.let { bmp -> { Image(bmp, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop) } },
        )
    }
}

/** The tinted placeholder an `Image` paints without a loaded picture: the image glyph over the alt text. */
@Composable
internal fun ImagePlaceholder(cx: LeafContext, ink: Color, label: String) {
    Column(
        Modifier.fillMaxSize().background(ink.copy(alpha = ink.alpha * 0.08f)),
        verticalArrangement = Arrangement.spacedBy(4.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        GlyphView(Glyph.Image, 20f, ink.copy(alpha = ink.alpha * 0.6f))
        BasicText(
            label,
            style = cx.composeTextStyle(ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily), color = ink.copy(alpha = ink.alpha * 0.6f)),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/** `Image`: the picture scaled by `fit` (cover default; contain / scaleDown fit; fill stretches), clipped; else the placeholder. */
@Composable
internal fun ImageLeaf(cx: LeafContext) {
    val alt = cx.props.str("alt").ifEmpty { "image" }
    val muted = cx.themeColor("mutedForeground") ?: cx.ink
    val picture = rememberLeafImage(cx, cx.props.str("src"))
    val scale = when (cx.props.str("fit")) {
        "contain", "scaleDown" -> ContentScale.Fit
        "fill" -> ContentScale.FillBounds
        "none" -> ContentScale.None
        else -> ContentScale.Crop
    }
    Box(Modifier.fillMaxSize().clipToBounds()) {
        if (picture != null) {
            Image(picture, null, Modifier.fillMaxSize(), contentScale = scale)
        } else {
            ImagePlaceholder(cx, muted, alt)
        }
    }
}

/**
 * The leaf's [MediaPlayback] (released when the leaf leaves the
 * composition) and the policed request for its `src` (null = no src or
 * DENIED: nothing loads, the controls stay inert).
 */
@Composable
private fun rememberPlayback(cx: LeafContext): Pair<MediaPlayback, MediaRequest?> {
    val context = LocalContext.current
    val playback = remember { MediaPlayback(context.applicationContext ?: context) }
    DisposableEffect(playback) { onDispose { playback.stop() } }
    val src = cx.props.str("src")
    val model = cx.model
    val request = remember(src, model.host) { if (src.isEmpty()) null else model.mediaRequest(src) }
    return playback to request
}

/**
 * `Video`: `src` plays through the policed media request (ExoPlayer in a
 * `PlayerView` with the platform's controls; `autoplay` = muted on
 * appear); before that the poster (or a dark tint), a play button and the
 * duration. A denied src stays that poster with an inert play glyph
 * (React's sourceless `<video>`).
 */
@Composable
internal fun VideoLeaf(cx: LeafContext) {
    val (playback, request) = rememberPlayback(cx)
    val model = cx.model
    val autoplay = cx.props["autoplay"]?.bool == true
    val scope = rememberCoroutineScope()
    LaunchedEffect(request, autoplay) {
        if (autoplay && request != null) {
            playback.play(request, muted = true, police = model::mediaRequest)
        } else if (playback.opened != request) {
            playback.stop()
        }
    }
    val player = playback.player
    Box(Modifier.fillMaxSize().clipToBounds().background(Color.Black.copy(alpha = 0.85f))) {
        if (player != null) {
            AndroidView(
                factory = { PlayerView(it).apply { useController = true } },
                modifier = Modifier.fillMaxSize(),
                update = { it.player = player },
                onRelease = { it.player = null },
            )
            return@Box
        }
        val poster = rememberLeafImage(cx, cx.props.str("poster"))
        if (poster != null) Image(poster, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        Box(
            Modifier
                .align(Alignment.Center)
                .size(44.dp)
                .background(Color.White.copy(alpha = 0.18f), CircleShape)
                .clickable(enabled = request != null, role = Role.Button) {
                    scope.launch { playback.play(request, police = model::mediaRequest) }
                },
            contentAlignment = Alignment.Center,
        ) { GlyphView(Glyph.Play, 20f, Color.White) }
        cx.props.num("durationMs")?.let { ms ->
            BasicText(
                DateModel.formatDuration(ms),
                modifier = Modifier
                    .align(Alignment.BottomEnd)
                    .padding(end = 8.dp, bottom = 6.dp)
                    .background(Color.Black.copy(alpha = 0.6f), RoundedCornerShape(4.dp))
                    .padding(horizontal = 6.dp, vertical = 2.dp),
                style = cx.composeTextStyle(ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily), color = Color.White),
                maxLines = 1,
            )
        }
    }
}

/**
 * `AudioPlayer`: the title line over a controls capsule. `src` plays
 * through the policed media request: play / pause, the track fills with
 * the position (a press seeks), `elapsed / length` (the item's, else
 * `durationMs`). A denied src keeps the controls inert.
 */
@Composable
internal fun AudioLeaf(cx: LeafContext) {
    val (playback, request) = rememberPlayback(cx)
    val model = cx.model
    val scope = rememberCoroutineScope()
    LaunchedEffect(request) { if (playback.opened != request) playback.stop() }
    // The position while it plays (4 Hz, Swift's periodic observer).
    LaunchedEffect(playback.player, playback.playing) {
        while (playback.playing) {
            playback.tick()
            delay(250)
        }
        playback.tick()
    }
    val title = cx.props.str("title")
    val muted = cx.themeColor("muted") ?: cx.ink.copy(alpha = 0.1f)
    val mutedFg = cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = 0.6f)
    val track = cx.part("AudioPlayer", "track")
    val known = cx.props.num("durationMs")?.toLong()
    val length = playback.durationMs ?: known
    val progress = length?.takeIf { it > 0 }?.let { (playback.positionMs.toFloat() / it).coerceIn(0f, 1f) } ?: 0f
    val elapsed = DateModel.formatDuration(playback.positionMs.toDouble())
    InnerBox(cx) {
        Column(verticalArrangement = Arrangement.spacedBy(cx.spacing("xs").dp)) {
            if (title.isNotEmpty()) LeafLine(cx, title, color = track.color ?: cx.ink)
            Row(
                Modifier.fillMaxWidth().height(40.dp).background(muted, CircleShape).padding(horizontal = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(
                    Modifier
                        .size(28.dp)
                        .background(cx.ink, CircleShape)
                        .clickable(enabled = request != null, role = Role.Button) {
                            if (playback.playing) playback.pause() else scope.launch { playback.play(request, police = model::mediaRequest) }
                        },
                    contentAlignment = Alignment.Center,
                ) {
                    GlyphView(if (playback.playing) Glyph.Pause else Glyph.Play, 14f, cx.themeColor("background") ?: Color.White)
                }
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .pointerInput(playback, known) {
                            detectTapGestures { o -> playback.seek(if (size.width > 0) o.x / size.width else 0f, known) }
                        },
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Box(Modifier.fillMaxWidth().height(4.dp).background(mutedFg.copy(alpha = mutedFg.alpha * 0.35f), CircleShape))
                    if (progress > 0f) Box(Modifier.fillMaxWidth(progress).height(4.dp).background(mutedFg, CircleShape))
                }
                BasicText(
                    "$elapsed / ${length?.let { DateModel.formatDuration(it.toDouble()) } ?: "0:00"}",
                    style = cx.composeTextStyle(ts = ResolvedTextStyle(12f, 400, 16f, cx.textStyle.fontFamily), color = mutedFg),
                    maxLines = 1,
                )
            }
        }
    }
}

/** The `Skeleton` block: the recipe background the box paints (a muted fill when the recipe has none); static. */
@Composable
internal fun SkeletonLeaf(cx: LeafContext) {
    if (cx.style.background != null) return
    val fill = cx.themeColor("muted") ?: return
    val shape = if (cx.props["rounded"]?.bool == true) CircleShape else RoundedCornerShape(cx.style.radius.dp)
    Box(Modifier.fillMaxSize().background(fill, shape))
}

/** `Ring`: the `Ring/track` circle + the `Ring/fill` arc from twelve o'clock (value / max, max 1), the label centred. */
@Composable
internal fun RingLeaf(cx: LeafContext) {
    val maxV = cx.props.num("max")?.takeIf { it > 0 } ?: 1.0
    val value = ((cx.props.num("value") ?: 0.0) / maxV).coerceIn(0.0, 1.0)
    val track = cx.part("Ring", "track")
    val fill = cx.part("Ring", "fill")
    val stroke = track.px("borderWidth") ?: 2f
    val fillStroke = fill.px("borderWidth") ?: stroke
    val trackColor = track.color ?: cx.ink.copy(alpha = 0.15f)
    val fillColor = fill.color ?: cx.tone(cx.props.str("tone")) ?: cx.ink
    val label = cx.part("Ring", "label")
    val size = min(cx.size.width, cx.size.height)
    LeafFrame {
        RingView(value.toFloat(), Modifier.align(Alignment.Center), size = size.dp, stroke = stroke.dp, track = trackColor, fill = fillColor, fillStroke = fillStroke.dp)
        val text = cx.props.str("label")
        if (text.isNotEmpty()) {
            BasicText(
                text,
                modifier = Modifier.align(Alignment.Center).wrapContentSize(unbounded = true),
                style = cx.composeTextStyle(ts = ResolvedTextStyle(label.px("fontSize") ?: 12f, 400, label.px("lineHeight") ?: 16f, label.fontFamily), color = label.color ?: cx.ink),
                softWrap = false,
                maxLines = 1,
            )
        }
    }
}

/**
 * `TreeGuides` (round 3, `layout.json` `treeGuideColumn` / `treeGuideRadius`
 * / `treeGuideBridge`, the gpui painter): 14 dp columns, the 1 dp line of
 * column i with its LEFT edge at x = i·14 + 7. The elbow = a vertical from
 * the row's top − bridge into a 3 dp ROUNDED corner at the row's centre,
 * then a stub to the column's right edge (i·14 + 14); `tee` carries the
 * vertical on to the bottom; `passThrough` columns = full-height verticals
 * from top − bridge. The bridge overshoots the row's top (paint only) so the
 * line carries across a Section divider. Mirrored in RTL. Stroke width /
 * colour = the `TreeGuides/line` recipe.
 */
@Composable
internal fun TreeGuidesLeaf(cx: LeafContext) {
    val geometry = treeGuideGeometry(cx.props, cx.rtl)
    val line = cx.part("TreeGuides", "line")
    val color = line.color ?: line.style.background ?: cx.ink.copy(alpha = 0.25f)
    val lw = line.px("width") ?: 1f
    Canvas(Modifier.fillMaxSize()) {
        val d = density
        val width = size.width / d
        val h = size.height / d
        geometry.paint(width, h, lw) { op ->
            when (op) {
                is TreeGuideOp.Vertical -> drawRect(color, Offset(op.x * d, op.from * d), Size(lw * d, (op.to - op.from) * d))
                is TreeGuideOp.Elbow -> {
                    val path = Path().apply {
                        moveTo(op.x * d, (op.mid - op.radius) * d)
                        quadraticTo(op.x * d, op.mid * d, (op.x + op.dir * op.radius) * d, op.mid * d)
                        lineTo(op.stubEnd * d, op.mid * d)
                    }
                    drawPath(path, color, style = Stroke(width = lw * d))
                }
            }
        }
    }
}

/** One drawing step of the tree guides (dp, the leaf's own coordinates). */
internal sealed interface TreeGuideOp {
    /** A `lw`-wide vertical whose LEFT edge is at `x`, from `from` to `to`. */
    data class Vertical(val x: Float, val from: Float, val to: Float) : TreeGuideOp

    /** The rounded corner + stub: down the line's centre `x` into a quarter turn at `mid`, then across to `stubEnd`. */
    data class Elbow(val x: Float, val mid: Float, val radius: Float, val dir: Float, val stubEnd: Float) : TreeGuideOp
}

/** The tree guides' props, laid out by [paint] (pure: the geometry test reads it without a canvas). */
internal class TreeGuideGeometry(val depth: Int, val elbowAt: Int, val tee: Boolean, val pass: List<Int>, val rtl: Boolean) {
    fun paint(width: Float, height: Float, lw: Float, draw: (TreeGuideOp) -> Unit) {
        val col = TREE_GUIDE_COLUMN
        val radius = TREE_GUIDE_RADIUS
        val top = -TREE_GUIDE_BRIDGE
        val mid = height / 2f
        // The physical x of a span starting at gutter offset `x`.
        fun at(x: Float, span: Float) = if (rtl) width - x - span else x
        fun vertical(x: Float, from: Float, to: Float) {
            if (to > from) draw(TreeGuideOp.Vertical(at(x, lw), from, to))
        }
        for (i in 0 until depth) {
            val x = i * col + col / 2f
            if (pass.contains(i)) vertical(x, top, height)
            if (i == elbowAt) {
                vertical(x, top, if (tee) height else mid - radius)
                draw(TreeGuideOp.Elbow(at(x, lw) + lw / 2f, mid, radius, if (rtl) -1f else 1f, at((i + 1) * col, 0f)))
            }
        }
    }
}

internal fun treeGuideGeometry(props: Props, rtl: Boolean): TreeGuideGeometry {
    val depth = max(props.num("depth") ?: 0.0, 0.0).toInt()
    return TreeGuideGeometry(
        depth = depth,
        // The core fills `elbowAt`; a row it never saw (a lone root, a template item) draws no elbow.
        elbowAt = props.num("elbowAt")?.toInt() ?: -1,
        tee = props["tee"]?.bool == true,
        pass = props.list("passThrough").mapNotNull { it.number?.toInt() },
        rtl = rtl,
    )
}

/** The Carousel dot strip (`Carousel/indicator`), centred on the core's leaf (overflowing it); a dot tap pages. */
@Composable
internal fun CarouselIndicatorLeaf(cx: LeafContext) {
    val count = max(cx.props.num("count") ?: 0.0, 0.0).toInt()
    val page = (cx.props.num("page") ?: 0.0).roundToInt()
    val dot = cx.part("Carousel", "indicator")
    val size = dot.width ?: 8f
    val gap = cx.spacing("xs")
    val base = dot.style.background ?: cx.ink
    val model = cx.model
    val index = cx.index
    LeafFrame {
        Row(Modifier.align(Alignment.Center).wrapContentSize(unbounded = true), horizontalArrangement = Arrangement.spacedBy(gap.dp)) {
            for (i in 0 until count) {
                Box(
                    Modifier
                        .size(size.dp)
                        .background(if (i == page) base else base.copy(alpha = base.alpha * 0.35f), CircleShape)
                        .clickable(role = Role.Tab) { model.carouselPage(index, i) }
                        .semantics {
                            contentDescription = cx.string("pageOf", mapOf("page" to i + 1, "total" to count))
                            selected = i == page
                        },
                )
            }
        }
    }
}
