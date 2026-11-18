import SwiftUI
import ExponentialUIPrimitives

#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

/// #EDE-95: paints genUI's search marks (`TextHighlight`): `$color.match`
/// behind each match, `$color.matchCurrent` behind the current one, as the
/// runs' `.backgroundColor` (paint only: the text keeps its colour, its
/// shaping and its measured lines never change).
enum HighlightPainter {
    /// The two fills (`match`, `matchCurrent`) of the surface's theme.
    @MainActor
    static func colors(_ model: SurfaceModel) -> (match: Color, current: Color) {
        let match = model.color("match") ?? Color.yellow.opacity(0.35)
        return (match, model.color("matchCurrent") ?? model.color("match") ?? Color.orange.opacity(0.6))
    }

    static func platform(_ c: Color) -> PlatformColor {
        #if canImport(UIKit)
        UIColor(c)
        #else
        NSColor(c)
        #endif
    }

    /// The text a markdown block's typeset runs (`TextShaper.runs`) search
    /// as, the normal form's: an inline chip = ONE `TextHighlight.barrier`,
    /// inline code's side padding and the word joiners the shaper glued
    /// around chips and code dropped. `map[i]` = the runs' UTF-16 offset of
    /// the searched text's unit `i`.
    static func searchText(of runs: NSAttributedString) -> (text: String, map: [Int]) {
        let ns = runs.string as NSString
        let n = ns.length
        guard n > 0 else { return ("", []) }
        var chip = [Bool](repeating: false, count: n)
        var pad = [Bool](repeating: false, count: n)
        var code = [Bool](repeating: false, count: n)
        let all = NSRange(location: 0, length: n)
        runs.enumerateAttribute(TextShaper.chipKey, in: all) { v, r, _ in
            if v != nil { for i in r.location..<NSMaxRange(r) { chip[i] = true } }
        }
        runs.enumerateAttribute(TextShaper.codeKey, in: all) { v, r, _ in
            if v != nil { for i in r.location..<NSMaxRange(r) { code[i] = true } }
        }
        runs.enumerateAttribute(.attachment, in: all) { v, r, _ in
            if v != nil { for i in r.location..<NSMaxRange(r) { pad[i] = true } }
        }
        var units: [UInt16] = []
        var map: [Int] = []
        units.reserveCapacity(n)
        map.reserveCapacity(n)
        for i in 0..<n {
            let u = ns.character(at: i)
            if chip[i] {
                // A chip: one barrier per chip run (its one object character).
                units.append(UInt16(TextHighlight.barrier.value))
                map.append(i)
                continue
            }
            // Inline code's side padding (an object character, no chip).
            if u == 0xFFFC, code[i], pad[i] { continue }
            if u == 0x2060 {
                let near = { (j: Int) in j >= 0 && j < n }
                // Inline code's glue, between its padding and its text.
                if code[i], (near(i - 1) && pad[i - 1] && code[i - 1]) || (near(i + 1) && pad[i + 1] && code[i + 1]) { continue }
                // The joiner the shaper glued a chip to its text with.
                if (near(i - 1) && chip[i - 1]) || (near(i + 1) && chip[i + 1]) { continue }
            }
            units.append(u)
            map.append(i)
        }
        return (String(decoding: units, as: UTF16.self), map)
    }

    /// `runs` with the matches of `query` filled (`current` = the index
    /// among THESE runs' matches painted current, -1 = none) and how many
    /// it has; the runs themselves when none.
    static func marked(_ runs: NSAttributedString, query: String, current: Int, match: PlatformColor, currentFill: PlatformColor) -> (text: NSAttributedString, count: Int) {
        let (text, map) = searchText(of: runs)
        let found = TextHighlight.ranges(text, query: query)
        guard !found.isEmpty else { return (runs, 0) }
        let out = NSMutableAttributedString(attributedString: runs)
        for (k, r) in found.enumerated() where r.end > r.start && r.end <= map.count {
            let start = map[r.start], end = map[r.end - 1] + 1
            out.addAttribute(.backgroundColor, value: k == current ? currentFill : match, range: NSRange(location: start, length: end - start))
        }
        return (out, found.count)
    }

    /// The UTF-16 offset in `text` each unit of each shown `line` came from
    /// (nil = a unit the text does not hold: a `…`). The lines are the
    /// text's in order, trailing spaces dropped and a clamp's lines joined
    /// with a space, so a unit matches the text's next equal unit (any
    /// space matching any space or line break). `middle` = the one-line
    /// text a `truncate: middle` line cut (start…end): its common start and
    /// end map, the cut does not.
    static func lineOffsets(_ text: String, lines: [String], middle: String? = nil) -> [[Int?]] {
        let t = Array(text.utf16)
        if let middle {
            let full = Array(middle.utf16)
            guard full.count == t.count else { return lines.map { [Int?](repeating: nil, count: $0.utf16.count) } }
            return lines.map { line -> [Int?] in
                let l = Array(line.utf16)
                if l == full { return l.indices.map { Optional($0) } }
                var p = 0
                while p < l.count, p < full.count, l[p] == full[p] { p += 1 }
                var s = 0
                while s < l.count - p, s < full.count - p, l[l.count - 1 - s] == full[full.count - 1 - s] { s += 1 }
                return l.indices.map { j -> Int? in
                    if j < p { return j }
                    if j >= l.count - s { return full.count - (l.count - j) }
                    return nil
                }
            }
        }
        func space(_ u: UInt16) -> Bool { u == 0x20 || u == 0x09 || u == 0x0A || u == 0x0B || u == 0x0C || u == 0x0D || u == 0x85 || u == 0x2028 || u == 0x2029 }
        var cursor = 0
        return lines.map { line -> [Int?] in
            line.utf16.map { u -> Int? in
                var c = cursor
                while c < t.count, !(t[c] == u || (space(t[c]) && space(u))) { c += 1 }
                guard c < t.count else { return nil }
                cursor = c + 1
                return c
            }
        }
    }

    /// The marks of a Text leaf's shown lines: the matches of the spec in
    /// the node's `text` (what a host numbers), each painted over the units
    /// of each line it came from; `current` = the local current match.
    static func lineMarks(_ text: String, lines: [String], spec: TextHighlightSpec, middle: String? = nil) -> [[(range: NSRange, current: Bool)]] {
        let found = TextHighlight.ranges(text, query: spec.query)
        guard !found.isEmpty else { return lines.map { _ in [] } }
        let local = spec.localCurrent
        // Which match each text unit belongs to (-1 = none).
        var owner = [Int](repeating: -1, count: (text as NSString).length)
        for (k, r) in found.enumerated() {
            for i in r.start..<min(r.end, owner.count) { owner[i] = k }
        }
        return lineOffsets(text, lines: lines, middle: middle).map { offsets -> [(range: NSRange, current: Bool)] in
            var out: [(range: NSRange, current: Bool)] = []
            var runStart = -1
            var runOwner = -1
            func close(_ j: Int) {
                if runOwner >= 0 { out.append((NSRange(location: runStart, length: j - runStart), runOwner == local)) }
                runOwner = -1
            }
            for (j, o) in offsets.enumerated() {
                let k = o.map { $0 < owner.count ? owner[$0] : -1 } ?? -1
                if k != runOwner {
                    close(j)
                    if k >= 0 {
                        runStart = j
                        runOwner = k
                    }
                }
            }
            close(offsets.count)
            return out
        }
    }
}
