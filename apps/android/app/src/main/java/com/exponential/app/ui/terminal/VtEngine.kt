package com.exponential.app.ui.terminal

// Minimal VT/ANSI terminal emulator for the remote steer mirror (masterplan
// §5c). Written from scratch for this app (no third-party terminal code): a
// fixed cell grid + scrollback fed verbatim PTY bytes from the relay. Scope —
// enough for a *mirror* of a desktop terminal running `claude`:
//   - UTF-8 decode (incremental, frames may split multi-byte chars)
//   - C0 controls (BS/TAB/CR/LF), deferred autowrap
//   - CSI cursor addressing (CUP/CUU/CUD/CUF/CUB/CHA/VPA/CNL/CPL)
//   - erase (ED/EL/ECH), insert/delete (ICH/DCH/IL/DL), scroll (SU/SD), REP
//   - SGR colors: 16/256/truecolor, bold, underline, inverse
//   - DECSTBM scroll region, alt screen (?47/?1047/?1049), cursor visibility,
//     autowrap mode; OSC/DCS/PM/APC strings are consumed and discarded
// It never answers back (a viewer socket has no DA/DSR reply path — the real
// terminal lives on the desktop). Pure Kotlin/JVM so it unit-tests off-device.

const val VT_COLOR_DEFAULT = -1

/** Truecolor values are packed as `VT_TRUECOLOR or (r shl 16) or (g shl 8) or b`. */
const val VT_TRUECOLOR = 0x1_000_000

data class VtCell(
    val text: String = " ",
    val fg: Int = VT_COLOR_DEFAULT,
    val bg: Int = VT_COLOR_DEFAULT,
    val bold: Boolean = false,
    val underline: Boolean = false,
    val inverse: Boolean = false,
) {
    companion object {
        val BLANK = VtCell()
    }
}

/** Immutable render snapshot: scrollback (main screen only) + the live grid. */
data class VtSnapshot(
    val cols: Int,
    val rows: Int,
    val cells: List<List<VtCell>>,
    val scrollback: List<List<VtCell>>,
    val cursorRow: Int,
    val cursorCol: Int,
    val cursorVisible: Boolean,
)

