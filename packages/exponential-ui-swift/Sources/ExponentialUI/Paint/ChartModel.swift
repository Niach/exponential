import SwiftUI

/// The numbers a `Chart` leaf paints (contract §3 "Chart"): the series,
/// categories and flags of the props plus what the CORE computed into
/// `props.chart` (`chartExtent`, `niceTicks(min, max, 5)`, the series
/// colour tokens, the donut hole, whether the legend shows), so every
/// painter draws the same axes. Pure: the leaf resolves the colour tokens.
struct ChartModel {
    struct Series: Equatable {
        var name: String
        var values: [Double]
        var tone: String?
    }

    var kind: String
    var categories: [String]
    var series: [Series]
    var title: String
    var xLabel: String
    var yLabel: String
    /// The value extent the plot spans (the core's `chartExtent`).
    var min: Double
    var max: Double
    /// Axis ticks and grid lines (the core's `niceTicks`).
    var ticks: [Double]
    /// One colour token per series (per slice for pie / donut):
    /// `$color.chart1..8` or a tone's colour.
    var colorTokens: [String]
    /// The donut hole as a share of the radius (0 = pie).
    var hole: Double
    var showAxes: Bool
    var showGrid: Bool
    var showValues: Bool

    init(_ props: Props) {
        kind = props.str("kind").isEmpty ? "bar" : props.str("kind")
        categories = props.list("categories").map(\.displayText)
        series = props.list("series").map { s in
            Series(name: s["name"]?.displayText ?? "", values: (s["values"]?.array ?? []).map { $0.number ?? Double($0.displayText) ?? 0 }, tone: s["tone"]?.string)
        }
        title = props.str("title")
        xLabel = props.str("xLabel")
        yLabel = props.str("yLabel")
        let chart = props["chart"]?.object ?? [:]
        let all = series.flatMap(\.values)
        min = chart["min"]?.number ?? 0
        max = chart["max"]?.number ?? Swift.max(all.max() ?? 1, 1)
        ticks = chart["ticks"]?.array?.compactMap(\.number) ?? [min, max]
        colorTokens = chart["colors"]?.array?.compactMap(\.string) ?? []
        hole = chart["hole"]?.number ?? (kind == "donut" ? 0.6 : 0)
        let plain = kind != "sparkline" && kind != "pie" && kind != "donut"
        showAxes = plain && props["showAxes"]?.bool != false
        showGrid = plain && props["showGrid"]?.bool != false
        showValues = props["showValues"]?.bool == true
    }

    var isSlices: Bool { kind == "pie" || kind == "donut" }

    /// The slices of a pie / donut: the first series' values (negatives = 0).
    var sliceValues: [Double] { (series.first?.values ?? []).map { Swift.max($0, 0) } }

    /// The category slots along x.
    var categoryCount: Int { Swift.max(categories.count, series.map(\.values.count).max() ?? 0, 1) }

    /// The theme colour NAME behind series / slice `i` (`chart3`,
    /// `destructive`…): the core's token, else the palette in order.
    func colorName(_ i: Int) -> String {
        if let t = colorTokens[safe: i], t.hasPrefix("$color.") { return String(t.dropFirst("$color.".count)) }
        if !isSlices, let tone = series[safe: i]?.tone {
            switch tone {
            case "danger": return "destructive"
            case "neutral": return "mutedForeground"
            default: return tone
            }
        }
        return "chart\(i % 8 + 1)"
    }

    /// The legend entries (pie / donut: categories; else series names when
    /// 2+); none for a sparkline or when the core / `showLegend` hides it.
    static func legend(_ props: Props) -> [String] {
        let kind = props.str("kind")
        let shows = (props["chart"]?["legend"]?.bool ?? true) && props["showLegend"]?.bool != false
        if !shows || kind == "sparkline" { return [] }
        if kind == "pie" || kind == "donut" {
            return props.list("categories").map(\.displayText)
        }
        let series = props.list("series")
        return series.count > 1 ? series.map { $0["name"]?.displayText ?? "" } : []
    }

    /// A tick / value label: integers plain, else up to 2 decimals.
    static func format(_ v: Double) -> String {
        if abs(v - v.rounded()) < 1e-4 { return String(Int(v.rounded())) }
        var s = String(format: "%.2f", v)
        while s.hasSuffix("0") { s.removeLast() }
        if s.hasSuffix(".") { s.removeLast() }
        return s
    }

    /// The category under x in a plot `width` wide with `n` slots.
    static func category(at x: CGFloat, width: CGFloat, count n: Int) -> Int? {
        guard n > 0, width > 0, x >= 0, x <= width else { return nil }
        return Swift.min(Int((x / width * CGFloat(n)).rounded(.down)), n - 1)
    }

    /// The slice under a point (relative to the centre), clockwise from 12.
    static func slice(dx: CGFloat, dy: CGFloat, radius r: CGFloat, hole: CGFloat, values: [Double]) -> Int? {
        let d = (dx * dx + dy * dy).squareRoot()
        let total = values.reduce(0, +)
        guard d <= r, d >= hole * r, total > 0 else { return nil }
        var a = atan2(dy, dx) + .pi / 2
        if a < 0 { a += 2 * .pi }
        var acc: CGFloat = 0
        for (i, v) in values.enumerated() {
            acc += 2 * .pi * CGFloat(v / total)
            if a <= acc { return i }
        }
        return values.isEmpty ? nil : values.count - 1
    }
}
