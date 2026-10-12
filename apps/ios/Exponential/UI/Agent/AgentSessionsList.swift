import ExpCore
import ExpUI
import SwiftUI

/// EXP-1248: one run as THE `SessionRow` — the Agent page's Running band,
/// the Recent sheet and an action's Runs, ×4 (web `session-tree.tsx`
/// `sessionRowFacts`). Decides the mark, the caption + tone
/// (`SessionRowCaption`, `list-item.json`) and the dimming; the host decides
/// where a tap goes.
struct SessionListRow: View {
    let session: CodingSessionEntity
    /// An issue run's identifier, a batch's `EXP-874 +2`; nil otherwise.
    let identifier: String?
    let title: String
    /// The PR state the display rule reads (the issue's, else the run's own).
    let prState: String?
    let device: SessionDevicePresentation
    let deviceIcon: String
    var guide: TreeGuide = TreeGuide()
    var size: SessionRow<AnyView>.Size = .big
    /// Pinned by previews; production reads the wall clock on every redraw.
    var now: Date = Date()

    var body: some View {
        let ended = PastRuns.hasEnded(session)
        let state = CodingSessionDisplayState.of(session: session, prState: prState)
        let paused = !ended && device.isPaused(state, status: session.status)
        let working = CodingSessionDisplayState.working(
            status: session.status, state: state, paused: paused, live: true
        )
        let caption = SessionRowCaption.sessionRowCaption(
            ended: ended,
            paused: paused,
            state: state,
            device: device.label,
            startedAt: session.startedAt,
            updatedAt: session.updatedAt,
            endedAt: session.endedAt,
            blockedLabel: ended ? nil : AgentUsagePresentation.blockedBadgeLabel(
                AgentUsagePresentation.parseBlocked(session.blocked), now: now
            ),
            now: now
        )
        SessionRow(
            size: size,
            identifier: identifier,
            title: title,
            caption: caption.text,
            captionTone: caption.tone,
            guide: guide,
            deviceIcon: deviceIcon,
            deviceName: device.label,
            dimmed: paused
        ) {
            AnyView(mark(ended: ended, state: state, paused: paused, working: working))
        }
    }

    @ViewBuilder
    private func mark(
        ended: Bool, state: CodingSessionDisplayState, paused: Bool, working: Bool
    ) -> some View {
        if ended {
            AgentRunMark(agent: session.agent, state: nil, ended: true)
        } else {
            AgentRunMark(
                agent: session.agent,
                state: runningRowMarkState(state, paused: paused, working: working),
                badgeSize: SessionRowLead.badgeSize
            )
        }
    }

    /// The host's glyph: the matched device's pick (`DeviceIconDisplay`), the
    /// generic device glyph when no row matched.
    static func deviceIcon(_ session: CodingSessionEntity, devices: [SteerDevice]?) -> String {
        if let id = session.deviceId, let device = devices?.first(where: { $0.deviceId == id }) {
            return DeviceIconDisplay.iconName(for: device)
        }
        return DeviceIconDisplay.iconName(icon: nil, kind: nil)
    }
}