class VtEngine(
    cols: Int = 80,
    rows: Int = 24,
    private val maxScrollback: Int = 2000,
) {
    var cols: Int = cols.coerceIn(1, 1000)
        private set
    var rows: Int = rows.coerceIn(1, 1000)
        private set

    private var main: Array<Array<VtCell>> = blankGrid(this.cols, this.rows)
    private var alt: Array<Array<VtCell>> = blankGrid(this.cols, this.rows)
    private var inAlt = false
    private val grid: Array<Array<VtCell>> get() = if (inAlt) alt else main
    private val scrollbackBuf = ArrayDeque<List<VtCell>>()

    var cursorRow = 0
        private set
    var cursorCol = 0
        private set
    var cursorVisible = true
        private set

    private var pendingWrap = false
    private var autowrap = true
    private var scrollTop = 0
    private var scrollBottom = this.rows - 1

    // Current SGR attributes.
    private var fg = VT_COLOR_DEFAULT
    private var bg = VT_COLOR_DEFAULT
    private var bold = false
    private var underline = false
    private var inverse = false

    private data class SavedCursor(
        val row: Int,
        val col: Int,
        val fg: Int,
        val bg: Int,
        val bold: Boolean,
        val underline: Boolean,
        val inverse: Boolean,
    )

    private var savedMain: SavedCursor? = null
    private var savedAlt: SavedCursor? = null

    private var lastPrinted: String? = null

    // ── Parser state ─────────────────────────────────────────────────────────

    private enum class State { GROUND, ESC, CSI, OSC, OSC_ESC, STR, STR_ESC, CHARSET }

    private var state = State.GROUND
    private val csiBuf = StringBuilder()

    // Incremental UTF-8 decode: continuation bytes still expected + partial cp.
    private var utfPending = 0
    private var utfCp = 0

    // ── Feeding ──────────────────────────────────────────────────────────────

    fun feed(bytes: ByteArray, offset: Int = 0, length: Int = bytes.size - offset) {
        var i = offset
        val end = offset + length
        while (i < end) {
            val b = bytes[i].toInt() and 0xFF
            if (utfPending > 0) {
                if (b in 0x80..0xBF) {
                    utfCp = (utfCp shl 6) or (b and 0x3F)
                    utfPending--
                    if (utfPending == 0) handleCodePoint(utfCp)
                    i++
                } else {
                    // Malformed sequence: emit replacement, reprocess this byte.
                    utfPending = 0
                    handleCodePoint(0xFFFD)
                }
            } else {
                when {
                    b < 0x80 -> handleCodePoint(b)
                    b in 0xC2..0xDF -> { utfCp = b and 0x1F; utfPending = 1 }
                    b in 0xE0..0xEF -> { utfCp = b and 0x0F; utfPending = 2 }
                    b in 0xF0..0xF4 -> { utfCp = b and 0x07; utfPending = 3 }
                    else -> handleCodePoint(0xFFFD)
                }
                i++
            }
        }
    }

    /** Convenience for tests / text frames. */
    fun feed(text: String) = feed(text.toByteArray(Charsets.UTF_8))

    // ── Public geometry / inspection ─────────────────────────────────────────

    fun resize(newCols: Int, newRows: Int) {
        val c = newCols.coerceIn(1, 1000)
        val r = newRows.coerceIn(1, 1000)
        if (c == cols && r == rows) return
        main = regrid(main, c, r)
        alt = regrid(alt, c, r)
        cols = c
        rows = r
        scrollTop = 0
        scrollBottom = r - 1
        cursorRow = cursorRow.coerceIn(0, r - 1)
        cursorCol = cursorCol.coerceIn(0, c - 1)
        pendingWrap = false
    }

    fun snapshot(): VtSnapshot = VtSnapshot(
        cols = cols,
        rows = rows,
        cells = grid.map { it.toList() },
        scrollback = if (inAlt) emptyList() else scrollbackBuf.toList(),
        cursorRow = cursorRow,
        cursorCol = cursorCol,
        cursorVisible = cursorVisible,
    )

    /** The visible text of one grid row, trailing blanks trimmed (test helper). */
    fun rowText(row: Int): String =
        grid[row].joinToString("") { it.text }.trimEnd()

    fun cellAt(row: Int, col: Int): VtCell = grid[row][col]

    val isAltScreen: Boolean get() = inAlt
    val scrollbackSize: Int get() = scrollbackBuf.size

    // ── Parser ───────────────────────────────────────────────────────────────

    private fun handleCodePoint(cp: Int) {
        when (state) {
            State.GROUND -> when {
                cp == 0x1B -> state = State.ESC
                cp < 0x20 || cp == 0x7F -> control(cp)
                else -> print(cp)
            }
            State.ESC -> handleEsc(cp)
            State.CSI -> handleCsiByte(cp)
            State.OSC -> when (cp) {
                0x07 -> state = State.GROUND
                0x1B -> state = State.OSC_ESC
                else -> Unit // swallow string content
            }
            State.OSC_ESC ->
                if (cp == '\\'.code) {
                    state = State.GROUND
                } else {
                    // ESC inside OSC terminates the string and starts a new escape.
                    state = State.ESC
                    handleEsc(cp)
                }
            State.STR -> when (cp) {
                0x07 -> state = State.GROUND
                0x1B -> state = State.STR_ESC
                else -> Unit
            }
            State.STR_ESC ->
                if (cp == '\\'.code) {
                    state = State.GROUND
                } else {
                    state = State.ESC
                    handleEsc(cp)
                }
            State.CHARSET -> state = State.GROUND // consume the designator
        }
    }

    private fun control(cp: Int) {
        when (cp) {
            0x08 -> { if (cursorCol > 0) cursorCol--; pendingWrap = false }
            0x09 -> {
                cursorCol = (((cursorCol / 8) + 1) * 8).coerceAtMost(cols - 1)
                pendingWrap = false
            }
            0x0A, 0x0B, 0x0C -> index()
            0x0D -> { cursorCol = 0; pendingWrap = false }
            else -> Unit // BEL, NUL, SO/SI, DEL … ignored
        }
    }

    private fun handleEsc(cp: Int) {
        state = State.GROUND
        when (cp.toChar()) {
            '[' -> { csiBuf.setLength(0); state = State.CSI }
            ']' -> state = State.OSC
            'P', '^', '_' -> state = State.STR
            '(', ')', '*', '+' -> state = State.CHARSET
            '7' -> saveCursor()
            '8' -> restoreCursor()
            'D' -> index()
            'M' -> reverseIndex()
            'E' -> { cursorCol = 0; pendingWrap = false; index() }
            'c' -> resetAll()
            else -> Unit // '=' '>' keypad modes etc.
        }
    }

    private fun handleCsiByte(cp: Int) {
        when {
            cp in 0x40..0x7E -> { state = State.GROUND; dispatchCsi(cp.toChar()) }
            cp in 0x20..0x3F -> { if (csiBuf.length < 64) csiBuf.append(cp.toChar()) }
            cp == 0x1B -> state = State.ESC
            cp == 0x18 || cp == 0x1A -> state = State.GROUND // CAN/SUB abort
            else -> control(cp) // C0 controls execute inside CSI
        }
    }

    private fun dispatchCsi(final: Char) {
        val raw = csiBuf.toString()
        val private = raw.startsWith("?")
        val paramStr = raw.dropWhile { it == '?' || it == '>' || it == '<' || it == '=' }
            .filter { it.isDigit() || it == ';' || it == ':' }
        val groups: List<IntArray> =
            if (paramStr.isEmpty()) emptyList()
            else paramStr.split(';').map { g ->
                if (g.isEmpty()) intArrayOf()
                else g.split(':').map { (it.toIntOrNull() ?: 0).coerceIn(0, 65535) }.toIntArray()
            }

        // Movement-style default: missing or 0 → def.
        fun p(i: Int, def: Int): Int =
            groups.getOrNull(i)?.firstOrNull()?.takeIf { it != 0 } ?: def

        // Selector-style default: missing → 0 (0 is meaningful for ED/EL).
        fun p0(i: Int): Int = groups.getOrNull(i)?.firstOrNull() ?: 0

        when (final) {
            'A' -> moveCursor(cursorRow - p(0, 1), cursorCol)
            'B', 'e' -> moveCursor(cursorRow + p(0, 1), cursorCol)
            'C', 'a' -> moveCursor(cursorRow, cursorCol + p(0, 1))
            'D' -> moveCursor(cursorRow, cursorCol - p(0, 1))
            'E' -> moveCursor(cursorRow + p(0, 1), 0)
            'F' -> moveCursor(cursorRow - p(0, 1), 0)
            'G', '`' -> moveCursor(cursorRow, p(0, 1) - 1)
            'd' -> moveCursor(p(0, 1) - 1, cursorCol)
            'H', 'f' -> moveCursor(p(0, 1) - 1, p(1, 1) - 1)
            'J' -> eraseDisplay(p0(0))
            'K' -> eraseLine(p0(0))
            'L' -> insertLines(p(0, 1))
            'M' -> deleteLines(p(0, 1))
            'P' -> deleteChars(p(0, 1))
            '@' -> insertChars(p(0, 1))
            'X' -> eraseChars(p(0, 1))
            'S' -> scrollUpRegion(p(0, 1))
            'T' -> scrollDownRegion(p(0, 1))
            'b' -> repeat(p(0, 1)) { lastPrinted?.let { printText(it) } }
            'm' -> sgr(groups)
            'r' -> if (!private) setScrollRegion(p(0, 1), p(1, rows))
            's' -> if (!private) saveCursor()
            'u' -> if (!private) restoreCursor()
            'h' -> if (private) groups.forEach { setPrivateMode(it.firstOrNull() ?: 0, true) }
            'l' -> if (private) groups.forEach { setPrivateMode(it.firstOrNull() ?: 0, false) }
            else -> Unit // DA/DSR/window ops — a mirror never answers back
        }
    }

    // ── Printing ─────────────────────────────────────────────────────────────

    private fun print(cp: Int) {
        // Zero-width chars would corrupt the grid; drop the common ones.
        if (cp == 0x200B || cp == 0x200C || cp == 0x200D || cp == 0xFEFF) return
        printText(String(Character.toChars(cp)))
    }

    private fun printText(text: String) {
        if (pendingWrap && autowrap) {
            cursorCol = 0
            index()
            pendingWrap = false
        }
        grid[cursorRow][cursorCol] = VtCell(text, fg, bg, bold, underline, inverse)
        lastPrinted = text
        if (cursorCol == cols - 1) {
            if (autowrap) pendingWrap = true
        } else {
            cursorCol++
        }
    }

    // ── Cursor / scrolling ───────────────────────────────────────────────────

    private fun moveCursor(row: Int, col: Int) {
        cursorRow = row.coerceIn(0, rows - 1)
        cursorCol = col.coerceIn(0, cols - 1)
        pendingWrap = false
    }

    private fun index() {
        pendingWrap = false
        if (cursorRow == scrollBottom) {
            scrollUpRegion(1)
        } else if (cursorRow < rows - 1) {
            cursorRow++
        }
    }

    private fun reverseIndex() {
        pendingWrap = false
        if (cursorRow == scrollTop) {
            scrollDownRegion(1)
        } else if (cursorRow > 0) {
            cursorRow--
        }
    }

    private fun scrollUpRegion(n: Int) {
        repeat(n.coerceAtMost(rows)) {
            if (!inAlt && scrollTop == 0) {
                scrollbackBuf.addLast(grid[0].toList())
                while (scrollbackBuf.size > maxScrollback) scrollbackBuf.removeFirst()
            }
            for (r in scrollTop until scrollBottom) grid[r] = grid[r + 1]
            grid[scrollBottom] = blankRow()
        }
    }

    private fun scrollDownRegion(n: Int) {
        repeat(n.coerceAtMost(rows)) {
            for (r in scrollBottom downTo scrollTop + 1) grid[r] = grid[r - 1]
            grid[scrollTop] = blankRow()
        }
    }

    private fun setScrollRegion(top: Int, bottom: Int) {
        val t = (top - 1).coerceIn(0, rows - 1)
        val b = (bottom - 1).coerceIn(0, rows - 1)
        if (t < b) {
            scrollTop = t
            scrollBottom = b
        } else {
            scrollTop = 0
            scrollBottom = rows - 1
        }
        moveCursor(0, 0)
    }

    private fun saveCursor() {
        val saved = SavedCursor(cursorRow, cursorCol, fg, bg, bold, underline, inverse)
        if (inAlt) savedAlt = saved else savedMain = saved
    }

    private fun restoreCursor() {
        val saved = (if (inAlt) savedAlt else savedMain) ?: return
        cursorRow = saved.row.coerceIn(0, rows - 1)
        cursorCol = saved.col.coerceIn(0, cols - 1)
        fg = saved.fg
        bg = saved.bg
        bold = saved.bold
        underline = saved.underline
        inverse = saved.inverse
        pendingWrap = false
    }

    // ── Erase / insert / delete ──────────────────────────────────────────────

    private fun eraseDisplay(mode: Int) {
        when (mode) {
            0 -> {
                eraseLine(0)
                for (r in cursorRow + 1 until rows) grid[r] = blankRow()
            }
            1 -> {
                for (r in 0 until cursorRow) grid[r] = blankRow()
                eraseLine(1)
            }
            2 -> for (r in 0 until rows) grid[r] = blankRow()
            3 -> {
                for (r in 0 until rows) grid[r] = blankRow()
                scrollbackBuf.clear()
            }
        }
        pendingWrap = false
    }

    private fun eraseLine(mode: Int) {
        val row = grid[cursorRow]
        when (mode) {
            0 -> for (c in cursorCol until cols) row[c] = blankCell()
            1 -> for (c in 0..cursorCol.coerceAtMost(cols - 1)) row[c] = blankCell()
            2 -> for (c in 0 until cols) row[c] = blankCell()
        }
        pendingWrap = false
    }

    private fun eraseChars(n: Int) {
        val row = grid[cursorRow]
        for (c in cursorCol until (cursorCol + n).coerceAtMost(cols)) row[c] = blankCell()
    }

    private fun insertChars(n: Int) {
        val row = grid[cursorRow]
        val count = n.coerceAtMost(cols - cursorCol)
        for (c in cols - 1 downTo cursorCol + count) row[c] = row[c - count]
        for (c in cursorCol until cursorCol + count) row[c] = blankCell()
    }

    private fun deleteChars(n: Int) {
        val row = grid[cursorRow]
        val count = n.coerceAtMost(cols - cursorCol)
        for (c in cursorCol until cols - count) row[c] = row[c + count]
        for (c in cols - count until cols) row[c] = blankCell()
    }

    private fun insertLines(n: Int) {
        if (cursorRow < scrollTop || cursorRow > scrollBottom) return
        repeat(n.coerceAtMost(scrollBottom - cursorRow + 1)) {
            for (r in scrollBottom downTo cursorRow + 1) grid[r] = grid[r - 1]
            grid[cursorRow] = blankRow()
        }
        pendingWrap = false
    }

    private fun deleteLines(n: Int) {
        if (cursorRow < scrollTop || cursorRow > scrollBottom) return
        repeat(n.coerceAtMost(scrollBottom - cursorRow + 1)) {
            for (r in cursorRow until scrollBottom) grid[r] = grid[r + 1]
            grid[scrollBottom] = blankRow()
        }
        pendingWrap = false
    }

    // ── SGR ──────────────────────────────────────────────────────────────────

    private fun sgr(groups: List<IntArray>) {
        if (groups.isEmpty()) {
            resetAttrs()
            return
        }
        var i = 0
        while (i < groups.size) {
            val g = groups[i]
            when (val code = g.firstOrNull() ?: 0) {
                0 -> resetAttrs()
                1 -> bold = true
                4 -> underline = true
                7 -> inverse = true
                21, 22 -> bold = false
                24 -> underline = false
                27 -> inverse = false
                in 30..37 -> fg = code - 30
                38 -> {
                    val (color, consumed) = extendedColor(g, groups, i)
                    if (color != null) fg = color
                    i += consumed
                }
                39 -> fg = VT_COLOR_DEFAULT
                in 40..47 -> bg = code - 40
                48 -> {
                    val (color, consumed) = extendedColor(g, groups, i)
                    if (color != null) bg = color
                    i += consumed
                }
                49 -> bg = VT_COLOR_DEFAULT
                in 90..97 -> fg = code - 90 + 8
                in 100..107 -> bg = code - 100 + 8
                else -> Unit // faint/italic/blink/etc. ignored
            }
            i++
        }
    }

    /**
     * SGR 38/48 extended color, both forms:
     *  - colon subparams: `38:5:n`, `38:2:r:g:b`, `38:2:<cs>:r:g:b` (one group)
     *  - semicolon params: `38;5;n`, `38;2;r;g;b` (consumes following groups)
     * Returns the packed color (or null) + how many *extra* groups were used.
     */
    private fun extendedColor(g: IntArray, groups: List<IntArray>, i: Int): Pair<Int?, Int> {
        if (g.size >= 2) {
            return when (g[1]) {
                5 -> (g.getOrElse(2) { 0 }.coerceIn(0, 255)) to 0
                2 -> if (g.size >= 5) {
                    packRgb(g[g.size - 3], g[g.size - 2], g[g.size - 1]) to 0
                } else {
                    null to 0
                }
                else -> null to 0
            }
        }
        return when (groups.getOrNull(i + 1)?.firstOrNull()) {
            5 -> ((groups.getOrNull(i + 2)?.firstOrNull() ?: 0).coerceIn(0, 255)) to 2
            2 -> {
                val r = groups.getOrNull(i + 2)?.firstOrNull() ?: 0
                val gr = groups.getOrNull(i + 3)?.firstOrNull() ?: 0
                val b = groups.getOrNull(i + 4)?.firstOrNull() ?: 0
                packRgb(r, gr, b) to 4
            }
            else -> null to 0
        }
    }

    private fun packRgb(r: Int, g: Int, b: Int): Int =
        VT_TRUECOLOR or (r.coerceIn(0, 255) shl 16) or (g.coerceIn(0, 255) shl 8) or b.coerceIn(0, 255)

    private fun resetAttrs() {
        fg = VT_COLOR_DEFAULT
        bg = VT_COLOR_DEFAULT
        bold = false
        underline = false
        inverse = false
    }

    // ── Modes ────────────────────────────────────────────────────────────────

    private fun setPrivateMode(mode: Int, set: Boolean) {
        when (mode) {
            7 -> { autowrap = set; if (!set) pendingWrap = false }
            25 -> cursorVisible = set
            47, 1047 -> setAltScreen(set)
            1048 -> if (set) saveCursor() else restoreCursor()
            1049 -> if (set) {
                saveCursor()
                setAltScreen(true)
            } else {
                setAltScreen(false)
                restoreCursor()
            }
            else -> Unit // mouse / bracketed paste / focus reporting … ignored
        }
    }

    private fun setAltScreen(on: Boolean) {
        if (on == inAlt) return
        if (on) {
            alt = blankGrid(cols, rows)
            inAlt = true
        } else {
            inAlt = false
        }
        pendingWrap = false
    }

    private fun resetAll() {
        main = blankGrid(cols, rows)
        alt = blankGrid(cols, rows)
        inAlt = false
        cursorRow = 0
        cursorCol = 0
        cursorVisible = true
        pendingWrap = false
        autowrap = true
        scrollTop = 0
        scrollBottom = rows - 1
        savedMain = null
        savedAlt = null
        resetAttrs()
    }

    // ── Grid helpers ─────────────────────────────────────────────────────────

    /** Erase honors the current background (BCE) so TUI fills look right. */
    private fun blankCell(): VtCell =
        if (bg == VT_COLOR_DEFAULT) VtCell.BLANK else VtCell(bg = bg)

    private fun blankRow(): Array<VtCell> = Array(cols) { blankCell() }

    private fun regrid(old: Array<Array<VtCell>>, c: Int, r: Int): Array<Array<VtCell>> =
        Array(r) { row ->
            Array(c) { col ->
                old.getOrNull(row)?.getOrNull(col) ?: VtCell.BLANK
            }
        }

    companion object {
        private fun blankGrid(cols: Int, rows: Int): Array<Array<VtCell>> =
            Array(rows) { Array(cols) { VtCell.BLANK } }
    }
}
