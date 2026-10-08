import SwiftUI

/// The chart data the `Chart` native paints (bar / line / area / pie).
struct ChartModel {
    struct Series {
        var name: String
        var values: [Double]
        var tone: String?
    }

    var kind: String
    var categories: [String]
    var series: [Series]
    var title: String
    var height: CGFloat

    init(_ props: Props) {
        kind = props.str("kind").isEmpty ? "bar" : props.str("kind")
        categories = props.list("categories").map(\.displayText)
        series = props.list("series").map { s in
            Series(name: s["name"]?.displayText ?? "", values: (s["values"]?.array ?? []).compactMap(\.number), tone: s["tone"]?.string)
        }
        title = props.str("title")
        height = CGFloat(props.num("height") ?? 200)
    }

    var maxValue: Double {
        let m = series.flatMap(\.values).max() ?? 0
        return m > 0 ? m : 1
    }

    /// The legend entries (pie: categories; else series names when > 1).
    static func legend(_ props: Props) -> [String] {
        let kind = props.str("kind")
        let series = props.list("series")
        if kind == "pie" {
            return props.list("categories").map(\.displayText)
        }
        if series.count > 1 {
            return series.map { $0["name"]?.displayText ?? "" }
        }
        return []
    }
}
