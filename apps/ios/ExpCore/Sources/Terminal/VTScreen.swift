import Foundation

// A minimal, self-contained VT10x-subset terminal screen model for the mobile
// steer VIEWER (masterplan §5c). It mirrors a remote desktop PTY: verbatim
// bytes come off the relay's binary frames and are folded into a cols×rows
// cell grid the SwiftUI layer renders as monospaced attributed lines.
//
// Deliberately small — the REAL terminal runs on the desktop; a
// mirror only needs the escapes interactive TUIs (claude code, shells) emit:
//   grid + cursor addressing (CUP/CUU/…/VPA/CHA), SGR colors (16/256/RGB +
//   bold/italic/underline/inverse/dim), erase ops (ED/EL/ECH/DCH/ICH/IL/DL),
//   scroll region (DECSTBM, SU/SD), alt screen (1049/47/1047), autowrap with
//   deferred wrap, and a primary-screen scrollback.
// Unhandled sequences are consumed and ignored, never rendered as garbage.
// No GPL lineage: written from the public ECMA-48/xterm control descriptions.
//
// Plain value type, Foundation-only ⇒ fully offline-testable in ExpCoreTests.

// MARK: - Cells + colors

public enum VTColor: Equatable, Sendable {
    /// The renderer's default foreground/background (slot-dependent).
    case standard
    /// xterm palette index 0–255 (0–15 = classic ANSI).
    case palette(UInt8)
    case rgb(UInt8, UInt8, UInt8)
}

public struct VTCell: Equatable, Sendable {
    public var ch: Character
    public var fg: VTColor
    public var bg: VTColor
    public var bold: Bool
    public var dim: Bool
    public var italic: Bool
    public var underline: Bool
    public var inverse: Bool

    public static let blank = VTCell(
        ch: " ", fg: .standard, bg: .standard,
        bold: false, dim: false, italic: false, underline: false, inverse: false
    )
}

// MARK: - Screen

public struct VTScreen: Sendable {
    public private(set) var cols: Int
    public private(set) var rows: Int
    /// 0-based cursor position.
    public private(set) var cursorRow = 0
    public private(set) var cursorCol = 0
    public private(set) var cursorVisible = true
    public private(set) var usingAltScreen = false
    /// Bumped on every visible mutation so views can invalidate cheaply.
    public private(set) var generation = 0

    private var grid: [[VTCell]]
    /// The primary grid parked while the alt screen is active.
    private var savedPrimaryGrid: [[VTCell]]?
    private var savedPrimaryCursor: (row: Int, col: Int) = (0, 0)

    /// Lines scrolled off the top of the PRIMARY screen (alt screen never
    /// scrolls back), oldest first, capped.
    private var scrollback: [[VTCell]] = []
    private let scrollbackCap: Int

    /// DECSTBM margins, 0-based inclusive.
    private var scrollTop = 0
    private var scrollBottom: Int

    /// Current SGR attributes applied to printed cells.
    private var pen = VTCell.blank
    /// Deferred autowrap: set after printing in the last column.
    private var pendingWrap = false
    private var savedCursor: (row: Int, col: Int, pen: VTCell)?

    // Parser state.
    private enum ParseState {
        case ground
        case esc
        case escCharset // ESC ( / ) / * / + — one designator byte follows
        case csi
        case oscString // OSC / DCS / APC / PM — consumed until BEL or ST
    }

    private var state: ParseState = .ground
    private var csiBuffer: [UInt8] = []
    private var oscPrevWasEsc = false

    // Streaming UTF-8 decode (relay frames can split a scalar across chunks).
    private var utf8Pending: [UInt8] = []
    private var utf8Expected = 0

    public init(cols: Int = 80, rows: Int = 24, scrollbackCap: Int = 2000) {
        self.cols = max(1, cols)
        self.rows = max(1, rows)
        self.scrollbackCap = max(0, scrollbackCap)
        self.scrollBottom = self.rows - 1
        self.grid = Self.blankGrid(cols: self.cols, rows: self.rows)
    }

    // MARK: Public surface

