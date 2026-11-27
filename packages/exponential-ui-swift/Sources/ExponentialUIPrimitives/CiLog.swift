import Foundation

/// #EXP-2204 (packages/exponential-ui/docs/ci-components-contract.md
/// §1-§3, §6): the pure rules of LogView/LogLines and the CodeBlock anchors
/// + blame, the SDK's src/ci-log.ts (eui-core `ci_log.rs`, Kotlin
/// `CiLog.kt`); `CiLogTests` replays fixtures/ci-log.json through it. Inputs
/// are Foundation JSON (`JSONSerialization` values); offsets are Unicode
/// code points (scalars), like the reference.
public enum CiLog {
    // MARK: §1 The status table

    /// The `ciStatus` enum, in catalogue order.
    public static let statuses = ["passed", "failed", "running", "queued", "cancelled", "skipped"]

    /// style.json `ciStatus.spinMs`: one turn of the running glyph.
    public static let spinMs: Double = 1000

    /// style.json `logView` / `codeBlock` (px, lines).
    public static let searchWidth: Double = 240
    public static let indent: Double = 16
    public static let maxLines: Double = 24
    public static let blameWidth: Double = 200

    /// One row of THE status table: glyph, spin, mark and text colour
    /// tokens (the neutral theme's), the a11y string id.
    public struct StatusEntry: Equatable {
        public let status: String
        public let icon: String
        public let spin: Bool
        public let color: String
        public let text: String
        public let label: String
    }

    static let table: [String: StatusEntry] = [
        "passed": StatusEntry(status: "passed", icon: "ci-passed", spin: false, color: "$color.success", text: "$color.foreground", label: "statusPassed"),
        "failed": StatusEntry(status: "failed", icon: "ci-failed", spin: false, color: "$color.destructive", text: "$color.foreground", label: "statusFailed"),
        "running": StatusEntry(status: "running", icon: "ci-running", spin: true, color: "$color.info", text: "$color.foreground", label: "statusRunning"),
        "queued": StatusEntry(status: "queued", icon: "ci-queued", spin: false, color: "$color.mutedForeground", text: "$color.foreground", label: "statusQueued"),
        "cancelled": StatusEntry(status: "cancelled", icon: "ci-skipped", spin: false, color: "$color.mutedForeground", text: "$color.mutedForeground", label: "statusCancelled"),
        "skipped": StatusEntry(status: "skipped", icon: "ci-skipped", spin: false, color: "$color.mutedForeground", text: "$color.mutedForeground", label: "statusSkipped"),
    ]

    /// A status value → a `ciStatus`: unknown or missing = `queued`.
    public static func status(_ value: Any?) -> String {
        guard let s = value as? String, statuses.contains(s) else { return "queued" }
        return s
    }

    /// THE table row of a status (unknown or missing = `queued`).
    public static func entry(_ value: Any?) -> StatusEntry { table[status(value)]! }

    // MARK: §2 Line anchors

    /// A 1-based inclusive line range.
    public struct LineRange: Equatable {
        public let start: Int
        public let end: Int
        public init(start: Int, end: Int) {
            self.start = start
            self.end = end
        }
        public func contains(_ line: Int) -> Bool { line >= start && line <= end }
    }

    private static func anchorNumber(_ b: [UInt8], _ from: Int) -> (value: Int, next: Int)? {
        var i = from
        while i < b.count, b[i] >= 0x30, b[i] <= 0x39 { i += 1 }
        guard i > from, i - from <= 9, let v = Int(String(decoding: b[from ..< i], as: UTF8.self)), v >= 1 else { return nil }
        return (v, i)
    }

    /// `L12` / `L6-L9` (an optional leading `#`) → the normalised range.
    public static func parseAnchor(_ value: Any?) -> LineRange? {
        guard let text = value as? String else { return nil }
        let b = Array(text.utf8)
        var i = b.first == 0x23 ? 1 : 0
        guard i < b.count, b[i] == 0x4C, let a = anchorNumber(b, i + 1) else { return nil }
        i = a.next
        if i == b.count { return LineRange(start: a.value, end: a.value) }
        guard i + 1 < b.count, b[i] == 0x2D, b[i + 1] == 0x4C, let c = anchorNumber(b, i + 2), c.next == b.count else { return nil }
        return LineRange(start: min(a.value, c.value), end: max(a.value, c.value))
    }

