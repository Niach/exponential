package com.exponential.app.ui.terminal

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// JVM tests for the self-written VT engine behind the Android steer viewer
// (masterplan §5c): cursor movement, SGR, wrapping, erase, scroll regions,
// alt screen, UTF-8 across frame boundaries, resize.
class VtEngineTest {

    private fun engine(cols: Int = 10, rows: Int = 4) = VtEngine(cols, rows)

    private val esc = "\u001b"
    private val bel = "\u0007"

    // ── Plain printing + controls ────────────────────────────────────────────

    @Test
    fun printsTextAndAdvancesCursor() {
        val t = engine()
        t.feed("hi")
        assertEquals("hi", t.rowText(0))
        assertEquals(0, t.cursorRow)
        assertEquals(2, t.cursorCol)
    }

    @Test
    fun crLfMoveCursor() {
        val t = engine()
        t.feed("ab\r\ncd")
        assertEquals("ab", t.rowText(0))
        assertEquals("cd", t.rowText(1))
        assertEquals(1, t.cursorRow)
        assertEquals(2, t.cursorCol)
    }

    @Test
    fun lineFeedKeepsColumn() {
        val t = engine()
        t.feed("abc\n")
        assertEquals(1, t.cursorRow)
        assertEquals(3, t.cursorCol)
    }

    @Test
    fun backspaceAndTab() {
        val t = engine(20, 4)
        t.feed("abx")
        assertEquals("ax", t.rowText(0))
        t.feed("\r\t")
        assertEquals(8, t.cursorCol)
    }

    // ── Wrapping ─────────────────────────────────────────────────────────────

    @Test
    fun wrapsAtLastColumn() {
        val t = engine(5, 4)
        t.feed("abcdefg")
        assertEquals("abcde", t.rowText(0))
        assertEquals("fg", t.rowText(1))
        assertEquals(1, t.cursorRow)
        assertEquals(2, t.cursorCol)
    }

    @Test
    fun deferredWrapDoesNotWrapOnCr() {
        val t = engine(5, 4)
        // Exactly fills the row: cursor stays on the row (pending wrap), and a
        // CR must land at col 0 of the SAME row, not the next one.
        t.feed("abcde\rX")
        assertEquals("Xbcde", t.rowText(0))
        assertEquals("", t.rowText(1))
    }

    @Test
    fun wrappingScrollsAtBottom() {
        val t = engine(5, 2)
        t.feed("aaaaabbbbbccccc")
        assertEquals("bbbbb", t.rowText(0))
        assertEquals("ccccc", t.rowText(1))
        assertEquals(1, t.scrollbackSize)
    }

    @Test
    fun autowrapOffOverwritesLastColumn() {
        val t = engine(5, 2)
        t.feed("$esc[?7labcdefg")
        assertEquals("abcdg", t.rowText(0))
        assertEquals(0, t.cursorRow)
    }

    // ── Cursor addressing ────────────────────────────────────────────────────

    @Test
    fun cupMovesCursorOneBased() {
        val t = engine(10, 4)
        t.feed("$esc[3;5HX")
        assertEquals(2, t.cursorRow)
        assertEquals("X", t.cellAt(2, 4).text)
    }

    @Test
    fun relativeCursorMovesClampToScreen() {
        val t = engine(10, 4)
        t.feed("$esc[2;2H")
        t.feed("$esc[9A") // way up — clamps to row 0
        assertEquals(0, t.cursorRow)
        t.feed("$esc[9B$esc[9B")
        assertEquals(3, t.cursorRow)
        t.feed("$esc[99C")
        assertEquals(9, t.cursorCol)
        t.feed("$esc[3D")
        assertEquals(6, t.cursorCol)
    }

    @Test
    fun columnAndRowSet() {
        val t = engine(10, 4)
        t.feed("$esc[4G")
        assertEquals(3, t.cursorCol)
        t.feed("$esc[2d")
        assertEquals(1, t.cursorRow)
    }

    // ── Erase ────────────────────────────────────────────────────────────────

    @Test
    fun eraseLineVariants() {
        val t = engine(5, 2)
        t.feed("abcde$esc[1;3H$esc[K") // erase right from col 3
        assertEquals("ab", t.rowText(0))
        t.feed("$esc[2Kxy") // erase all, then print at col 3-4
        assertEquals("  xy", t.rowText(0))
        val t2 = engine(5, 2)
        t2.feed("abcde$esc[1;3H$esc[1K") // erase left through col 3
        assertEquals("   de", t2.rowText(0))
    }

    @Test
    fun eraseDisplayBelowAndAll() {
        val t = engine(5, 3)
        t.feed("aaaaa\r\nbbbbb\r\nccccc")
        t.feed("$esc[2;3H$esc[J")
        assertEquals("aaaaa", t.rowText(0))
        assertEquals("bb", t.rowText(1))
        assertEquals("", t.rowText(2))
        t.feed("$esc[2J")
        assertEquals("", t.rowText(0))
    }

