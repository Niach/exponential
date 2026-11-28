package at.exponential.ui.measure

import android.content.Context
import android.content.res.AssetManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontListFontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.ResourceFont
import androidx.compose.ui.text.font.SystemFontFamily
import java.io.File
import java.nio.ByteBuffer

/**
 * The per-glyph `Inter Symbols` fallback of the shaper ([INTER_SYMBOLS_FAMILY]):
 * Compose has no cross-family fallback, so a symbol the run's face lacks
 * gets a span in [family], as CSS font matching walks
 * `"<theme family>", "Inter Symbols", <platform>`. A symbol the face HAS
 * stays in it (Geist draws its own ⇧ ↩ arrows); a family that is not
 * registered here (a [SystemFontFamily]: the platform default, serif,
 * monospace) is a missing first entry, so every symbol takes Inter Symbols;
 * a face whose cmap cannot be read keeps its symbols (the platform as
 * before). The measurer ([TextShaper.layout]) and the painters apply the
 * same spans, so measured = painted.
 */
internal class SymbolFallback(private val fonts: FontResolver, private val context: Context?) {
    /** The host's / registered `Inter Symbols` (the conformance fonts), else the bundled faces. */
    val family: FontFamily by lazy { fonts.family(INTER_SYMBOLS_FAMILY) ?: InterSymbols.bundled }

    /** Per (family, weight, italic): the symbols its matched face covers. */
    private val masks = HashMap<Triple<FontFamily, Int, Boolean>, Int>()

    /** [text] with the fallback spans; null = none applies (paint the String as is). */
    fun apply(text: String, style: TextStyle): AnnotatedString? {
        if (!InterSymbols.any(text)) return null
        val runs = missing(text, style, emptyList()) ?: return null
        val b = AnnotatedString.Builder(text)
        for (r in runs) b.addStyle(SpanStyle(fontFamily = family), r.first, r.last + 1)
        return b.toAnnotatedString()
    }

    /** [text] with the fallback spans; the same instance when none applies. */
    fun apply(text: AnnotatedString, style: TextStyle): AnnotatedString {
        if (!InterSymbols.any(text.text)) return text
        val runs = missing(text.text, style, text.spanStyles) ?: return text
        val b = AnnotatedString.Builder(text)
        for (r in runs) b.addStyle(SpanStyle(fontFamily = family), r.first, r.last + 1)
        return b.toAnnotatedString()
    }

    /** The ranges of symbols their run's face lacks (null = none); spans in order, a later one overriding. */
    private fun missing(s: String, style: TextStyle, spans: List<AnnotatedString.Range<SpanStyle>>): List<IntRange>? {
        var out: ArrayList<IntRange>? = null
        for (i in s.indices) {
            val bit = InterSymbols.bit(s[i].code)
            if (bit == 0) continue
            var family = style.fontFamily
            var weight = style.fontWeight?.weight ?: 400
            var italic = style.fontStyle == FontStyle.Italic
            for (r in spans) {
                if (i < r.start || i >= r.end) continue
                r.item.fontFamily?.let { family = it }
                r.item.fontWeight?.let { weight = it.weight }
                r.item.fontStyle?.let { italic = it == FontStyle.Italic }
            }
            if (covers(family, weight, italic, bit)) continue
            val runs = out ?: ArrayList<IntRange>().also { out = it }
            val last = runs.lastOrNull()
            if (last != null && last.last == i - 1) runs[runs.size - 1] = last.first..i else runs.add(i..i)
        }
        return out
    }

    private fun covers(family: FontFamily?, weight: Int, italic: Boolean, bit: Int): Boolean {
        if (family == null || family is SystemFontFamily) return false
        if (family == this.family) return true
        val key = Triple(family, weight, italic)
        val mask = synchronized(masks) { masks[key] } ?: when (family) {
            is FontListFontFamily -> matchFace(family.fonts, weight, italic)?.let { FontCoverage.mask(it, context) } ?: InterSymbols.ALL
            // A loaded Typeface: its coverage is unknown, it keeps the symbols.
            else -> InterSymbols.ALL
        }.also { m -> synchronized(masks) { masks[key] = m } }
        return mask and bit != 0
    }

    companion object {
        /**
         * The face of [fonts] CSS font matching picks for [weight] / [italic]
         * (Compose's own `FontMatcher` is internal): the style first, then
         * 400–500 look up to 500, then down, then up; below 400 down then up;
         * above 500 up then down.
         */
        fun matchFace(fonts: List<Font>, weight: Int, italic: Boolean): Font? {
            if (fonts.isEmpty()) return null
            val wanted = if (italic) FontStyle.Italic else FontStyle.Normal
            val pool = fonts.filter { it.style == wanted }.ifEmpty { fonts }
            pool.firstOrNull { it.weight.weight == weight }?.let { return it }
            val below = pool.filter { it.weight.weight < weight }.maxByOrNull { it.weight.weight }
            val above = pool.filter { it.weight.weight > weight }.minByOrNull { it.weight.weight }
            return when {
                weight in 400..500 -> pool.filter { it.weight.weight in (weight + 1)..500 }.minByOrNull { it.weight.weight } ?: below ?: above
                weight < 400 -> below ?: above
                else -> above ?: below
            }
        }
    }
}