    /// `L<start>` when equal, else `L<min>-L<max>`.
    public static func formatAnchor(_ start: Int, _ end: Int? = nil) -> String {
        let e = end ?? start
        let lo = min(start, e), hi = max(start, e)
        return lo == hi ? "L\(lo)" : "L\(lo)-L\(hi)"
    }

    /// A press on `line`: no shift or no valid current anchor → `L<line>`;
    /// shift → the range from the current START to `line`.
    public static func extendAnchor(_ current: Any?, line: Int, shift: Bool) -> String {
        if shift, let r = parseAnchor(current) { return formatAnchor(r.start, line) }
        return formatAnchor(line)
    }

    // MARK: §3 The log model

    /// One content line (marker lines get none).
    public struct Line: Equatable {
        /// 1-based content line number.
        public let n: Int
        /// The open section's index, -1 outside every section.
        public let section: Int
        /// `HH:MM:SS` of a leading timestamp, as written.
        public let time: String?
        /// `error` | `warning` | `comment` from a `##[error]`-style prefix.
        public let kind: String?
        /// The line without the timestamp and the prefix, ANSI escapes kept.
        public let text: String
    }

    public struct Section: Equatable {
        public let index: Int
        public var title: String
        public var start: Int
        public var end: Int
        public var count: Int
        public var status: String
        public var duration: String?
    }

    public struct Model: Equatable {
        public var lines: [Line] = []
        public var sections: [Section] = []
        public init(lines: [Line] = [], sections: [Section] = []) {
            self.lines = lines
            self.sections = sections
        }

        /// The content line `n` (1-based).
        public func line(_ n: Int) -> Line? {
            guard n >= 1, n <= lines.count, lines[n - 1].n == n else { return nil }
            return lines[n - 1]
        }
    }

    /// The end index of a COMPLETE CSI escape at `i`, else nil.
    private static func csiEnd(_ t: [Unicode.Scalar], _ i: Int) -> Int? {
        guard i + 1 < t.count, t[i].value == 0x1B, t[i + 1] == "[" else { return nil }
        var j = i + 2
        while j < t.count, (0x30 ... 0x3F).contains(t[j].value) { j += 1 }
        while j < t.count, (0x20 ... 0x2F).contains(t[j].value) { j += 1 }
        return j < t.count && (0x40 ... 0x7E).contains(t[j].value) ? j + 1 : nil
    }

    /// The text with every complete ANSI CSI escape removed.
    public static func stripAnsi(_ text: String) -> String {
        guard text.unicodeScalars.contains(where: { $0.value == 0x1B }) else { return text }
        let t = Array(text.unicodeScalars)
        var out = String.UnicodeScalarView()
        var i = 0
        while i < t.count {
            if let end = csiEnd(t, i) {
                i = end
                continue
            }
            out.append(t[i])
            i += 1
        }
        return String(out)
    }

    /// `text` with its first `count` escape-free scalars removed, every
    /// escape kept where it was.
    private static func cutVisible(_ text: String, _ count: Int) -> String {
        let t = Array(text.unicodeScalars)
        var out = String.UnicodeScalarView()
        var removed = 0
        var i = 0
        while i < t.count {
            if let end = csiEnd(t, i) {
                out.append(contentsOf: t[i ..< end])
                i = end
                continue
            }
            if removed < count { removed += 1 } else { out.append(t[i]) }
            i += 1
        }
        return String(out)
    }