    @Test
    fun eraseChars() {
        val t = engine(6, 2)
        t.feed("abcdef$esc[1;2H$esc[3X")
        assertEquals("a   ef", t.rowText(0))
    }

    // ── Insert / delete ──────────────────────────────────────────────────────

    @Test
    fun insertAndDeleteChars() {
        val t = engine(6, 2)
        t.feed("abcdef$esc[1;2H$esc[2@")
        assertEquals("a  bcd", t.rowText(0))
        t.feed("$esc[2P")
        assertEquals("abcd", t.rowText(0))
    }

    @Test
    fun insertAndDeleteLines() {
        val t = engine(3, 3)
        t.feed("aaa\r\nbbb\r\nccc")
        t.feed("$esc[1;1H$esc[L")
        assertEquals("", t.rowText(0))
        assertEquals("aaa", t.rowText(1))
        assertEquals("bbb", t.rowText(2))
        t.feed("$esc[M")
        assertEquals("aaa", t.rowText(0))
        assertEquals("bbb", t.rowText(1))
        assertEquals("", t.rowText(2))
    }

    // ── SGR ──────────────────────────────────────────────────────────────────

    @Test
    fun sgrBasicColorsAndReset() {
        val t = engine()
        t.feed("$esc[31mr$esc[0mn")
        assertEquals(1, t.cellAt(0, 0).fg)
        assertEquals(VT_COLOR_DEFAULT, t.cellAt(0, 1).fg)
    }

    @Test
    fun sgrBoldUnderlineInverse() {
        val t = engine()
        t.feed("$esc[1;4;7mx")
        val cell = t.cellAt(0, 0)
        assertTrue(cell.bold)
        assertTrue(cell.underline)
        assertTrue(cell.inverse)
        t.feed("$esc[22;24;27my")
        val cell2 = t.cellAt(0, 1)
        assertFalse(cell2.bold)
        assertFalse(cell2.underline)
        assertFalse(cell2.inverse)
    }

    @Test
    fun sgrBrightAndBackground() {
        val t = engine()
        t.feed("$esc[93;44mx")
        assertEquals(11, t.cellAt(0, 0).fg) // bright yellow = 8 + 3
        assertEquals(4, t.cellAt(0, 0).bg)
    }

    @Test
    fun sgr256Semicolon() {
        val t = engine()
        t.feed("$esc[38;5;196m$esc[48;5;17mx")
        assertEquals(196, t.cellAt(0, 0).fg)
        assertEquals(17, t.cellAt(0, 0).bg)
    }

    @Test
    fun sgr256Colon() {
        val t = engine()
        t.feed("$esc[38:5:100mx")
        assertEquals(100, t.cellAt(0, 0).fg)
    }

    @Test
    fun sgrTruecolor() {
        val t = engine()
        t.feed("$esc[38;2;10;20;30mx")
        assertEquals(VT_TRUECOLOR or (10 shl 16) or (20 shl 8) or 30, t.cellAt(0, 0).fg)
        t.feed("$esc[38:2:1:2:3my")
        assertEquals(VT_TRUECOLOR or (1 shl 16) or (2 shl 8) or 3, t.cellAt(0, 1).fg)
    }

    @Test
    fun sgrExtendedColorDoesNotEatFollowingParams() {
        val t = engine()
        t.feed("$esc[38;5;196;1mx") // 256-color then bold in one sequence
        assertEquals(196, t.cellAt(0, 0).fg)
        assertTrue(t.cellAt(0, 0).bold)
    }

    @Test
    fun eraseUsesCurrentBackground() {
        val t = engine(5, 2)
        t.feed("$esc[44m$esc[2J")
        assertEquals(4, t.cellAt(1, 3).bg)
    }

    // ── Scroll region ────────────────────────────────────────────────────────

    @Test
    fun scrollRegionScrollsOnlyRegion() {
        val t = engine(3, 4)
        t.feed("aaa\r\nbbb\r\nccc\r\nddd")
        t.feed("$esc[2;3r") // region rows 2..3
        t.feed("$esc[3;1H\n") // LF at region bottom scrolls region only
        assertEquals("aaa", t.rowText(0))
        assertEquals("ccc", t.rowText(1))
        assertEquals("", t.rowText(2))
        assertEquals("ddd", t.rowText(3))
        assertEquals(0, t.scrollbackSize) // region scroll never hits scrollback
    }

    @Test
    fun reverseIndexScrollsDownAtTop() {
        val t = engine(3, 3)
        t.feed("aaa\r\nbbb")
        t.feed("$esc[1;1H${esc}M")
        assertEquals("", t.rowText(0))
        assertEquals("aaa", t.rowText(1))
        assertEquals("bbb", t.rowText(2))
    }

    // ── Scrollback ───────────────────────────────────────────────────────────

