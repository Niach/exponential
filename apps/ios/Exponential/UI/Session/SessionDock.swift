import ExpCore
import ExpUI
import SwiftUI

/// N-12: a session's Dock beyond the asks (web `useSessionDock`): THE Queue
/// (task-queue.json), the queued messages (EXP-861, the X revokes a held
/// one) and the status banners (a dropped stream with its Reconnect, EXP-804
/// the usage wall with EXP-849's account switch, EXP-724 compacting, EXP-784
/// the rate-limit window, the starting line) as `Dock.Source`s with their
/// sheets (dock.json `banner`: the host's words). EXP-1374: a background or
/// queued session leads with its own card (`SessionTree.sessionBanner`,
/// session-card.json `banner`). S-3: no paused strip, no interrupted card
/// (an offline host is invisible). The session view and a thread's
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

    static func sources(_ model: AgentSessionModel, session: SessionEntity) -> [Dock.Source] {
        var out: [Dock.Source] = []
        if !model.isOver {
            out.append(.queued(model.queued.map(\.text)))
            out.append(.tasks(model.taskQueue(now: Date()).map(Dock.QueueSummary.init)))
        }
        let paused = hostPaused(model)
        let hasFeed = !model.feed.isEmpty
        let row = model.session ?? session
        let tree = TreeSession(sessionRow: row)
        // EXP-1374 (session-card.json `banner`): a background or queued
        // session's own card.
        if let banner = SessionTree.sessionBanner(tree) {
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

    /// THE Queue's and the queued messages' sheets; nil for any other row.
    static func sheet(_ row: Dock.Row, model: AgentSessionModel, close: @escaping () -> Void) -> AnyView? {
        switch row.kind {
        case .tasks:
            return AnyView(DockSheetChrome(row: row, onClose: close) {
                TaskQueueView(
                    todoOpen: Bindable(model).queueTodoOpen,
                    completedOpen: Bindable(model).queueCompletedOpen,
                    build: { model.taskQueue(now: $0) }
                )
            })
        case .queued:
            return AnyView(DockSheetChrome(row: row, onClose: close) {
                SessionQueuedList(model: model)
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

/// The queued messages' sheet body: the list with the remove glyphs (it
/// observes the model, so a revoked row leaves at once).
private struct SessionQueuedList: View {
    let model: AgentSessionModel

    var body: some View {
        AgentQueueStrip(messages: model.queued, plain: true) { id in
            model.unqueue(id)
        }
    }
}