    /// `YYYY-MM-DDTHH:MM:SS(.fraction)?(Z|±HH:MM) ` at the start: the
    /// `HH:MM:SS` and the UTF-8 length including the space.
    private static func timestamp(_ text: String) -> (time: String, length: Int)? {
        let b = Array(text.utf8.prefix(64))
        func digits(_ from: Int, _ n: Int) -> Bool { b.count >= from + n && b[from ..< from + n].allSatisfy { $0 >= 0x30 && $0 <= 0x39 } }
        func at(_ i: Int, _ c: UInt8) -> Bool { i < b.count && b[i] == c }
        guard digits(0, 4), at(4, 0x2D), digits(5, 2), at(7, 0x2D), digits(8, 2), at(10, 0x54),
              digits(11, 2), at(13, 0x3A), digits(14, 2), at(16, 0x3A), digits(17, 2) else { return nil }
        var i = 19
        if at(i, 0x2E), digits(i + 1, 1) {
            i += 1
            while i < b.count, b[i] >= 0x30, b[i] <= 0x39 { i += 1 }
        }
        if at(i, 0x5A) {
            i += 1
        } else if at(i, 0x2B) || at(i, 0x2D), digits(i + 1, 2), at(i + 3, 0x3A), digits(i + 4, 2) {
            i += 6
        } else {
            return nil
        }
        guard at(i, 0x20) else { return nil }
        return (String(decoding: b[11 ..< 19], as: UTF8.self), i + 1)
    }

    private static let prefixes: [(String, String)] = [("##[error]", "error"), ("##[warning]", "warning"), ("##[notice]", "comment"), ("##[debug]", "comment")]

    /// ASCII space and tab trimmed off both ends.
    private static func trimSpaces(_ s: Substring) -> String {
        var t = s
        while let f = t.first, f == " " || f == "\t" { t = t.dropFirst() }
        while let l = t.last, l == " " || l == "\t" { t = t.dropLast() }
        return String(t)
    }

    /// Raw log lines (+ per-section `{status, duration}`) → numbered content
    /// lines and their `##[group]` sections (contract §3).
    public static func model(_ lines: Any?, sections meta: Any? = nil) -> Model {
        var out: [Line] = []
        var sections: [Section] = []
        var open = -1
        var closedLast = true
        let metas = meta as? [Any] ?? []
        for raw in lines as? [Any] ?? [] {
            var text = raw as? String ?? ""
            var time: String?
            if let stamp = timestamp(text) {
                time = stamp.time
                text = String(decoding: Array(text.utf8).dropFirst(stamp.length), as: UTF8.self)
            }
            let plain = stripAnsi(text)
            if plain.hasPrefix("##[group]") {
                if open >= 0 { sections[open].end = out.count }
                open = sections.count
                closedLast = false
                sections.append(Section(index: open, title: trimSpaces(plain.dropFirst("##[group]".count)), start: out.count + 1, end: out.count, count: 0, status: "passed", duration: nil))
                continue
            }
            if plain.hasPrefix("##[endgroup]") {
                if open >= 0 {
                    sections[open].end = out.count
                    open = -1
                    closedLast = true
                }
                continue
            }
            var kind: String?
            for (prefix, k) in prefixes where plain.hasPrefix(prefix) {
                kind = k
                text = cutVisible(text, prefix.unicodeScalars.count)
                break
            }
            out.append(Line(n: out.count + 1, section: open, time: time, kind: kind, text: text))
        }
        if open >= 0 { sections[open].end = out.count }
        let last = sections.count - 1
        for i in sections.indices {
            sections[i].count = sections[i].end - sections[i].start + 1
            let given = i < metas.count ? metas[i] as? [String: Any] : nil
            if let s = given?["status"], !(s is NSNull) {
                sections[i].status = status(s)
            } else {
                sections[i].status = !closedLast && i == last ? "running" : "passed"
            }
            sections[i].duration = given?["duration"] as? String
        }
        return Model(lines: out, sections: sections)
    }

    /// The sections that open on their own: `failed` or `running`.
    public static func openDefault(_ model: Model) -> [Int] {
        model.sections.filter { $0.status == "failed" || $0.status == "running" }.map(\.index)
    }

    /// A JSON number (never a bool), else nil.
    static func number(_ v: Any?) -> Double? {
        guard let n = v as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() else { return nil }
        return n.doubleValue
    }

    /// The section indexes a JSON `open` array names (anything else ignored).
    public static func openSet(_ open: Any?, _ model: Model) -> [Int] {
        let values = (open as? [Any] ?? []).compactMap(number)
        return model.sections.map(\.index).filter { values.contains(Double($0)) }
    }

    /// The current open sections: a given array, else the default.
    public static func resolvedOpen(_ open: Any?, _ model: Model) -> [Int] {
        open is [Any] ? openSet(open, model) : openDefault(model)
    }

