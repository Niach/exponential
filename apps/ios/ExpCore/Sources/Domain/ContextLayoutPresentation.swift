import Foundation

// EXP-1051: where a run's context window actually GOES — the stacked bar and
// its legend, folded from the `usage` meter (how full the window is) plus the
// device's `context_layout` state (what the launcher put in it before the
// first turn).
//
// Hand-mirrored ×4 and byte-locked by the ONE contract fixture every client's
// test iterates (`packages/domain-contract/fixtures/context-layout.json`):
//   web      apps/web/src/lib/context-layout.ts
//   desktop  apps/desktop/crates/ui/src/context_layout.rs
//   Android  apps/android/.../domain/ContextLayoutPresentation.kt
// Changing a rule or a string here means changing it in all four and in the
// fixture — the fixture IS the spec.
//
// Two rules the renderers depend on:
//  • `conversation` and `free` are DERIVED here, never on the wire: the device
//    publishes only what it can attribute, and everything else the window
//    holds is the conversation.
//  • The segments are NEVER scaled to fit. When the device's estimates
//    overshoot the measured `contextUsed` the percents still add past 100 and
//    the renderer CLIPS — a silently rescaled bar would lie about the layer
//    sizes to hide a rounding error.

/// One slice of the stacked bar. `percent` is of the WHOLE window (size), to
/// two decimals — small layers (a 600-token task prompt in a 200k window) are
/// a hairline rather than nothing. `free` is not a slice: it is the track.
public struct ContextBarSlice: Equatable, Sendable, Identifiable {
    public let key: String
    public let tone: String
    public let percent: Double

    public var id: String { key }

    public init(key: String, tone: String, percent: Double) {
        self.key = key
        self.tone = tone
        self.percent = percent
    }
}

/// One legend row. Both numbers are already FORMATTED — the ×4 lock is on the
/// strings, not on the arithmetic behind them.
public struct ContextLegendRow: Equatable, Sendable, Identifiable {
    public let key: String
    public let label: String
    public let tone: String
    /// `21k`, `37.4k`, `600` — see `tokensCompact`.
    public let tokens: String
    /// `10.5%`, `0.3%` — one decimal, always.
    public let percent: String
    /// The device guessed this layer rather than measuring it; the UI prefixes
    /// `≈`. Always false for the derived rows.
    public let estimated: Bool
    /// What the layer is made of, when the device named it (`CLAUDE.md,
    /// ~/.claude/CLAUDE.md`).
    public let detail: String?

    public var id: String { key }

    public init(
        key: String, label: String, tone: String, tokens: String, percent: String,
        estimated: Bool, detail: String? = nil
    ) {
        self.key = key
        self.label = label
        self.tone = tone
        self.tokens = tokens
        self.percent = percent
        self.estimated = estimated
        self.detail = detail
    }
}

public struct ContextWindowView: Equatable, Sendable {
    /// `65k / 200k (32%)` — `AgentUsagePresentation.formatContextUsage`,
    /// unchanged.
    public let headline: String
    /// 0-100, floored (`AgentSessionUsage.percent`).
    public let percent: Int
    public let severity: AgentUsageSeverity
    /// Where the bar draws its marks: the compaction floor, then the two usage
    /// thresholds.
    public let ticks: [Int]
    public let bar: [ContextBarSlice]
    public let legend: [ContextLegendRow]

    public init(
        headline: String, percent: Int, severity: AgentUsageSeverity, ticks: [Int],
        bar: [ContextBarSlice], legend: [ContextLegendRow]
    ) {
        self.headline = headline
        self.percent = percent
        self.severity = severity
        self.ticks = ticks
        self.bar = bar
        self.legend = legend
    }
}

public enum ContextLayoutPresentation {
    /// EXP-1051 section title. Byte-identical ×4.
    public static let title = DomainContract.contextLayoutTitle

    /// `21k`, `37.4k`, `1.5k`, `134.7k`, and the raw number under 1000
    /// (`600`). One decimal, with a trailing `.0` dropped — `21.0k` reads as
    /// false precision on a number the device estimated.
    public static func tokensCompact(_ tokens: Int) -> String {
        let value = max(0, tokens)
        if value < 1000 { return "\(value)" }
        // One decimal, rounded the way the other three round it (half away
        // from zero on the SAME double), built by hand so no locale or
        // formatter gets a say.
        let tenths = Int(((Double(value) / 1000) * 10).rounded(.toNearestOrAwayFromZero))
        let whole = tenths / 10
        let fraction = tenths % 10
        return fraction == 0 ? "\(whole)k" : "\(whole).\(fraction)k"
    }