    @Test
    fun scrolledOffLinesLandInScrollback() {
        val t = engine(5, 2)
        t.feed("one\r\ntwo\r\nthree")
        assertEquals(1, t.scrollbackSize)
        val snap = t.snapshot()
        assertEquals("one", snap.scrollback[0].joinToString("") { it.text }.trimEnd())
        assertEquals("two", t.rowText(0))
        assertEquals("three", t.rowText(1))
    }

    // ── Alt screen ───────────────────────────────────────────────────────────

    @Test
    fun altScreenSwitchAndRestore() {
        val t = engine(6, 3)
        t.feed("main$esc[?1049h")
        assertTrue(t.isAltScreen)
        assertEquals("", t.rowText(0)) // alt starts blank
        // 1049 preserves the cursor (apps home it themselves, like real TUIs).
        t.feed("$esc[Halt")
        assertEquals("alt", t.rowText(0))
        t.feed("$esc[?1049l")
        assertFalse(t.isAltScreen)
        assertEquals("main", t.rowText(0)) // main content untouched
        assertEquals(4, t.cursorCol) // cursor restored to after "main"
        assertEquals(0, t.cursorRow)
    }

    @Test
    fun altScreenHidesScrollbackInSnapshot() {
        val t = engine(5, 2)
        t.feed("a\r\nb\r\nc") // pushes one line to scrollback
        t.feed("$esc[?1049h")
        assertEquals(0, t.snapshot().scrollback.size)
        t.feed("$esc[?1049l")
        assertEquals(1, t.snapshot().scrollback.size)
    }

    // ── Modes / misc ─────────────────────────────────────────────────────────

    @Test
    fun cursorVisibilityMode() {
        val t = engine()
        assertTrue(t.cursorVisible)
        t.feed("$esc[?25l")
        assertFalse(t.cursorVisible)
        t.feed("$esc[?25h")
        assertTrue(t.cursorVisible)
    }

    @Test
    fun oscIsSwallowed() {
        val t = engine(20, 2)
        t.feed("$esc]0;window title${bel}after")
        assertEquals("after", t.rowText(0))
        val t2 = engine(20, 2)
        t2.feed("$esc]8;;http://x$esc\\link")
        assertEquals("link", t2.rowText(0))
    }

    @Test
    fun repRepeatsLastPrintedChar() {
        val t = engine(10, 2)
        t.feed("a$esc[3b")
        assertEquals("aaaa", t.rowText(0))
    }

    @Test
    fun saveRestoreCursorEsc7and8() {
        val t = engine(10, 3)
        t.feed("ab${esc}7\r\nxyz${esc}8Z")
        assertEquals("abZ", t.rowText(0))
    }

    // ── UTF-8 ────────────────────────────────────────────────────────────────

    @Test
    fun utf8MultiByte() {
        val t = engine()
        t.feed("héllo ✻")
        assertEquals("héllo ✻", t.rowText(0))
    }

    @Test
    fun utf8SplitAcrossFeeds() {
        val t = engine()
        val bytes = "é✻".toByteArray(Charsets.UTF_8)
        // Feed one byte at a time — mid-sequence splits must not corrupt.
        for (b in bytes) t.feed(byteArrayOf(b))
        assertEquals("é✻", t.rowText(0))
    }

    @Test
    fun invalidUtf8BecomesReplacement() {
        val t = engine()
        t.feed(byteArrayOf(0xFF.toByte(), 'a'.code.toByte()))
        assertEquals("�a", t.rowText(0))
    }

    @Test
    fun supplementaryPlaneCodepoint() {
        val t = engine()
        t.feed("😀") // 4-byte UTF-8 → surrogate pair in one cell
        assertEquals("😀", t.cellAt(0, 0).text)
        assertEquals(1, t.cursorCol)
    }

    // ── Resize ───────────────────────────────────────────────────────────────

    @Test
    fun resizePreservesTopLeftContent() {
        val t = engine(6, 3)
        t.feed("abcdef\r\nghijkl")
        t.resize(4, 2)
        assertEquals(4, t.cols)
        assertEquals(2, t.rows)
        assertEquals("abcd", t.rowText(0))
        assertEquals("ghij", t.rowText(1))
        t.resize(8, 3)
        assertEquals("abcd", t.rowText(0))
        t.feed("$esc[1;5HX")
        assertEquals("abcdX", t.rowText(0))
    }

    @Test
    fun resizeClampsCursor() {
        val t = engine(10, 5)
        t.feed("$esc[5;10H")
        t.resize(4, 2)
        assertEquals(1, t.cursorRow)
        assertEquals(3, t.cursorCol)
    }

    // ── Full reset ───────────────────────────────────────────────────────────

    @Test
    fun risResetsEverything() {
        val t = engine(5, 3)
        t.feed("$esc[31mabc$esc[2;3r${esc}c")
        assertEquals("", t.rowText(0))
        assertEquals(0, t.cursorRow)
        assertEquals(0, t.cursorCol)
        t.feed("x")
        assertEquals(VT_COLOR_DEFAULT, t.cellAt(0, 0).fg)
    }
}