    public mutating func feed(_ data: Data) {
        feed([UInt8](data))
    }

    public mutating func feed(_ bytes: [UInt8]) {
        for byte in bytes { consume(byte) }
        generation &+= 1
    }

    /// The publisher resized (relay `resize` frame). Content is preserved
    /// top-left anchored; interactive TUIs repaint right after anyway.
    public mutating func resize(cols newCols: Int, rows newRows: Int) {
        let c = max(1, newCols)
        let r = max(1, newRows)
        guard c != cols || r != rows else { return }
        grid = Self.regrid(grid, cols: c, rows: r)
        if var saved = savedPrimaryGrid {
            saved = Self.regrid(saved, cols: c, rows: r)
            savedPrimaryGrid = saved
        }
        cols = c
        rows = r
        scrollTop = 0
        scrollBottom = r - 1
        cursorRow = min(cursorRow, r - 1)
        cursorCol = min(cursorCol, c - 1)
        pendingWrap = false
        generation &+= 1
    }

    public func cells(atRow row: Int) -> [VTCell] {
        guard row >= 0, row < rows else { return [] }
        return grid[row]
    }

    public var scrollbackLineCount: Int { scrollback.count }

    public func scrollbackLine(_ index: Int) -> [VTCell] {
        guard index >= 0, index < scrollback.count else { return [] }
        return scrollback[index]
    }

    // MARK: Byte pump

    private mutating func consume(_ byte: UInt8) {
        switch state {
        case .ground:
            consumeGround(byte)
        case .esc:
            consumeEsc(byte)
        case .escCharset:
            state = .ground // swallow the charset designator
        case .csi:
            consumeCsi(byte)
        case .oscString:
            consumeOsc(byte)
        }
    }

    private mutating func consumeGround(_ byte: UInt8) {
        if byte >= 0x20 && byte != 0x7F {
            consumeUtf8(byte)
            return
        }
        utf8Reset()
        executeControl(byte)
    }

    /// C0 controls execute in ground AND inside CSI (per ECMA-48).
    private mutating func executeControl(_ byte: UInt8) {
        switch byte {
        case 0x1B: state = .esc
        case 0x0D: cursorCol = 0; pendingWrap = false
        case 0x0A, 0x0B, 0x0C: lineFeed()
        case 0x08:
            if cursorCol > 0 { cursorCol -= 1 }
            pendingWrap = false
        case 0x09:
            cursorCol = min(cols - 1, ((cursorCol / 8) + 1) * 8)
            pendingWrap = false
        default:
            break // BEL, NUL, SO/SI, …
        }
    }

    private mutating func consumeEsc(_ byte: UInt8) {
        state = .ground
        switch byte {
        case UInt8(ascii: "["):
            csiBuffer.removeAll(keepingCapacity: true)
            state = .csi
        case UInt8(ascii: "]"), UInt8(ascii: "P"), UInt8(ascii: "X"),
             UInt8(ascii: "^"), UInt8(ascii: "_"):
            oscPrevWasEsc = false
            state = .oscString
        case UInt8(ascii: "("), UInt8(ascii: ")"), UInt8(ascii: "*"), UInt8(ascii: "+"):
            state = .escCharset
        case UInt8(ascii: "7"):
            savedCursor = (cursorRow, cursorCol, pen)
        case UInt8(ascii: "8"):
            if let saved = savedCursor {
                cursorRow = min(saved.row, rows - 1)
                cursorCol = min(saved.col, cols - 1)
                pen = saved.pen
                pendingWrap = false
            }
        case UInt8(ascii: "D"): // IND
            lineFeed()
        case UInt8(ascii: "E"): // NEL
            cursorCol = 0
            lineFeed()
        case UInt8(ascii: "M"): // RI
            reverseLineFeed()
        case UInt8(ascii: "c"): // RIS
            fullReset()
        default:
            break // '=', '>', unknown — consumed
        }
    }

