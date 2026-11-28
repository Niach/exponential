package at.exponential.ui.measure

import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import at.exponential.ui.R

/**
 * `Inter Symbols`: the key-symbol fallback every eUI renderer puts right
 * after the theme family, ahead of any OS fallback (the web's
 * `"Geist", "Inter Symbols", ui-sans-serif, …`). Geist and Nunito have no
 * ⌘, so without it each OS substituted its own face and a Kbd "⌘S"
 * measured differently per platform. The ONE list is
 * `packages/exponential-ui/fonts/inter-symbols.json` (its `codepoints`, in
 * that order); `src/web/inter-symbols.test.ts` fails when this copy, the
 * family name or the bundled faces drift from it.
 */
const val INTER_SYMBOLS_FAMILY = "Inter Symbols"

/** The codepoints `Inter Symbols` covers (`inter-symbols.json` `codepoints[].cp`, same order). */
val INTER_SYMBOLS_CODEPOINTS: IntArray = intArrayOf(
    0x2318, 0x2325, 0x21E7, 0x2303, 0x21EA, 0x21A9, 0x23CE, 0x21B5,
    0x2305, 0x232B, 0x2326, 0x238B, 0x21E5, 0x21E4, 0x2423, 0x23CF,
    0x2190, 0x2191, 0x2192, 0x2193, 0x21DE, 0x21DF, 0x2196, 0x2198,
)

internal object InterSymbols {
    /** The bundled faces (`res/font/inter_symbols_*.ttf`, SIL OFL 1.1, `OFL-Inter.txt`): what a host that registers no `Inter Symbols` gets. */
    val bundled: FontFamily by lazy {
        FontFamily(
            Font(R.font.inter_symbols_regular, FontWeight.W400),
            Font(R.font.inter_symbols_medium, FontWeight.W500),
            Font(R.font.inter_symbols_semibold, FontWeight.W600),
            Font(R.font.inter_symbols_bold, FontWeight.W700),
        )
    }

    /** A mask with every bit set: a face that keeps every symbol (it covers them, or its coverage is unknown). */
    const val ALL: Int = -1

    private val low = INTER_SYMBOLS_CODEPOINTS.min()
    private val high = INTER_SYMBOLS_CODEPOINTS.max()
    private val bits: Map<Int, Int> = INTER_SYMBOLS_CODEPOINTS.withIndex().associate { (i, cp) -> cp to (1 shl i) }

    init {
        check(INTER_SYMBOLS_CODEPOINTS.size <= 31) { "a coverage mask holds 31 symbols" }
    }

    /** The mask bit of [cp] (0 = not an Inter Symbols codepoint). Every symbol is in the BMP: one UTF-16 unit. */
    fun bit(cp: Int): Int = if (cp < low || cp > high) 0 else bits[cp] ?: 0

    /** Whether [text] holds any Inter Symbols codepoint (the cheap gate: no allocation). */
    fun any(text: CharSequence): Boolean {
        for (i in 0 until text.length) {
            val c = text[i].code
            if (c in low..high && bits.containsKey(c)) return true
        }
        return false
    }
}
