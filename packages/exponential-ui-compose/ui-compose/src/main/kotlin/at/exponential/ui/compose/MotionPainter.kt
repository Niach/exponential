package at.exponential.ui.compose

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.BlurMaskFilter
import android.graphics.Canvas
import android.graphics.ColorMatrixColorFilter
import android.graphics.DashPathEffect
import android.graphics.LinearGradient
import android.graphics.Matrix
import android.graphics.Paint
import android.graphics.Path
import android.graphics.PorterDuff
import android.graphics.PorterDuffXfermode
import android.graphics.RadialGradient
import android.graphics.Rect
import android.graphics.RectF
import android.graphics.Shader
import android.graphics.Typeface
import android.os.Build
import android.util.Base64
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/**
 * EXP-1604: one frame of a Motion, decoded from the packed display list
 * (`apps/desktop/crates/eui-motion/README.md` › Packed frames). The painter
 * paints this and nothing else: no Lottie, no clock.
 */
class MotionList(val width: Float, val height: Float, val ops: List<Op>) {
    class Stop(val offset: Float, val color: FloatArray)

    sealed class Brush {
        class Solid(val color: FloatArray) : Brush()
        class Linear(val x0: Float, val y0: Float, val x1: Float, val y1: Float, val stops: List<Stop>) : Brush()
        class Radial(val cx: Float, val cy: Float, val fx: Float, val fy: Float, val radius: Float, val stops: List<Stop>) : Brush()
    }

    sealed class Filter {
        class Blur(val sigmaX: Float, val sigmaY: Float) : Filter()
        /** Row-major 4×5, offsets in 0…1 (SVG `feColorMatrix`). */
        class ColorMatrix(val m: FloatArray) : Filter()
        class GradientMap(val amount: Float, val stops: List<Stop>) : Filter()
        class DropShadow(val color: FloatArray, val dx: Float, val dy: Float, val sigma: Float) : Filter()
    }

    class StrokeStyle(val width: Float, val cap: Int, val join: Int, val miter: Float, val offset: Float, val dashes: FloatArray)

    sealed class Op {
        class Layer(val blend: Int, val alpha: Float, val t: FloatArray, val clip: Path, val filters: List<Filter>) : Op()
        class Clip(val t: FloatArray, val path: Path) : Op()
        data object Pop : Op()
        class Fill(val t: FloatArray, val evenOdd: Boolean, val brush: Brush, val path: Path) : Op()
        class Stroke(val t: FloatArray, val style: StrokeStyle, val brush: Brush, val path: Path) : Op()
        class Image(val t: FloatArray, val asset: Int, val alpha: Float, val width: Float, val height: Float) : Op()

        /**
         * Round 8 (VERSION 2): one line of native text at the op's local
         * origin. [mono] = `font` 1; [weight] CSS (400…700); [align] 0 start |
         * 1 center | 2 end (physical); [baseline] 0 top | 1 middle | 2
         * alphabetic; [color] straight RGBA.
         */
        class Text(val t: FloatArray, val mono: Boolean, val weight: Int, val size: Float, val align: Int, val baseline: Int, val color: FloatArray, val text: String) : Op()
    }

    companion object {
        /** Decode a packed frame; null when it is not VERSION 1 or 2 (round 8: + `text`) or ends early. */
        fun decode(p: List<Float>): MotionList? = try {
            Reader(p).list()
        } catch (_: IndexOutOfBoundsException) {
            null
        } catch (_: IllegalStateException) {
            null
        }
    }

    private class Reader(val p: List<Float>) {
        var i = 0
        fun f(): Float = p[i++]
        fun n(): Int = p[i++].toInt()

        fun list(): MotionList {
            check(n() in 1..2)
            val w = f()
            val h = f()
            val ops = ArrayList<Op>()
            while (i < p.size) ops += op()
            return MotionList(w, h, ops)
        }

        fun t() = FloatArray(6) { f() }

        fun path(): Path {
            val path = Path()
            repeat(n()) {
                when (n()) {
                    0 -> path.moveTo(f(), f())
                    1 -> path.lineTo(f(), f())
                    2 -> path.cubicTo(f(), f(), f(), f(), f(), f())
                    3 -> path.close()
                    else -> error("verb")
                }
            }
            return path
        }

        fun color() = FloatArray(4) { f() }
        fun stops(): List<Stop> = List(n()) { Stop(f(), color()) }

        fun brush(): Brush = when (n()) {
            0 -> Brush.Solid(color())
            1 -> Brush.Linear(f(), f(), f(), f(), stops())
            2 -> Brush.Radial(f(), f(), f(), f(), f(), stops())
            else -> error("brush")
        }