    private mutating func consumeCsi(_ byte: UInt8) {
        if byte < 0x20 {
            executeControl(byte) // ESC restarts, CR/LF execute mid-sequence
            return
        }
        if byte >= 0x40 && byte <= 0x7E {
            state = .ground
            dispatchCsi(final: byte)
            return
        }
        // Parameter (0x30–0x3F) and intermediate (0x20–0x2F) bytes.
        if csiBuffer.count < 64 { csiBuffer.append(byte) }
    }

    private mutating func consumeOsc(_ byte: UInt8) {
        if byte == 0x07 { // BEL terminator
            state = .ground
            return
        }
        if oscPrevWasEsc && byte == UInt8(ascii: "\\") { // ST terminator
            state = .ground
            oscPrevWasEsc = false
            return
        }
        oscPrevWasEsc = byte == 0x1B
    }

    // MARK: UTF-8 streaming decode

    private mutating func consumeUtf8(_ byte: UInt8) {
        if utf8Expected == 0 {
            if byte < 0x80 {
                printScalar(Unicode.Scalar(byte))
            } else if byte >= 0xC2 && byte <= 0xDF {
                utf8Pending = [byte]; utf8Expected = 1
            } else if byte >= 0xE0 && byte <= 0xEF {
                utf8Pending = [byte]; utf8Expected = 2
            } else if byte >= 0xF0 && byte <= 0xF4 {
                utf8Pending = [byte]; utf8Expected = 3
            }
            // else: stray continuation / invalid lead — dropped
            return
        }
        guard byte & 0xC0 == 0x80 else {
            // Broken sequence — drop it and reprocess this byte fresh.
            utf8Reset()
            consumeUtf8(byte)
            return
        }
        utf8Pending.append(byte)
        utf8Expected -= 1
        if utf8Expected == 0 {
            if let s = String(bytes: utf8Pending, encoding: .utf8),
               let scalar = s.unicodeScalars.first {
                printScalar(scalar)
            }
            utf8Pending.removeAll(keepingCapacity: true)
        }
    }

    private mutating func utf8Reset() {
        utf8Pending.removeAll(keepingCapacity: true)
        utf8Expected = 0
    }

    // MARK: Printing + cursor motion

    private mutating func printScalar(_ scalar: Unicode.Scalar) {
        if pendingWrap {
            pendingWrap = false
            cursorCol = 0
            lineFeed()
        }
        var cell = pen
        cell.ch = Character(scalar)
        grid[cursorRow][cursorCol] = cell
        if cursorCol == cols - 1 {
            pendingWrap = true
        } else {
            cursorCol += 1
        }
    }

    private mutating func lineFeed() {
        pendingWrap = false
        if cursorRow == scrollBottom {
            scrollUp(1)
        } else if cursorRow < rows - 1 {
            cursorRow += 1
        }
    }

    private mutating func reverseLineFeed() {
        pendingWrap = false
        if cursorRow == scrollTop {
            scrollDown(1)
        } else if cursorRow > 0 {
            cursorRow -= 1
        }
    }

    private mutating func scrollUp(_ n: Int) {
        let count = min(max(1, n), scrollBottom - scrollTop + 1)
        for _ in 0..<count {
            let evicted = grid.remove(at: scrollTop)
            if !usingAltScreen && scrollTop == 0 && scrollbackCap > 0 {
                scrollback.append(evicted)
                if scrollback.count > scrollbackCap {
                    scrollback.removeFirst(scrollback.count - scrollbackCap)
                }
            }
            grid.insert(blankLine(), at: scrollBottom)
        }
    }

    private mutating func scrollDown(_ n: Int) {
        let count = min(max(1, n), scrollBottom - scrollTop + 1)
        for _ in 0..<count {
            grid.remove(at: scrollBottom)
            grid.insert(blankLine(), at: scrollTop)
        }
    }

    // MARK: CSI dispatch

