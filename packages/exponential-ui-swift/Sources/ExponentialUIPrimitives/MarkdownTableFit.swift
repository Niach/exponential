import Foundation

/// #FEED-224 (`fixtures/table-fit.json`): THE column fit of a Markdown
/// table, the twin of packages/exponential-ui/src/table-fit.ts. Each column
/// comes measured in content px (`min` = its widest unbreakable piece, a
/// chip whole; `max` = its widest cell on one line); `fit` returns
/// whole-pixel content widths: under `available` when every column fits on
/// one line (the table hugs its content), over it when the minimums do not
/// fit (the table scrolls sideways).
public enum MarkdownTableFit {
    /// The widest a column's min-content counts (the fixture's `wordCap`).
    public static let wordCap: Double = 160

    public struct Column: Equatable, Sendable {
        public var min: Double
        public var max: Double
        public init(min: Double, max: Double) {
            self.min = min
            self.max = max
        }
    }

    public static func fit(_ columns: [Column], available: Double, cap: Double = wordCap) -> [Double] {
        let avail = Swift.max(available, 0).rounded(.down)
        let lo = columns.map { Swift.min(Swift.max($0.min, 0).rounded(.up), cap) }
        let hi = columns.indices.map { Swift.max(Swift.max(columns[$0].max, 0).rounded(.up), lo[$0]) }
        if lo.reduce(0, +) >= avail { return lo }
        if hi.reduce(0, +) <= avail { return hi }
        // Water-fill: one level L with Σ clamp(L, lo, hi) = avail.
        func at(_ level: Double) -> Double { lo.indices.reduce(0) { $0 + Swift.min(Swift.max(level, lo[$1]), hi[$1]) } }
        let points = Array(Set(lo + hi)).sorted()
        var k = 0
        while at(points[k + 1]) < avail { k += 1 }
        let a = points[k]
        let growing = lo.indices.filter { lo[$0] <= a && hi[$0] > a }
        let level = a + (avail - at(a)) / Double(growing.count)
        var out = lo.indices.map { Swift.min(Swift.max(level.rounded(.down), lo[$0]), hi[$0]) }
        var left = avail - out.reduce(0, +)
        for i in growing {
            if left <= 0 { break }
            if out[i] < hi[i] {
                out[i] += 1
                left -= 1
            }
        }
        return out
    }
}
