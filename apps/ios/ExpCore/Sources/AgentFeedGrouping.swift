import Foundation

/// Pure grouping helper for the agent-session activity feed (EXP-97) — kept in
/// ExpCore so it's unit-testable (the app target has no test target). Mirrors
/// the web `groupToolRuns` / Android `groupToolRuns` semantics: runs of ≥2
/// CONSECUTIVE tool items collapse into one "N tool calls" render row; a lone
/// tool stays a single row.
public enum AgentFeedGrouping {
    /// Index ranges of consecutive runs (length ≥ 2) where `isTool` is true.
    /// The caller maps each range onto its feed slice; everything outside the
    /// ranges renders as single rows. A pure render-time projection — the flat
    /// feed (and the trailing-question rule over it) is never restructured.
    public static func toolRunRanges(isTool: [Bool]) -> [Range<Int>] {
        var ranges: [Range<Int>] = []
        var i = 0
        while i < isTool.count {
            guard isTool[i] else {
                i += 1
                continue
            }
            var end = i
            while end + 1 < isTool.count, isTool[end + 1] { end += 1 }
            if end > i { ranges.append(i..<(end + 1)) }
            i = end + 1
        }
        return ranges
    }
}