    private mutating func dispatchCsi(final: UInt8) {
        var buf = csiBuffer[...]
        let isPrivate = buf.first == UInt8(ascii: "?")
        if isPrivate { buf = buf.dropFirst() }
        // Sequences with intermediate bytes (0x20–0x2F, e.g. `CSI ... $ p`) or
        // other prefix markers (`>`, `=`, `!`) are protocol we don't model.
        if buf.contains(where: { ($0 >= 0x20 && $0 <= 0x2F) || $0 == UInt8(ascii: ">") || $0 == UInt8(ascii: "=") || $0 == UInt8(ascii: "!") }) {
            return
        }
        // Split on ';' and ':' (SGR sub-parameter form 38:5:n).
        var params: [Int] = []
        var current: Int? = nil
        for b in buf {
            if b == UInt8(ascii: ";") || b == UInt8(ascii: ":") {
                params.append(current ?? 0)
                current = nil
            } else if b >= UInt8(ascii: "0") && b <= UInt8(ascii: "9") {
                current = (current ?? 0) * 10 + Int(b - UInt8(ascii: "0"))
                if let c = current, c > 99999 { current = 99999 }
            }
        }
        if current != nil || !params.isEmpty || buf.isEmpty {
            params.append(current ?? 0)
        }

        func p(_ i: Int, _ fallback: Int) -> Int {
            guard i < params.count, params[i] != 0 else { return fallback }
            return params[i]
        }

        switch final {
        case UInt8(ascii: "A"): moveCursor(rowDelta: -p(0, 1), colDelta: 0)
        case UInt8(ascii: "B"): moveCursor(rowDelta: p(0, 1), colDelta: 0)
        case UInt8(ascii: "C"): moveCursor(rowDelta: 0, colDelta: p(0, 1))
        case UInt8(ascii: "D"): moveCursor(rowDelta: 0, colDelta: -p(0, 1))
        case UInt8(ascii: "E"):
            moveCursor(rowDelta: p(0, 1), colDelta: 0)
            cursorCol = 0
        case UInt8(ascii: "F"):
            moveCursor(rowDelta: -p(0, 1), colDelta: 0)
            cursorCol = 0
        case UInt8(ascii: "G"):
            cursorCol = clampCol(p(0, 1) - 1)
            pendingWrap = false
        case UInt8(ascii: "H"), UInt8(ascii: "f"):
            cursorRow = clampRow(p(0, 1) - 1)
            cursorCol = clampCol(p(1, 1) - 1)
            pendingWrap = false
        case UInt8(ascii: "d"):
            cursorRow = clampRow(p(0, 1) - 1)
            pendingWrap = false
        case UInt8(ascii: "J"): eraseDisplay(mode: params.first ?? 0)
        case UInt8(ascii: "K"): eraseLine(mode: params.first ?? 0)
        case UInt8(ascii: "L"): insertLines(p(0, 1))
        case UInt8(ascii: "M"): deleteLines(p(0, 1))
        case UInt8(ascii: "P"): deleteChars(p(0, 1))
        case UInt8(ascii: "@"): insertChars(p(0, 1))
        case UInt8(ascii: "X"): eraseChars(p(0, 1))
        case UInt8(ascii: "S"): scrollUp(p(0, 1))
        case UInt8(ascii: "T"): scrollDown(p(0, 1))
        case UInt8(ascii: "r"):
            let top = clampRow(p(0, 1) - 1)
            let bottom = clampRow(p(1, rows) - 1)
            if top < bottom {
                scrollTop = top
                scrollBottom = bottom
                cursorRow = 0
                cursorCol = 0
                pendingWrap = false
            }
        case UInt8(ascii: "m"): applySgr(params)
        case UInt8(ascii: "h"): setMode(params, isPrivate: isPrivate, enable: true)
        case UInt8(ascii: "l"): setMode(params, isPrivate: isPrivate, enable: false)
        case UInt8(ascii: "s"): savedCursor = (cursorRow, cursorCol, pen)
        case UInt8(ascii: "u"):
            if let saved = savedCursor {
                cursorRow = min(saved.row, rows - 1)
                cursorCol = min(saved.col, cols - 1)
                pen = saved.pen
                pendingWrap = false
            }
        default:
            break // DA/DSR/other queries — a passive mirror never answers
        }
    }

