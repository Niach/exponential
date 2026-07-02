import XCTest
@testable import ExpCore

// Offline gate for the steer viewer's VT10x-subset engine (masterplan §5c):
// cursor addressing, SGR colors, autowrap, erase ops, scroll region, alt
// screen, and split-UTF-8 feeds — all pure VTScreen value semantics, no I/O.
final class VTScreenTests: XCTestCase {
    private func feed(_ screen: inout VTScreen, _ text: String) {
        screen.feed([UInt8](text.utf8))
    }

    // MARK: - Plain printing + control chars

    func testPrintsPlainTextAtCursor() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "hi there")
        XCTAssertEqual(s.trimmedRowText(0), "hi there")
        XCTAssertEqual(s.cursorRow, 0)
        XCTAssertEqual(s.cursorCol, 8)
    }

    func testCarriageReturnAndLineFeed() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "one\r\ntwo")
        XCTAssertEqual(s.trimmedRowText(0), "one")
        XCTAssertEqual(s.trimmedRowText(1), "two")
        XCTAssertEqual(s.cursorRow, 1)
        XCTAssertEqual(s.cursorCol, 3)
    }

    func testBackspaceAndOverwrite() {
        var s = VTScreen(cols: 10, rows: 2)
        feed(&s, "ab\u{08}c")
        XCTAssertEqual(s.trimmedRowText(0), "ac")
    }

    func testTabAdvancesToNextEightColumnStop() {
        var s = VTScreen(cols: 20, rows: 2)
        feed(&s, "a\tb")
        XCTAssertEqual(s.cells(atRow: 0)[8].ch, "b")
    }

    // MARK: - Cursor addressing

    func testCupMovesCursorOneBased() {
        var s = VTScreen(cols: 20, rows: 5)
        feed(&s, "\u{1B}[3;7HX")
        XCTAssertEqual(s.cells(atRow: 2)[6].ch, "X")
    }

    func testRelativeCursorMoves() {
        var s = VTScreen(cols: 20, rows: 5)
        feed(&s, "\u{1B}[3;3H") // row 2, col 2 (0-based)
        feed(&s, "\u{1B}[A")    // up 1
        feed(&s, "\u{1B}[2C")   // right 2
        feed(&s, "X")
        XCTAssertEqual(s.cells(atRow: 1)[4].ch, "X")
    }

    func testCursorMovesClampToBounds() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "\u{1B}[99;99H")
        XCTAssertEqual(s.cursorRow, 2)
        XCTAssertEqual(s.cursorCol, 9)
        feed(&s, "\u{1B}[99A\u{1B}[99D")
        XCTAssertEqual(s.cursorRow, 0)
        XCTAssertEqual(s.cursorCol, 0)
    }

    func testColumnAndRowAbsoluteMoves() {
        var s = VTScreen(cols: 20, rows: 5)
        feed(&s, "\u{1B}[4dY")  // VPA row 4 (1-based)
        XCTAssertEqual(s.cells(atRow: 3)[0].ch, "Y")
        feed(&s, "\u{1B}[10GZ") // CHA col 10 (1-based)
        XCTAssertEqual(s.cells(atRow: 3)[9].ch, "Z")
    }

    // MARK: - Autowrap (deferred wrap)

    func testAutowrapDefersUntilNextPrintable() {
        var s = VTScreen(cols: 5, rows: 3)
        feed(&s, "abcde")
        // Deferred wrap: cursor logically parked on the last column.
        XCTAssertEqual(s.cursorRow, 0)
        feed(&s, "f")
        XCTAssertEqual(s.trimmedRowText(0), "abcde")
        XCTAssertEqual(s.trimmedRowText(1), "f")
        XCTAssertEqual(s.cursorRow, 1)
        XCTAssertEqual(s.cursorCol, 1)
    }

    func testCarriageReturnCancelsPendingWrap() {
        var s = VTScreen(cols: 5, rows: 3)
        feed(&s, "abcde\rX")
        XCTAssertEqual(s.trimmedRowText(0), "Xbcde")
        XCTAssertEqual(s.cursorRow, 0)
    }

    func testWrapAtBottomScrollsIntoScrollback() {
        var s = VTScreen(cols: 3, rows: 2)
        feed(&s, "aaabbbccc")
        XCTAssertEqual(s.trimmedRowText(0), "bbb")
        XCTAssertEqual(s.trimmedRowText(1), "ccc")
        XCTAssertEqual(s.scrollbackLineCount, 1)
        XCTAssertEqual(String(s.scrollbackLine(0).map(\.ch)), "aaa")
    }

    // MARK: - SGR

    func testSgrBasicAndBrightColors() {
        var s = VTScreen(cols: 20, rows: 2)
        feed(&s, "\u{1B}[31mr\u{1B}[92mg\u{1B}[39md")
        XCTAssertEqual(s.cells(atRow: 0)[0].fg, .palette(1))
        XCTAssertEqual(s.cells(atRow: 0)[1].fg, .palette(10))
        XCTAssertEqual(s.cells(atRow: 0)[2].fg, .standard)
    }

    func testSgr256AndTruecolor() {
        var s = VTScreen(cols: 20, rows: 2)
        feed(&s, "\u{1B}[38;5;196ma\u{1B}[48;2;10;20;30mb")
        XCTAssertEqual(s.cells(atRow: 0)[0].fg, .palette(196))
        XCTAssertEqual(s.cells(atRow: 0)[1].bg, .rgb(10, 20, 30))
        // Colon-separated sub-parameter form.
        feed(&s, "\u{1B}[0m\u{1B}[38:5:21mc")
        XCTAssertEqual(s.cells(atRow: 0)[2].fg, .palette(21))
    }

    func testSgrAttributesAndReset() {
        var s = VTScreen(cols: 20, rows: 2)
        feed(&s, "\u{1B}[1;4;7ma\u{1B}[0mb")
        let a = s.cells(atRow: 0)[0]
        XCTAssertTrue(a.bold)
        XCTAssertTrue(a.underline)
        XCTAssertTrue(a.inverse)
        let b = s.cells(atRow: 0)[1]
        XCTAssertFalse(b.bold)
        XCTAssertFalse(b.underline)
        XCTAssertFalse(b.inverse)
    }

    // MARK: - Erase ops

    func testEraseLineModes() {
        var s = VTScreen(cols: 5, rows: 2)
        feed(&s, "abcde\u{1B}[1;3H\u{1B}[K") // clear cursor→eol from col 3
        XCTAssertEqual(s.trimmedRowText(0), "ab")
        feed(&s, "\u{1B}[2;1Hvwxyz\u{1B}[2;3H\u{1B}[1K") // clear bol→cursor
        XCTAssertEqual(s.rowText(1), "   yz")
    }

    func testEraseDisplayBelowAndAll() {
        var s = VTScreen(cols: 4, rows: 3)
        feed(&s, "1111\r\n2222\r\n3333")
        feed(&s, "\u{1B}[2;1H\u{1B}[J") // cursor row 2 → erase below
        XCTAssertEqual(s.trimmedRowText(0), "1111")
        XCTAssertEqual(s.trimmedRowText(1), "")
        XCTAssertEqual(s.trimmedRowText(2), "")
        feed(&s, "\u{1B}[1;1Hxxxx\u{1B}[2J")
        XCTAssertEqual(s.trimmedRowText(0), "")
    }

    func testDeleteAndInsertChars() {
        var s = VTScreen(cols: 6, rows: 2)
        feed(&s, "abcdef\u{1B}[1;2H\u{1B}[2P") // delete 2 chars at col 2
        XCTAssertEqual(s.trimmedRowText(0), "adef")
        feed(&s, "\u{1B}[1;2H\u{1B}[1@") // insert 1 blank at col 2
        XCTAssertEqual(s.trimmedRowText(0), "a def")
    }

    // MARK: - Scroll region

    func testScrollRegionScrollsOnlyInsideMargins() {
        var s = VTScreen(cols: 4, rows: 4)
        feed(&s, "AAAA\r\nBBBB\r\nCCCC\r\nDDDD")
        feed(&s, "\u{1B}[2;3r") // margins rows 2–3
        feed(&s, "\u{1B}[3;1H\n") // LF at region bottom → scroll region only
        XCTAssertEqual(s.trimmedRowText(0), "AAAA")
        XCTAssertEqual(s.trimmedRowText(1), "CCCC")
        XCTAssertEqual(s.trimmedRowText(2), "")
        XCTAssertEqual(s.trimmedRowText(3), "DDDD")
        // Region scrolls never leak into the primary scrollback.
        XCTAssertEqual(s.scrollbackLineCount, 0)
    }

    func testReverseLineFeedAtRegionTopScrollsDown() {
        var s = VTScreen(cols: 4, rows: 3)
        feed(&s, "AAAA\r\nBBBB\r\nCCCC")
        feed(&s, "\u{1B}[1;1H\u{1B}M") // RI at top → scroll down
        XCTAssertEqual(s.trimmedRowText(0), "")
        XCTAssertEqual(s.trimmedRowText(1), "AAAA")
        XCTAssertEqual(s.trimmedRowText(2), "BBBB")
    }

    // MARK: - Alt screen

    func testAltScreenSwitchAndRestore() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "primary")
        feed(&s, "\u{1B}[?1049h") // enter alt: cleared grid, cursor home
        XCTAssertTrue(s.usingAltScreen)
        XCTAssertEqual(s.trimmedRowText(0), "")
        feed(&s, "alt!")
        XCTAssertEqual(s.trimmedRowText(0), "alt!")
        feed(&s, "\u{1B}[?1049l") // leave alt: primary content + cursor return
        XCTAssertFalse(s.usingAltScreen)
        XCTAssertEqual(s.trimmedRowText(0), "primary")
        XCTAssertEqual(s.cursorCol, 7)
    }

    func testAltScreenNeverFeedsScrollback() {
        var s = VTScreen(cols: 3, rows: 2)
        feed(&s, "\u{1B}[?1049h")
        feed(&s, "aaabbbccc") // wraps + scrolls inside the alt screen
        XCTAssertEqual(s.scrollbackLineCount, 0)
    }

    // MARK: - Cursor save/restore + visibility

    func testDecCursorSaveRestore() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "\u{1B}[2;5H\u{1B}7\u{1B}[1;1Hmoved\u{1B}8X")
        XCTAssertEqual(s.cells(atRow: 1)[4].ch, "X")
    }

    func testCursorVisibilityMode() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "\u{1B}[?25l")
        XCTAssertFalse(s.cursorVisible)
        feed(&s, "\u{1B}[?25h")
        XCTAssertTrue(s.cursorVisible)
    }

    // MARK: - OSC + unknown sequences are consumed silently

    func testOscTitleSequenceIsSwallowed() {
        var s = VTScreen(cols: 20, rows: 2)
        feed(&s, "\u{1B}]0;window title\u{07}ok")
        XCTAssertEqual(s.trimmedRowText(0), "ok")
        feed(&s, "\u{1B}]8;;https://x\u{1B}\\!") // ST-terminated OSC
        XCTAssertEqual(s.trimmedRowText(0), "ok!")
    }

    // MARK: - Streaming UTF-8

    func testUtf8ScalarSplitAcrossFeeds() {
        var s = VTScreen(cols: 10, rows: 2)
        let bytes = [UInt8]("é".utf8) // 2 bytes
        s.feed([bytes[0]])
        s.feed([bytes[1]])
        XCTAssertEqual(s.trimmedRowText(0), "é")
    }

    func testResizePreservesTopLeftContent() {
        var s = VTScreen(cols: 10, rows: 3)
        feed(&s, "hello")
        s.resize(cols: 4, rows: 2)
        XCTAssertEqual(s.trimmedRowText(0), "hell")
        XCTAssertEqual(s.cols, 4)
        XCTAssertEqual(s.rows, 2)
    }
}