        fun filter(): Filter = when (n()) {
            0 -> Filter.Blur(f(), f())
            1 -> Filter.ColorMatrix(FloatArray(20) { f() })
            2 -> Filter.GradientMap(f(), stops())
            3 -> Filter.DropShadow(color(), f(), f(), f())
            else -> error("filter")
        }

        fun op(): Op = when (n()) {
            1 -> {
                val blend = n()
                val alpha = f()
                val t = t()
                val clip = path()
                Op.Layer(blend, alpha, t, clip, List(n()) { filter() })
            }
            2 -> Op.Clip(t(), path())
            3 -> Op.Pop
            4 -> Op.Fill(t(), n() == 1, brush(), path())
            5 -> {
                val t = t()
                val style = StrokeStyle(f(), n(), n(), f(), f(), FloatArray(n()) { f() })
                Op.Stroke(t, style, brush(), path())
            }
            6 -> Op.Image(t(), n(), f(), f(), f())
            7 -> {
                val t = t()
                val mono = n() == 1
                val weight = n()
                val size = f()
                val align = n()
                val baseline = n()
                val color = color()
                val units = CharArray(n()) { n().toChar() }
                Op.Text(t, mono, weight, size, align, baseline, color, String(units))
            }
            else -> error("op")
        }
    }
}

/**
 * EXP-1604: paints a [MotionList] on an Android [Canvas] (contract §5, the
 * `raster` reference is the truth; both are Skia): every `layer` a
 * `saveLayer` under its clip, composited on `pop` with its alpha, blend and
 * filters. Compose reaches it through `drawIntoCanvas { it.nativeCanvas }`.
 */
object MotionPainter {
    /** The fit transform of a `w × h` composition into `boxW × boxH` (§4): [sx, sy, tx, ty]. */
    fun fit(w: Float, h: Float, boxW: Float, boxH: Float, mode: String): FloatArray {
        if (w <= 0f || h <= 0f) return floatArrayOf(1f, 1f, 0f, 0f)
        val sx = boxW / w
        val sy = boxH / h
        if (mode == "fill") return floatArrayOf(sx, sy, 0f, 0f)
        val s = if (mode == "cover") max(sx, sy) else min(sx, sy)
        return floatArrayOf(s, s, (boxW - w * s) / 2f, (boxH - h * s) / 2f)
    }

    /**
     * Round 8: the typeface of a `text` op ([mono], CSS [weight]): the
     * theme's `$type.family.sans` / `.mono` at that weight; null = the
     * platform's sans / monospace.
     */
    fun interface Typefaces {
        fun typeface(mono: Boolean, weight: Int): Typeface?
    }

    /** The platform faces (no theme): sans-serif / monospace at the CSS weight. */
    val PLATFORM_TYPEFACES = Typefaces { mono, weight -> weighted(if (mono) Typeface.MONOSPACE else Typeface.SANS_SERIF, weight) }

    /** [base] at a CSS weight (API 28+: exact; older: bold from 600). */
    fun weighted(base: Typeface, weight: Int): Typeface =
        if (Build.VERSION.SDK_INT >= 28) Typeface.create(base, weight.coerceIn(1, 1000), false)
        else Typeface.create(base, if (weight >= 600) Typeface.BOLD else Typeface.NORMAL)

    /** Paint [list] into the `boxW × boxH` px box at the canvas origin (clipped to it); `text` ops in [typefaces]. */
    fun paint(canvas: Canvas, list: MotionList, boxW: Float, boxH: Float, fit: String = "contain", images: List<Bitmap?> = emptyList(), typefaces: Typefaces = PLATFORM_TYPEFACES) {
        val f = fit(list.width, list.height, boxW, boxH, fit)
        val save = canvas.save()
        canvas.clipRect(0f, 0f, boxW, boxH)
        canvas.translate(f[2], f[3])
        canvas.scale(f[0], f[1])
        val scale = sqrt(abs(f[0] * f[1]))
        Run(list.ops, canvas, scale, images, typefaces).group(0, Mods())
        canvas.restoreToCount(save)
    }

    /** What a filtered group changes on every draw it repeats (blur, shadow tint). */
    private class Mods(val blur: Float = 0f)

