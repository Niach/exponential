import Foundation

/// #EXP-1789 (`minKept`): a middle cut keeping fewer characters than this
/// (start + end together) shows the `…` alone.
public let middleMinKept = 4

/// #EXP-1778 (`fixtures/text-truncate.json`): a Text `truncate: "middle"`
/// line (start…end), the rule gpui's `text_ellipsis_middle` applies (the TS
/// twin is packages/exponential-ui/src/text-truncate.ts). Per-character
/// widths are summed; a line that fits shows whole, else the `…` takes its
/// width off the box and the rest splits 2:1 between the longest start and
/// the longest end that fit; no room for either = the `…` alone, and so is a
/// cut keeping fewer than `middleMinKept` characters (#EXP-1789), so a
/// nearly-gone run never reads as stray glyphs. Characters = code points.
/// Public: the app's path labels cut by the same rule (#EXP-1789).
public func middleTruncateWidth(_ text: String, width: Double, affix: String = "…", measure: (String) -> Double) -> String {
    let chars = text.unicodeScalars.map { String($0) }
    let widths = chars.map(measure)
    if widths.reduce(0, +) <= width { return text }
    let budget = width - affix.unicodeScalars.reduce(0) { $0 + measure(String($1)) }
    if budget <= 0 { return affix }
    let frontBudget = budget * (2.0 / 3.0)
    let backBudget = budget - frontBudget
    var front = 0
    var frontWidth = 0.0
    while front < chars.count, frontWidth + widths[front] <= frontBudget {
        frontWidth += widths[front]
        front += 1
    }
    var back = chars.count
    var backWidth = 0.0
    while back > 0, backWidth + widths[back - 1] <= backBudget {
        back -= 1
        backWidth += widths[back]
    }
    if front >= back || front + chars.count - back < middleMinKept { return affix }
    return chars[..<front].joined() + affix + chars[back...].joined()
}