    /// The whole context-window view, or nil when there is no window to draw:
    /// the engine has published no `usage` yet, or it reported a zero size
    /// ("unknown"). A layout WITHOUT a usage is nothing — the bar has no
    /// scale.
    ///
    /// `conversation` = what the window holds that the device could not
    /// attribute (clamped at 0 when its estimates overshoot), `free` = the
    /// rest of the window (clamped at 0 once a run runs past its own size).
    public static func contextWindowView(
        usage: AgentSessionUsage?, segments: [ContextSegment]?
    ) -> ContextWindowView? {
        guard let usage, let percent = usage.percent else { return nil }
        guard let headline = AgentUsagePresentation.formatContextUsage(
            used: usage.contextUsed, size: usage.contextSize
        ) else { return nil }
        let size = usage.contextSize
        let used = max(0, usage.contextUsed)

        let known = knownSegments(segments)
        let attributed = known.reduce(0) { $0 + $1.tokens }
        let conversation = max(0, used - attributed)
        let free = max(0, size - used)

        var bar = known.map { entry in
            ContextBarSlice(key: entry.key, tone: entry.tone, percent: barPercent(entry.tokens, size))
        }
        // Always drawn, even at zero: the conversation is the slice that
        // GROWS, and a bar that gains a segment mid-run would re-key its
        // slices.
        bar.append(ContextBarSlice(
            key: derivedKey(0), tone: derivedTone(0), percent: barPercent(conversation, size)
        ))

        var legend = known.map { entry in
            ContextLegendRow(
                key: entry.key,
                label: entry.label,
                tone: entry.tone,
                tokens: tokensCompact(entry.tokens),
                percent: legendPercent(entry.tokens, size),
                estimated: entry.source == estimatedSource,
                detail: entry.detail
            )
        }
        for (index, tokens) in [conversation, free].enumerated() {
            legend.append(ContextLegendRow(
                key: derivedKey(index),
                label: derivedLabel(index),
                tone: derivedTone(index),
                tokens: tokensCompact(tokens),
                percent: legendPercent(tokens, size),
                // Derived from the engine's own measurement — never a guess.
                estimated: false
            ))
        }

        return ContextWindowView(
            headline: headline,
            percent: percent,
            severity: AgentUsagePresentation.severity(Double(percent)),
            // The compaction floor first: below it
            // `exponential_sessions_compact` refuses, so the tick is what
            // makes "not yet" legible.
            ticks: [DomainContract.contextLayoutCompactMinPercent, 75, 95],
            bar: bar,
            legend: legend
        )
    }

    // MARK: - Internals

    /// The `source` that earns the `≈` prefix — the second
    /// `DomainContract.contextLayoutSourceValues` entry, matched by value like
    /// the other three clients match it.
    private static let estimatedSource = "estimated"

    /// One wire segment this client KNOWS, carrying the contract's label and
    /// tone with it.
    private struct KnownSegment {
        let key: String
        let label: String
        let tone: String
        let tokens: Int
        let source: String
        let detail: String?
    }

    /// The wire segments this client KNOWS, in the contract's render order: an
    /// unknown key is dropped (an older client never draws a layer it cannot
    /// label), the FIRST of a duplicate key wins, a negative count reads as
    /// zero and a zero-token layer never draws at all.
    private static func knownSegments(_ segments: [ContextSegment]?) -> [KnownSegment] {
        guard let segments else { return [] }
        var out: [KnownSegment] = []
        for (index, key) in DomainContract.contextLayoutSegmentKeys.enumerated() {
            guard let segment = segments.first(where: { $0.key == key }) else { continue }
            let tokens = max(0, segment.tokens)
            if tokens == 0 { continue }
            out.append(KnownSegment(
                key: key,
                label: DomainContract.contextLayoutSegmentLabels[index],
                tone: DomainContract.contextLayoutSegmentTones[index],
                tokens: tokens,
                source: segment.source,
                detail: (segment.detail?.isEmpty == false) ? segment.detail : nil
            ))
        }
        return out
    }

    private static func derivedKey(_ index: Int) -> String {
        DomainContract.contextLayoutDerivedKeys[index]
    }

    private static func derivedLabel(_ index: Int) -> String {
        DomainContract.contextLayoutDerivedLabels[index]
    }

    private static func derivedTone(_ index: Int) -> String {
        DomainContract.contextLayoutDerivedTones[index]
    }

    /// Percent of the whole window, to TWO decimals — the bar's geometry.
    /// Multiply before divide, so the four clients agree on the rounding of an
    /// exact half.
    private static func barPercent(_ tokens: Int, _ size: Int) -> Double {
        (Double(tokens) * 10_000 / Double(size)).rounded(.toNearestOrAwayFromZero) / 100
    }

    /// Percent of the whole window, to ONE decimal plus the sign — the
    /// legend's string. Rounded from the same arithmetic as `barPercent`, so a
    /// row reading `0.0%` is a row whose slice is a hairline, never a rounding
    /// disagreement between the two.
    private static func legendPercent(_ tokens: Int, _ size: Int) -> String {
        let tenths = Int((Double(tokens) * 1_000 / Double(size)).rounded(.toNearestOrAwayFromZero))
        return "\(tenths / 10).\(tenths % 10)%"
    }
}
