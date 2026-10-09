import Foundation

/// EXP-1248: the BIG session row's caption, ×4 (web
/// `apps/web/src/lib/session-row-caption.ts`, desktop `domain::session_row`,
/// Android `SessionRowCaption.kt`), locked by
/// `packages/domain-contract/fixtures/list-item.json`. The live states and
/// their tones are `session-display.json`'s (`CodingSessionDisplayState`);
/// this only words them for a list: "<State> · <device> · <age>".
public enum SessionRowCaption {
    private static let minute: Double = 60_000
    private static let hour: Double = 60 * minute
    private static let day: Double = 24 * hour

    /// The list's coarse age ladder: `now`, `5 min`, `21 h`, `2 d` (floored;
    /// a negative span reads `now`).
    public static func listElapsed(ms: Double) -> String {
        if !(ms >= minute) { return "now" }
        if ms < hour { return "\(Int((ms / minute).rounded(.down))) min" }
        if ms < day { return "\(Int((ms / hour).rounded(.down))) h" }
        return "\(Int((ms / day).rounded(.down))) d"
    }

    /// The caption line and its tone.
    public struct Caption: Equatable, Sendable {
        public let text: String
        public let tone: SessionStatusTone

        public init(text: String, tone: SessionStatusTone) {
            self.text = text
            self.tone = tone
        }
    }

    private static func liveWord(_ state: CodingSessionDisplayState) -> String {
        switch state {
        case .working: "Building"
        case .needsInput: "Needs input"
        case .review: "Ready for review"
        case .done: "Done"
        }
    }

    /// First match wins: ended → `Done · <device> · <when>` muted (when =
    /// now − (endedAt else updatedAt)); paused → `Paused · <device>` muted;
    /// a usage wall's badge label → that label, amber; else the live display
    /// state with its `statusTone` (working/needs input age from startedAt,
    /// review/done from updatedAt). A missing device or an unparsable stamp
    /// drops that segment.
    ///
    /// - Parameters:
    ///   - ended: `PastRuns.hasEnded(session)`.
    ///   - paused: an offline host (`SessionDevicePresentation.isPaused`).
    ///   - state: `CodingSessionDisplayState.of(...)`; ignored once ended.
    ///   - device: the resolved device label; nil drops the segment.
    ///   - blockedLabel: `AgentUsagePresentation.blockedBadgeLabel`, or nil.
    public static func sessionRowCaption(
        ended: Bool,
        paused: Bool,
        state: CodingSessionDisplayState,
        device: String?,
        startedAt: String?,
        updatedAt: String?,
        endedAt: String?,
        blockedLabel: String? = nil,
        now: Date
    ) -> Caption {
        func since(_ stamp: String?) -> String? {
            guard let stamp, !stamp.isEmpty, let at = WireTimestamps.parse(stamp) else { return nil }
            return listElapsed(ms: (now.timeIntervalSince1970 - at.timeIntervalSince1970) * 1000)
        }
        func join(_ parts: String?...) -> String {
            parts.compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
        }
        let trimmed = device?.trimmingCharacters(in: .whitespacesAndNewlines)
        let deviceLabel = (trimmed?.isEmpty ?? true) ? nil : trimmed

        if ended {
            return Caption(
                text: join("Done", deviceLabel, since(endedAt) ?? since(updatedAt)), tone: .muted
            )
        }
        if paused { return Caption(text: join("Paused", deviceLabel), tone: .muted) }
        if let blockedLabel, !blockedLabel.isEmpty {
            return Caption(text: blockedLabel, tone: .amber)
        }
        let live = state == .working || state == .needsInput ? since(startedAt) : since(updatedAt)
        return Caption(text: join(liveWord(state), deviceLabel, live), tone: state.statusTone)
    }
}