    private mutating func moveCursor(rowDelta: Int, colDelta: Int) {
        cursorRow = clampRow(cursorRow + rowDelta)
        cursorCol = clampCol(cursorCol + colDelta)
        pendingWrap = false
    }

    private func clampRow(_ r: Int) -> Int { min(max(0, r), rows - 1) }
    private func clampCol(_ c: Int) -> Int { min(max(0, c), cols - 1) }

    // MARK: Erase / insert / delete (BCE — erased cells keep the pen bg)

    private var eraseCell: VTCell {
        var cell = VTCell.blank
        cell.bg = pen.bg
        return cell
    }

    private mutating func eraseDisplay(mode: Int) {
        switch mode {
        case 0:
            eraseLine(mode: 0)
            for r in (cursorRow + 1)..<rows { grid[r] = blankLine() }
        case 1:
            eraseLine(mode: 1)
            for r in 0..<cursorRow { grid[r] = blankLine() }
        case 2:
            grid = Self.blankGrid(cols: cols, rows: rows, fill: eraseCell)
        case 3:
            grid = Self.blankGrid(cols: cols, rows: rows, fill: eraseCell)
            scrollback.removeAll()
        default:
            break
        }
    }

    private mutating func eraseLine(mode: Int) {
        let fill = eraseCell
        switch mode {
        case 0:
            for c in cursorCol..<cols { grid[cursorRow][c] = fill }
        case 1:
            for c in 0...cursorCol { grid[cursorRow][c] = fill }
        case 2:
            grid[cursorRow] = blankLine()
        default:
            break
        }
    }

    private mutating func insertLines(_ n: Int) {
        guard cursorRow >= scrollTop && cursorRow <= scrollBottom else { return }
        let count = min(n, scrollBottom - cursorRow + 1)
        for _ in 0..<count {
            grid.remove(at: scrollBottom)
            grid.insert(blankLine(), at: cursorRow)
        }
        pendingWrap = false
    }

    private mutating func deleteLines(_ n: Int) {
        guard cursorRow >= scrollTop && cursorRow <= scrollBottom else { return }
        let count = min(n, scrollBottom - cursorRow + 1)
        for _ in 0..<count {
            grid.remove(at: cursorRow)
            grid.insert(blankLine(), at: scrollBottom)
        }
        pendingWrap = false
    }

    private mutating func deleteChars(_ n: Int) {
        let count = min(n, cols - cursorCol)
        guard count > 0 else { return }
        grid[cursorRow].removeSubrange(cursorCol..<(cursorCol + count))
        grid[cursorRow].append(contentsOf: Array(repeating: eraseCell, count: count))
    }

    private mutating func insertChars(_ n: Int) {
        let count = min(n, cols - cursorCol)
        guard count > 0 else { return }
        grid[cursorRow].insert(contentsOf: Array(repeating: eraseCell, count: count), at: cursorCol)
        grid[cursorRow].removeLast(count)
    }

    private mutating func eraseChars(_ n: Int) {
        let count = min(n, cols - cursorCol)
        let fill = eraseCell
        for c in cursorCol..<(cursorCol + count) { grid[cursorRow][c] = fill }
    }

    // MARK: SGR

    private mutating func applySgr(_ params: [Int]) {
        var i = 0
        let list = params.isEmpty ? [0] : params
        while i < list.count {
            let code = list[i]
            switch code {
            case 0:
                let ch = pen.ch
                pen = VTCell.blank
                pen.ch = ch
            case 1: pen.bold = true
            case 2: pen.dim = true
            case 3: pen.italic = true
            case 4: pen.underline = true
            case 7: pen.inverse = true
            case 21, 22: pen.bold = false; pen.dim = false
            case 23: pen.italic = false
            case 24: pen.underline = false
            case 27: pen.inverse = false
            case 30...37: pen.fg = .palette(UInt8(code - 30))
            case 39: pen.fg = .standard
            case 40...47: pen.bg = .palette(UInt8(code - 40))
            case 49: pen.bg = .standard
            case 90...97: pen.fg = .palette(UInt8(code - 90 + 8))
            case 100...107: pen.bg = .palette(UInt8(code - 100 + 8))
            case 38, 48:
                let isFg = code == 38
                if i + 1 < list.count, list[i + 1] == 5, i + 2 < list.count {
                    let idx = UInt8(clamping: list[i + 2])
                    if isFg { pen.fg = .palette(idx) } else { pen.bg = .palette(idx) }
                    i += 2
                } else if i + 1 < list.count, list[i + 1] == 2, i + 4 < list.count {
                    let r = UInt8(clamping: list[i + 2])
                    let g = UInt8(clamping: list[i + 3])
                    let b = UInt8(clamping: list[i + 4])
                    if isFg { pen.fg = .rgb(r, g, b) } else { pen.bg = .rgb(r, g, b) }
                    i += 4
                } else {
                    i = list.count // malformed — bail on this SGR
                }
            default:
                break
            }
            i += 1
        }
    }

