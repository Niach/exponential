package at.exponential.ui.compose

import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.util.LruCache
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
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
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
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
import kotlinx.coroutines.withContext
import java.io.InputStream
import at.exponential.ui.host.MediaRequest
import java.net.HttpURLConnection
import java.net.URL
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * The media loader the leaves share (no image library): `http(s)`, `file`
 * and `content` URIs decoded with `BitmapFactory` on the IO dispatcher,
 * downsampled to ≤ 2048 px, cached per URL (an LRU of 48 pictures);
 * failures are remembered so a broken URL is not refetched per frame.
 */
object LeafImages {
    private const val MAX_SIDE = 2048
    private val cache = LruCache<String, ImageBitmap>(48)
    private val failed = HashSet<String>()

    /** A cached picture (null = not loaded yet or failed). */
    fun cached(url: String): ImageBitmap? = cache.get(url)

    /** Did `url` fail to load? */
    fun hasFailed(url: String): Boolean = synchronized(failed) { failed.contains(url) }

    /** Drop every cached picture and failure (memory pressure, a host reset). */
    fun clear() {
        cache.evictAll()
        synchronized(failed) { failed.clear() }
    }

    /** Load `url` (cache first); null when the scheme is unsupported or the load fails. */
    suspend fun load(context: Context?, url: String): ImageBitmap? = load(context, MediaRequest(url))

    /** Load a host's media request (its headers go with an http(s) fetch); cached per url. */
    suspend fun load(context: Context?, request: MediaRequest): ImageBitmap? {
        val url = request.url
        cache.get(url)?.let { return it }
        if (hasFailed(url)) return null
        val bitmap = withContext(Dispatchers.IO) { runCatching { decode(context, url, request.headers) }.getOrNull() }
        if (bitmap == null) {
            synchronized(failed) { failed.add(url) }
            return null
        }
        cache.put(url, bitmap)
        return bitmap
    }

    private fun open(context: Context?, url: String, headers: Map<String, String>): InputStream? {
        val uri = Uri.parse(url)
        return when (uri.scheme?.lowercase()) {
            "http", "https" -> (URL(url).openConnection() as HttpURLConnection).run {
                connectTimeout = 15_000
                readTimeout = 30_000
                instanceFollowRedirects = true
                for ((k, v) in headers) setRequestProperty(k, v)
                if (responseCode !in 200..299) null else inputStream
            }
            "file" -> uri.path?.let { java.io.File(it).inputStream() }
            "content", "android.resource" -> context?.contentResolver?.openInputStream(uri)
            else -> null
        }
    }

    private fun decode(context: Context?, url: String, headers: Map<String, String>): ImageBitmap? {
        val bytes = open(context, url, headers)?.use { it.readBytes() } ?: return null
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        var sample = 1
        while (max(bounds.outWidth, bounds.outHeight) / sample > MAX_SIDE) sample *= 2
        val opts = BitmapFactory.Options().apply { inSampleSize = sample }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, opts)?.asImageBitmap()
    }
}

/** The picture at `src` (the host's media request: url + headers), loading in the background; null while loading / without one. */
@Composable
internal fun rememberLeafImage(cx: LeafContext, src: String): ImageBitmap? {
    if (src.isEmpty()) return null
    val request = remember(src, cx.model.host) { cx.model.host.mediaRequest(src) } ?: return null
    val url = request.url
    val scheme = Uri.parse(url).scheme
    if (scheme.isNullOrEmpty()) return null
    val context = LocalContext.current.applicationContext
    val image by produceState(LeafImages.cached(url), request) { value = LeafImages.load(context, request) }
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

/** `Video`: the poster (or a dark tint) with a play button and the duration badge (static; playback is the host's). */
@Composable
internal fun VideoLeaf(cx: LeafContext) {
    val poster = rememberLeafImage(cx, cx.props.str("poster"))
    Box(Modifier.fillMaxSize().clipToBounds().background(Color.Black.copy(alpha = 0.85f))) {
        if (poster != null) Image(poster, null, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        Box(
            Modifier.align(Alignment.Center).size(44.dp).background(Color.White.copy(alpha = 0.18f), CircleShape),
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

/** `AudioPlayer`: the title line over a static controls capsule (play, track, `0:00 / duration`). */
@Composable
internal fun AudioLeaf(cx: LeafContext) {
    val title = cx.props.str("title")
    val muted = cx.themeColor("muted") ?: cx.ink.copy(alpha = 0.1f)
    val mutedFg = cx.themeColor("mutedForeground") ?: cx.ink.copy(alpha = 0.6f)
    val track = cx.part("AudioPlayer", "track")
    val duration = cx.props.num("durationMs")?.let(DateModel::formatDuration) ?: "0:00"
    InnerBox(cx) {
        Column(verticalArrangement = Arrangement.spacedBy(cx.spacing("xs").dp)) {
            if (title.isNotEmpty()) LeafLine(cx, title, color = track.color ?: cx.ink)
            Row(
                Modifier.fillMaxWidth().height(40.dp).background(muted, CircleShape).padding(horizontal = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Box(Modifier.size(28.dp).background(cx.ink, CircleShape), contentAlignment = Alignment.Center) {
                    GlyphView(Glyph.Play, 14f, cx.themeColor("background") ?: Color.White)
                }
                Box(Modifier.weight(1f).height(4.dp).background(mutedFg.copy(alpha = mutedFg.alpha * 0.35f), CircleShape))
                BasicText(
                    "0:00 / $duration",
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
        elbowAt = props.num("elbowAt")?.toInt() ?: (depth - 1),
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