    /// One flat row of a LogLines body.
    public enum Row: Equatable, Hashable {
        case line(Int)
        case section(Int)
    }

    /// The flat rows a LogLines body windows: lines outside sections, each
    /// section's head row, then its lines when its index is in `open`.
    public static func rows(_ model: Model, open: [Int]) -> [Row] {
        var rows: [Row] = []
        rows.reserveCapacity(model.lines.count + model.sections.count)
        var line = 0
        let lines = model.lines
        for s in model.sections {
            while line < lines.count, lines[line].n < s.start {
                rows.append(.line(lines[line].n))
                line += 1
            }
            rows.append(.section(s.index))
            let shown = open.contains(s.index)
            while line < lines.count, lines[line].n <= s.end {
                if shown { rows.append(.line(lines[line].n)) }
                line += 1
            }
        }
        while line < lines.count {
            rows.append(.line(lines[line].n))
            line += 1
        }
        return rows
    }

    /// One search match: scalar offsets into the escape-free text, end exclusive.
    public struct Match: Equatable {
        public let line: Int
        public let start: Int
        public let end: Int
        public init(line: Int, start: Int, end: Int) {
            self.line = line
            self.start = start
            self.end = end
        }
    }

    private static func folded(_ s: String) -> [UInt32] {
        s.unicodeScalars.map { $0.value >= 0x41 && $0.value <= 0x5A ? $0.value + 0x20 : $0.value }
    }

    /// Every non-overlapping match of `query` (ASCII case-insensitive) in
    /// every content line's escape-free text, folded sections too.
    public static func search(_ model: Model, query: Any?) -> [Match] {
        guard let q = query as? String, !q.isEmpty else { return [] }
        let needle = folded(q)
        var out: [Match] = []
        for line in model.lines {
            let t = folded(stripAnsi(line.text))
            var i = 0
            while i + needle.count <= t.count {
                if t[i ..< i + needle.count].elementsEqual(needle) {
                    out.append(Match(line: line.n, start: i, end: i + needle.count))
                    i += needle.count
                } else {
                    i += 1
                }
            }
        }
        return out
    }

    /// `open` plus the section holding `line` (sorted, unique, integers only).
    public static func reveal(_ model: Model, open: Any?, line: Int) -> [Int] {
        var set = Set<Int>()
        for v in (open as? [Any] ?? []).compactMap(number) where v.rounded() == v && abs(v) < 9_007_199_254_740_992 {
            set.insert(Int(v))
        }
        if let at = model.line(line), at.section >= 0 { set.insert(at.section) }
        return set.sorted()
    }

    // MARK: §6 CodeBlock blame

    /// One line's blame block.
    public struct BlameLine: Equatable {
        /// Index into the `blame` array, -1 before the first entry.
        public let block: Int
        /// The block's first line (shows author + age).
        public let first: Bool
        /// A first line of every block but the first shown: the hairline.
        public let border: Bool
        public init(block: Int, first: Bool, border: Bool) {
            self.block = block
            self.first = first
            self.border = border
        }
    }

    /// Per line (1…lineCount) its blame block: entries sorted by `start`
    /// (stable), each running to the next entry's start − 1.
    public static func blameBlocks(_ blame: Any?, lineCount: Int) -> [BlameLine] {
        var entries: [(index: Int, start: Double)] = []
        for (index, e) in (blame as? [Any] ?? []).enumerated() {
            if let s = number((e as? [String: Any])?["start"]), s.rounded() == s, s >= 1 { entries.append((index, s)) }
        }
        entries = entries.enumerated().sorted { $0.element.start != $1.element.start ? $0.element.start < $1.element.start : $0.offset < $1.offset }.map(\.element)
        var out: [BlameLine] = []
        out.reserveCapacity(max(lineCount, 0))
        var at = -1
        var firstShown = 0
        if lineCount >= 1 {
            for n in 1 ... lineCount {
                var first = false
                while at + 1 < entries.count, entries[at + 1].start <= Double(n) {
                    at += 1
                    first = true
                }
                if first, firstShown == 0 { firstShown = n }
                out.append(BlameLine(block: at >= 0 ? entries[at].index : -1, first: first, border: first && n != firstShown))
            }
        }
        return out
    }
}
