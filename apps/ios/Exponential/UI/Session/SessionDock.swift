import ExpCore
import ExpUI
import SwiftUI

/// N-12: a session's Dock beyond the asks (web `useSessionDock`): THE Queue
/// (task-queue.json) and the status banners (a dropped stream with its Reconnect, EXP-804
/// the usage wall with EXP-849's account switch, EXP-724 compacting, EXP-784
/// the rate-limit window, the starting line) as `Dock.Source`s with their
/// sheets (dock.json `banner`: the host's words). EXP-1374: a background or
/// queued session leads with its own card (`SessionTree.sessionBanner`,
/// session-card.json `banner`). S-3: no paused strip, no interrupted card
/// (an offline host is invisible); #EXP-1703: a session on a person because
/// its device stayed away gets the `offline` card with Move. The session view and a chat's
/// Conversation share it.
@MainActor
enum SessionDock {
    /// EXP-550: the hosting machine is asleep while the stream is down (S-3:
    /// no strip; it only keeps the Connection lost / Starting lines quiet).
    static func hostPaused(_ model: AgentSessionModel) -> Bool {
        guard model.hostDeviceOffline else { return false }
        switch model.phase {
        case .starting, .connecting, .idle: return true
        case let .closed(_, reconnecting): return reconnecting
        default: return false
        }
    }

    /// `tasks` = false: the host passes THE Queue itself (#EXP-1545: a
    /// chat's ONE tasks card, `Dock.chatDockQueue`).
    static func sources(_ model: AgentSessionModel, session: SessionEntity, tasks: Bool = true) -> [Dock.Source] {
        var out: [Dock.Source] = []
        // #FEED-237 (dock.json): no `queued` source: the viewer's own pending
        // messages are bubbles at the transcript's bottom (message-delivery.json).
        if !model.isOver {
            if tasks {
                out.append(.tasks(model.taskQueue(now: Date()).map(Dock.QueueSummary.init)))
            }
        }
        let paused = hostPaused(model)
        let hasFeed = model.hasFeed
        let row = model.session ?? session
        // #NEW-49 (dock.json `watches`): what it waits on, each cancellable
        // in the card's sheet (`DockWatchesSheet`).
        if !model.isOver {
            let watching = SessionDock.watches(row)
            if !watching.isEmpty { out.append(.watches(watching)) }
        }
        let tree = TreeSession(sessionRow: row)
        // EXP-1374 (session-card.json `banner`): a background or queued
        // session's own card; #EXP-1703: a device that stayed away names
        // itself (the synced devices row's label), with Move.
        if let banner = SessionTree.sessionBanner(tree, deviceName: model.hostDevice.label) {
            out.append(.banner(
                id: banner.id, icon: banner.icon, label: banner.label, text: banner.text,
                tone: Dock.Tint(rawValue: banner.tone) ?? .muted, action: banner.action
            ))
        }
        if case let .closed(detail, _) = model.phase, !paused {
            out.append(.banner(
                id: "closed", icon: "ui-refresh", label: "Connection lost",
                text: detail ?? "Connection lost.", tone: .muted, action: "Reconnect"
            ))
        }
        let blocked = AgentUsagePresentation.parseBlocked(row.blocked)
        // S-3: an interruption (device_offline, restarted) has no label.
        if let label = AgentUsagePresentation.blockedBadgeLabel(blocked, now: model.activityNow) {
            out.append(.banner(
                id: "blocked", icon: "ui-usage", label: "Usage limit", text: label, tone: .warning,
                action: !model.sessionEnded && model.mayPost && AgentUsagePresentation.isUsageWall(blocked)
                    ? SessionAccountSwitch.wallSwitchLabel : nil
            ))
        }
        if let compacting = model.compacting, !model.isOver {
            out.append(.banner(
                id: "compacting", icon: "coding-compact", label: "Compacting",
                text: compacting.trigger == "manual"
                    ? "\(AgentFeed.compactingLabel) · requested" : AgentFeed.compactingLabel,
                tone: .muted, action: nil
            ))
        }
        if let limit = model.sessionRateLimit, !model.isOver,
           AgentFeed.rateLimitBannerShows(limit, now: model.activityNow) {
            let caption = AgentFeed.rateLimitCaption(limit, now: model.activityNow)
            out.append(.banner(
                id: "rate-limit", icon: "ui-usage", label: "Rate limit",
                text: "\(caption.message) · \(AgentFeed.rateLimitDetail(caption.countdown))",
                tone: .warning, action: nil
            ))
        }
        // #EXP-1405 / EXP-1487 (steer-enet.json `noFeed`): an idle row's
        // transcript waits for its next turn, not for a start: only a busy
        // run is "starting".
        if model.phase == .starting, !paused, hasFeed,
           SteerNoFeed.state(ended: model.sessionEnded, busy: row.busy) == .starting {
            out.append(.banner(
                id: "starting", icon: "ui-loading", label: "Starting",
                text: "The agent is starting. Waiting for the live stream…", tone: .muted, action: nil
            ))
        }
        return out
    }