/**
 * Which Inter Symbols codepoints a font file maps (its `cmap`), as a mask
 * over [INTER_SYMBOLS_CODEPOINTS]; read once per face. A [ResourceFont]
 * reads through the context's resources; a file or asset font (Compose's
 * internal `AndroidFileFont` / `AndroidAssetFont`) through its public
 * getters, by reflection; anything else (or an unreadable file) = null.
 */
internal object FontCoverage {
    private val cache = HashMap<Font, Int>()

    fun mask(font: Font, context: Context?): Int? {
        synchronized(cache) { cache[font]?.let { return it } }
        val bytes = runCatching { bytes(font, context) }.getOrNull() ?: return null
        val mask = runCatching { cmapMask(ByteBuffer.wrap(bytes)) }.getOrNull() ?: return null
        synchronized(cache) { cache[font] = mask }
        return mask
    }

    private fun bytes(font: Font, context: Context?): ByteArray? {
        if (font is ResourceFont) return context?.resources?.openRawResource(font.resId)?.use { it.readBytes() }
        val type = font.javaClass
        val getter = { name: String -> type.methods.firstOrNull { it.name == name && it.parameterTypes.isEmpty() }?.invoke(font) }
        (getter("getFile") as? File)?.let { return it.readBytes() }
        val assets = getter("getAssetManager") as? AssetManager ?: return null
        val path = getter("getPath") as? String ?: return null
        return assets.open(path).use { it.readBytes() }
    }

    /** The mask of the first face's best Unicode cmap subtable (format 12, else 4); null = not an sfnt with one. */
    fun cmapMask(buf: ByteBuffer): Int? {
        var font = 0
        // A collection ('ttcf'): its first face.
        if (buf.getInt(0) == 0x74746366) font = buf.getInt(12)
        val tables = buf.getShort(font + 4).toInt() and 0xFFFF
        var cmap = -1
        for (t in 0 until tables) {
            val rec = font + 12 + t * 16
            if (buf.getInt(rec) == 0x636D6170) cmap = buf.getInt(rec + 8)
        }
        if (cmap < 0) return null
        val subtables = buf.getShort(cmap + 2).toInt() and 0xFFFF
        var best = -1
        var bestRank = 0
        for (s in 0 until subtables) {
            val rec = cmap + 4 + s * 8
            val platform = buf.getShort(rec).toInt() and 0xFFFF
            val encoding = buf.getShort(rec + 2).toInt() and 0xFFFF
            val at = cmap + buf.getInt(rec + 4)
            val format = buf.getShort(at).toInt() and 0xFFFF
            val unicode = platform == 0 || (platform == 3 && (encoding == 1 || encoding == 10))
            val rank = when {
                !unicode -> 0
                format == 12 -> 2
                format == 4 -> 1
                else -> 0
            }
            if (rank > bestRank) {
                best = at
                bestRank = rank
            }
        }
        if (best < 0) return null
        var mask = 0
        for ((i, cp) in INTER_SYMBOLS_CODEPOINTS.withIndex()) {
            val glyph = if (bestRank == 2) glyph12(buf, best, cp) else glyph4(buf, best, cp)
            if (glyph != 0) mask = mask or (1 shl i)
        }
        return mask
    }

    private fun glyph12(buf: ByteBuffer, at: Int, cp: Int): Int {
        val groups = buf.getInt(at + 12)
        for (g in 0 until groups) {
            val rec = at + 16 + g * 12
            val start = buf.getInt(rec)
            val end = buf.getInt(rec + 4)
            if (cp in start..end) return buf.getInt(rec + 8) + (cp - start)
        }
        return 0
    }

    private fun glyph4(buf: ByteBuffer, at: Int, cp: Int): Int {
        val segX2 = buf.getShort(at + 6).toInt() and 0xFFFF
        val ends = at + 14
        val starts = ends + segX2 + 2
        val deltas = starts + segX2
        val offsets = deltas + segX2
        for (k in 0 until segX2 / 2) {
            val end = buf.getShort(ends + 2 * k).toInt() and 0xFFFF
            if (end < cp) continue
            val start = buf.getShort(starts + 2 * k).toInt() and 0xFFFF
            if (start > cp) return 0
            val delta = buf.getShort(deltas + 2 * k).toInt()
            val rangeAt = offsets + 2 * k
            val range = buf.getShort(rangeAt).toInt() and 0xFFFF
            if (range == 0) return (cp + delta) and 0xFFFF
            val glyph = buf.getShort(rangeAt + range + 2 * (cp - start)).toInt() and 0xFFFF
            return if (glyph == 0) 0 else (glyph + delta) and 0xFFFF
        }
        return 0
    }
}