    private class Run(val ops: List<MotionList.Op>, val canvas: Canvas, val scale: Float, val images: List<Bitmap?>, val typefaces: Typefaces) {
        /** Paint ops from [start] to the `pop` closing the group; the index after it. */
        fun group(start: Int, mods: Mods): Int {
            var i = start
            while (i < ops.size) {
                val op = ops[i++]
                when (op) {
                    is MotionList.Op.Pop -> return i
                    is MotionList.Op.Clip -> {
                        val save = canvas.save()
                        canvas.clipPath(transformed(op.path, op.t))
                        i = group(i, mods)
                        canvas.restoreToCount(save)
                    }
                    is MotionList.Op.Layer -> i = layer(op, i, mods)
                    is MotionList.Op.Fill -> {
                        val path = Path(op.path).apply { fillType = if (op.evenOdd) Path.FillType.EVEN_ODD else Path.FillType.WINDING }
                        draw(op.t, path, op.brush, Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.FILL }, mods)
                    }
                    is MotionList.Op.Stroke -> {
                        val s = op.style
                        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            style = Paint.Style.STROKE
                            strokeWidth = s.width
                            strokeCap = when (s.cap) { 1 -> Paint.Cap.ROUND; 2 -> Paint.Cap.SQUARE; else -> Paint.Cap.BUTT }
                            strokeJoin = when (s.join) { 1 -> Paint.Join.ROUND; 2 -> Paint.Join.BEVEL; else -> Paint.Join.MITER }
                            strokeMiter = s.miter
                            var d = s.dashes
                            if (d.size % 2 == 1) d += d
                            if (d.isNotEmpty() && d.any { it > 0f } && d.none { it < 0f }) pathEffect = DashPathEffect(d, s.offset)
                        }
                        draw(op.t, op.path, op.brush, paint, mods)
                    }
                    is MotionList.Op.Image -> {
                        val img = images.getOrNull(op.asset) ?: continue
                        val save = canvas.save()
                        canvas.concat(matrix(op.t))
                        val w = if (op.width > 0f) op.width else img.width.toFloat()
                        val h = if (op.height > 0f) op.height else img.height.toFloat()
                        val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG).apply { alpha = (op.alpha.coerceIn(0f, 1f) * 255f).toInt() }
                        canvas.drawBitmap(img, Rect(0, 0, img.width, img.height), RectF(0f, 0f, w, h), paint)
                        canvas.restoreToCount(save)
                    }
                    is MotionList.Op.Text -> text(op)
                }
            }
            return i
        }

        fun layer(op: MotionList.Op.Layer, start: Int, mods: Mods): Int {
            val clip = transformed(op.clip, op.t)
            val bounds = RectF().also { clip.computeBounds(it, true) }
            val save = canvas.save()
            canvas.clipPath(clip)
            val paint = Paint().apply {
                alpha = (op.alpha.coerceIn(0f, 1f) * 255f + 0.5f).toInt()
                blend(this, op.blend)
            }
            val shadow = op.filters.filterIsInstance<MotionList.Filter.DropShadow>().lastOrNull()
            val matrices = op.filters.mapNotNull { colorMatrix(it) }
            if (matrices.isNotEmpty()) paint.colorFilter = ColorMatrixColorFilter(matrices.reduce { a, b -> android.graphics.ColorMatrix(b).apply { preConcat(a) } })
            val blur = op.filters.filterIsInstance<MotionList.Filter.Blur>().fold(mods.blur) { b, f -> max(b, max(f.sigmaX, f.sigmaY) * scale) }
            val inner = Mods(blur)
            canvas.saveLayer(bounds, paint)
            var end = start
            if (shadow != null) {
                // The group again, tinted, offset and blurred, under itself.
                val tint = Paint().apply { colorFilter = ColorMatrixColorFilter(shadowMatrix(shadow.color)) }
                val s = canvas.saveLayer(null, tint)
                canvas.translate(shadow.dx, shadow.dy)
                group(start, Mods(max(blur, shadow.sigma * scale)))
                canvas.restoreToCount(s)
            }
            if (op.blend == 18 || op.blend == 19) {
                // The matte over opaque black, then luminance → alpha: Rec. 709
                // of the PREMULTIPLIED colour (the raster's `luma_mask`).
                val s = canvas.saveLayer(bounds, Paint().apply { colorFilter = ColorMatrixColorFilter(LUMA) })
                canvas.drawColor(android.graphics.Color.BLACK)
                end = group(start, inner)
                canvas.restoreToCount(s)
            } else {
                end = group(start, inner)
            }
            canvas.restoreToCount(save)
            return end
        }

        fun draw(t: FloatArray, path: Path, brush: MotionList.Brush, paint: Paint, mods: Mods) {
            when (brush) {
                is MotionList.Brush.Solid -> paint.color = argb(brush.color)
                is MotionList.Brush.Linear -> {
                    if (brush.stops.isEmpty()) return
                    if (brush.x0 == brush.x1 && brush.y0 == brush.y1) paint.color = argb(brush.stops.last().color)
                    else straight(brush.stops).let { s -> paint.shader = LinearGradient(brush.x0, brush.y0, brush.x1, brush.y1, colors(s), offsets(s), Shader.TileMode.CLAMP) }
                }
                is MotionList.Brush.Radial -> {
                    if (brush.stops.isEmpty() || brush.radius <= 0f) return
                    val stops = straight(brush.stops)
                    paint.shader = if ((brush.fx != brush.cx || brush.fy != brush.cy) && Build.VERSION.SDK_INT >= 31) {
                        // Two-point conical: the focal circle (radius 0) → the centre circle.
                        RadialGradient(brush.fx, brush.fy, 0f, brush.cx, brush.cy, brush.radius, longColors(stops), offsets(stops), Shader.TileMode.CLAMP)
                    } else {
                        RadialGradient(brush.cx, brush.cy, brush.radius, colors(stops), offsets(stops), Shader.TileMode.CLAMP)
                    }
                }
            }
            if (mods.blur > 0f) paint.maskFilter = BlurMaskFilter(mods.blur, BlurMaskFilter.Blur.NORMAL)
            val save = canvas.save()
            canvas.concat(matrix(t))
            canvas.drawPath(path, paint)
            canvas.restoreToCount(save)
        }

        /**
         * Round 8: one line at the op's origin through the canvas matrix
         * (fit × transform, so `size` scales with it): align physical
         * (Paint.Align), the baseline from the face's own ascent / descent
         * (Paint.FontMetrics, no leading). Only the box clips it.
         */
        fun text(op: MotionList.Op.Text) {
            if (op.text.isEmpty() || op.size <= 0f) return
            val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.SUBPIXEL_TEXT_FLAG or Paint.LINEAR_TEXT_FLAG).apply {
                typeface = typefaces.typeface(op.mono, op.weight) ?: PLATFORM_TYPEFACES.typeface(op.mono, op.weight)
                textSize = op.size
                color = argb(op.color)
                textAlign = when (op.align) { 1 -> Paint.Align.CENTER; 2 -> Paint.Align.RIGHT; else -> Paint.Align.LEFT }
            }
            val fm = paint.fontMetrics
            val ascent = -fm.ascent
            val descent = fm.descent
            val y = when (op.baseline) {
                0 -> ascent
                1 -> (ascent - descent) / 2f
                else -> 0f
            }
            val save = canvas.save()
            canvas.concat(matrix(op.t))
            canvas.drawText(op.text, 0f, y, paint)
            canvas.restoreToCount(save)
        }

        fun colorMatrix(f: MotionList.Filter): android.graphics.ColorMatrix? = when (f) {
            is MotionList.Filter.ColorMatrix -> android.graphics.ColorMatrix(svg(f.m))
            is MotionList.Filter.GradientMap -> {
                // The map's end stops (luminance 0 → first, 1 → last) as one
                // colour matrix, mixed with the source by `amount`.
                val lo = f.stops.firstOrNull()?.color
                val hi = f.stops.lastOrNull()?.color
                if (lo == null || hi == null) null else {
                    val l = floatArrayOf(0.2125f, 0.7154f, 0.0721f)
                    val m = FloatArray(20)
                    for (row in 0 until 3) {
                        for (col in 0 until 3) m[row * 5 + col] = f.amount * l[col] * (hi[row] - lo[row]) + if (row == col) 1 - f.amount else 0f
                        m[row * 5 + 4] = f.amount * lo[row]
                    }
                    m[18] = 1f
                    android.graphics.ColorMatrix(svg(m))
                }
            }
            else -> null
        }
    }

    /**
     * The raster interpolates gradient colours UNpremultiplied, Android's
     * Skia premultiplied: where alpha changes between two stops, 16 stops in
     * between carry the straight interpolation (≤ 1/255 apart).
     */
    fun straight(stops: List<MotionList.Stop>): List<MotionList.Stop> {
        if (stops.size < 2) return stops
        val out = arrayListOf(stops[0])
        for ((a, b) in stops.zipWithNext()) {
            if (a.color[3] != b.color[3] && b.offset > a.offset) {
                for (k in 1 until 16) {
                    val t = k / 16f
                    out += MotionList.Stop(a.offset + (b.offset - a.offset) * t, FloatArray(4) { a.color[it] + (b.color[it] - a.color[it]) * t })
                }
            }
            out += b
        }
        return out
    }

    /** SVG offsets (0…1) → Android's (0…255). */
    private fun svg(m: FloatArray): FloatArray = m.copyOf().also { for (r in 0 until 4) it[r * 5 + 4] = m[r * 5 + 4] * 255f }

    private fun shadowMatrix(c: FloatArray): FloatArray = floatArrayOf(
        0f, 0f, 0f, 0f, c[0] * 255f,
        0f, 0f, 0f, 0f, c[1] * 255f,
        0f, 0f, 0f, 0f, c[2] * 255f,
        0f, 0f, 0f, c[3], 0f,
    )

    /** Luminance → alpha (feColorMatrix `luminanceToAlpha`). */
    private val LUMA = floatArrayOf(
        0f, 0f, 0f, 0f, 0f,
        0f, 0f, 0f, 0f, 0f,
        0f, 0f, 0f, 0f, 0f,
        0.2125f, 0.7154f, 0.0721f, 0f, 0f,
    )

    /** tiny-skia `from_row(sx, ky, kx, sy, tx, ty)` → an Android matrix. */
    fun matrix(t: FloatArray): Matrix = Matrix().apply { setValues(floatArrayOf(t[0], t[2], t[4], t[1], t[3], t[5], 0f, 0f, 1f)) }

    private fun transformed(path: Path, t: FloatArray): Path = Path(path).apply { transform(matrix(t)) }

    private fun channel(v: Float): Int = (v.coerceIn(0f, 1f) * 255f + 0.5f).toInt()
    private fun argb(c: FloatArray): Int = android.graphics.Color.argb(channel(c[3]), channel(c[0]), channel(c[1]), channel(c[2]))
    private fun colors(s: List<MotionList.Stop>): IntArray = IntArray(s.size) { argb(s[it].color) }
    private fun longColors(s: List<MotionList.Stop>): LongArray = LongArray(s.size) { android.graphics.Color.pack(s[it].color[0].coerceIn(0f, 1f), s[it].color[1].coerceIn(0f, 1f), s[it].color[2].coerceIn(0f, 1f), s[it].color[3].coerceIn(0f, 1f)) }
    private fun offsets(s: List<MotionList.Stop>): FloatArray = FloatArray(s.size) { s[it].offset }

    private fun blend(p: Paint, b: Int) {
        if (Build.VERSION.SDK_INT >= 29) {
            p.blendMode = when (b) {
                1 -> android.graphics.BlendMode.MULTIPLY
                2 -> android.graphics.BlendMode.SCREEN
                3 -> android.graphics.BlendMode.OVERLAY
                4 -> android.graphics.BlendMode.DARKEN
                5 -> android.graphics.BlendMode.LIGHTEN
                6 -> android.graphics.BlendMode.COLOR_DODGE
                7 -> android.graphics.BlendMode.COLOR_BURN
                8 -> android.graphics.BlendMode.HARD_LIGHT
                9 -> android.graphics.BlendMode.SOFT_LIGHT
                10 -> android.graphics.BlendMode.DIFFERENCE
                11 -> android.graphics.BlendMode.EXCLUSION
                12 -> android.graphics.BlendMode.HUE
                13 -> android.graphics.BlendMode.SATURATION
                14 -> android.graphics.BlendMode.COLOR
                15 -> android.graphics.BlendMode.LUMINOSITY
                16, 18 -> android.graphics.BlendMode.DST_IN
                17, 19 -> android.graphics.BlendMode.DST_OUT
                20 -> android.graphics.BlendMode.XOR
                else -> android.graphics.BlendMode.SRC_OVER
            }
            return
        }
        val mode = when (b) {
            1 -> PorterDuff.Mode.MULTIPLY
            2 -> PorterDuff.Mode.SCREEN
            3 -> PorterDuff.Mode.OVERLAY
            4 -> PorterDuff.Mode.DARKEN
            5 -> PorterDuff.Mode.LIGHTEN
            16, 18 -> PorterDuff.Mode.DST_IN
            17, 19 -> PorterDuff.Mode.DST_OUT
            20 -> PorterDuff.Mode.XOR
            else -> null
        }
        if (mode != null) p.xfermode = PorterDuffXfermode(mode)
    }

    /** Decode an image asset location: a `data:` URL (base64) into a bitmap. */
    fun decodeDataUrl(location: String): Bitmap? {
        if (!location.startsWith("data:")) return null
        val comma = location.indexOf(',').takeIf { it > 0 } ?: return null
        val bytes = try {
            if (location.substring(0, comma).endsWith(";base64")) Base64.decode(location.substring(comma + 1), Base64.DEFAULT)
            else java.net.URLDecoder.decode(location.substring(comma + 1), "UTF-8").toByteArray()
        } catch (_: IllegalArgumentException) {
            return null
        }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size)
    }
}