    /// #NEW-49: a session's watches (`sessions.watching`) as the Dock reads them.
    static func watches(_ session: SessionEntity) -> [Dock.Watch] {
        SessionWatching.parseList(session.watching).map { Dock.Watch(id: $0.id, label: $0.label) }
    }

    /// #EXP-1383 (chats.json `agentHint`): the "Instructions changed" banner
    /// of an agent chat's ROOT that still runs on instructions older than the
    /// agent's current ones (`Chats.agentInstructionsChanged`); nil = no hint.
    /// The row stays until the fresh launch re-fetches the brief (the synced
    /// stamp hides it).
    static func agentHint(root: SessionEntity, agent: AgentDefinitionEntity?) -> Dock.Source? {
        guard root.isRoot, let agentId = root.agentId, let agent, agent.id == agentId,
              Chats.agentInstructionsChanged(
                  sessionStamp: root.agentInstructionsAt, agentStamp: agent.instructionsUpdatedAt
              )
        else { return nil }
        return .banner(
            id: Chats.AgentHint.id, icon: Chats.AgentHint.icon, label: Chats.AgentHint.label,
            text: Chats.AgentHint.text, tone: Dock.Tint(rawValue: Chats.AgentHint.tone) ?? .muted,
            action: Chats.AgentHint.action
        )
    }

    /// The hint's action word: sends `/new` exactly like the agent menu's
    /// New conversation (`send`).
    static func agentHintAction(_ row: Dock.Row, send: @escaping (String) -> Void) -> AnyView? {
        guard row.id == Chats.AgentHint.rowId, let action = row.action else { return nil }
        return AnyView(
            GlassPill(action, mode: .action { send(Chats.AgentHint.sends) })
                .accessibilityIdentifier("dock-instructions-new")
        )
    }

    /// THE Queue's sheet; nil for any other row.
    static func sheet(_ row: Dock.Row, model: AgentSessionModel, close: @escaping () -> Void) -> AnyView? {
        switch row.kind {
        case .tasks:
            return AnyView(DockSheetChrome(row: row, onClose: close) {
                TaskQueueView(
                    completedOpen: Bindable(model).queueCompletedOpen,
                    build: { model.taskQueue(now: $0) }
                )
            })
        default:
            return nil
        }
    }

    /// The closed stream's Reconnect (the other banners' actions are the
    /// host's: the usage wall's account switch).
    static func reconnect(_ model: AgentSessionModel) -> AnyView {
        AnyView(
            GlassPill("Reconnect", icon: AppIcons.uiRefresh, mode: .action { model.connect(force: true) })
                .accessibilityIdentifier("session-reconnect")
        )
    }
}

/// #NEW-49 (dock.json `watches`): THE watches card's sheet: one row per
/// watch (its label, trailing the ghost icon-xs `ui-close` whose label is
/// `Dock.Copy.cancelWatch`, for a viewer who may steer), hairline-divided
/// like the suggested tasks' rows. A tap cancels that watch
/// (`sessions.unwatch`); the last one gone takes the card and its sheet.
struct DockWatchesSheet: View {
    let row: Dock.Row
    let watches: [Dock.Watch]
    var pending: Set<String> = []
    /// nil = the viewer may not steer the session (no cancel glyph).
    var onCancel: ((String) -> Void)?
    let onClose: () -> Void

    var body: some View {
        DockSheetChrome(row: row, onClose: onClose) {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(watches.enumerated()), id: \.element.id) { index, watch in
                    if index > 0 { GlassDivider() }
                    watchRow(watch)
                }
            }
            .font(.caption)
            .foregroundStyle(.white)
            // The rows pad themselves (as the suggested tasks' sheet).
            .padding(.horizontal, -12)
            .accessibilityIdentifier(Dock.TestIds.watches)
        }
    }

    private func watchRow(_ watch: Dock.Watch) -> some View {
        HStack(spacing: 8) {
            Text(watch.label)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 8)
            if let onCancel {
                GhostIconButton(
                    AppIcons.uiClose, accessibilityLabel: Dock.Copy.cancelWatch, size: 24, glyphSize: 12,
                    tint: DesignTokens.Palette.mutedForeground, enabled: !pending.contains(watch.id),
                    action: { onCancel(watch.id) }
                )
                .accessibilityIdentifier("dock-watch-cancel")
            }
        }
        .padding(.horizontal, 12)
        .frame(minHeight: WorkCards.Geometry.row)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("dock-watch")
    }
}