    // MARK: Modes (DECSET/DECRST subset)

    private mutating func setMode(_ params: [Int], isPrivate: Bool, enable: Bool) {
        guard isPrivate else { return }
        for param in params {
            switch param {
            case 25:
                cursorVisible = enable
            case 47, 1047:
                setAltScreen(enable, saveCursor: false)
            case 1049:
                setAltScreen(enable, saveCursor: true)
            default:
                break // mouse modes, bracketed paste, focus events, …
            }
        }
    }

    private mutating func setAltScreen(_ enable: Bool, saveCursor: Bool) {
        if enable {
            guard !usingAltScreen else { return }
            if saveCursor { savedPrimaryCursor = (cursorRow, cursorCol) }
            savedPrimaryGrid = grid
            grid = Self.blankGrid(cols: cols, rows: rows)
            usingAltScreen = true
            cursorRow = 0
            cursorCol = 0
        } else {
            guard usingAltScreen else { return }
            grid = savedPrimaryGrid ?? Self.blankGrid(cols: cols, rows: rows)
            savedPrimaryGrid = nil
            usingAltScreen = false
            if saveCursor {
                cursorRow = min(savedPrimaryCursor.row, rows - 1)
                cursorCol = min(savedPrimaryCursor.col, cols - 1)
            }
        }
        scrollTop = 0
        scrollBottom = rows - 1
        pendingWrap = false
    }

    private mutating func fullReset() {
        grid = Self.blankGrid(cols: cols, rows: rows)
        savedPrimaryGrid = nil
        usingAltScreen = false
        scrollTop = 0
        scrollBottom = rows - 1
        cursorRow = 0
        cursorCol = 0
        cursorVisible = true
        pen = .blank
        pendingWrap = false
        savedCursor = nil
    }

    // MARK: Grid helpers

    private func blankLine() -> [VTCell] {
        Array(repeating: eraseCell, count: cols)
    }

    private static func blankGrid(cols: Int, rows: Int, fill: VTCell = .blank) -> [[VTCell]] {
        Array(repeating: Array(repeating: fill, count: cols), count: rows)
    }

    private static func regrid(_ old: [[VTCell]], cols: Int, rows: Int) -> [[VTCell]] {
        var next = old
        if next.count > rows {
            next.removeLast(next.count - rows)
        } else if next.count < rows {
            next.append(contentsOf: Array(
                repeating: Array(repeating: VTCell.blank, count: cols),
                count: rows - next.count
            ))
        }
        for i in 0..<next.count {
            if next[i].count > cols {
                next[i].removeLast(next[i].count - cols)
            } else if next[i].count < cols {
                next[i].append(contentsOf: Array(repeating: VTCell.blank, count: cols - next[i].count))
            }
        }
        return next
    }
}

// MARK: - Debug/test convenience

extension VTScreen {
    /// The row's characters as a string (trailing blanks kept) — test helper.
    public func rowText(_ row: Int) -> String {
        String(cells(atRow: row).map(\.ch))
    }

    /// The row's characters with trailing spaces trimmed — test helper.
    public func trimmedRowText(_ row: Int) -> String {
        var text = rowText(row)
        while text.hasSuffix(" ") { text.removeLast() }
        return text
    }
}
