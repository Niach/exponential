import ExpCore
import SwiftUI

/// EXP-1191 — the agent task list's own mark ×4, so its line above the
/// composer never reads as a queued steer message or as the composer: one
/// short segment per task, done ones solid, the current one half, the rest
/// faint. A list longer than `maxSegments` draws one continuous track filled
/// to `done / total` instead. Web `TaskListProgress` (`@exp/ui`), desktop
/// `task_list_progress`, Android `TaskListProgress`.
public struct TaskListProgress: View {
    public static let maxSegments = 12

    private static let segmentWidth: CGFloat = 10
    private static let segmentHeight: CGFloat = 4
    private static let segmentGap: CGFloat = 3
    private static let trackWidth: CGFloat = 80

    private static let doneOpacity = 0.70
    private static let currentOpacity = 0.35
    private static let pendingOpacity = 0.12

    let statuses: [AgentTaskListStatus]

    public init(statuses: [AgentTaskListStatus]) {
        self.statuses = statuses
    }

    public var body: some View {
        if !statuses.isEmpty {
            Group {
                if statuses.count > Self.maxSegments {
                    track
                } else {
                    segments
                }
            }
            .accessibilityHidden(true)
        }
    }

    private var segments: some View {
        HStack(spacing: Self.segmentGap) {
            // Positional: the agent rewrites the list whole.
            ForEach(Array(statuses.enumerated()), id: \.offset) { _, status in
                Capsule()
                    .fill(Color.white.opacity(Self.opacity(status)))
                    .frame(width: Self.segmentWidth, height: Self.segmentHeight)
            }
        }
        .fixedSize()
    }

    private var track: some View {
        let done = statuses.filter { $0 == .completed }.count
        let fraction = CGFloat(done) / CGFloat(statuses.count)
        return ZStack(alignment: .leading) {
            Capsule()
                .fill(Color.white.opacity(Self.pendingOpacity))
            Capsule()
                .fill(Color.white.opacity(Self.doneOpacity))
                .frame(width: Self.trackWidth * fraction)
        }
        .frame(width: Self.trackWidth, height: Self.segmentHeight)
        .clipShape(Capsule())
    }

    private static func opacity(_ status: AgentTaskListStatus) -> Double {
        switch status {
        case .completed: doneOpacity
        case .inProgress: currentOpacity
        case .pending: pendingOpacity
        }
    }
}
