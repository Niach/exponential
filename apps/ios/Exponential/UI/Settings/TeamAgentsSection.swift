import ExpUI
import ExpCore
import SwiftUI

/// EXP-1291: Team settings → Agents, managed exactly like boards — the
/// filled group band over flat rows (`TeamBoardsSection`), one row per agent
/// DEFINITION (NEW-2; its glyph tinted with its colour + its name). A row opens the agent
/// page (`AgentSettingsPage`); members open it read-only. Creating one is the
/// owner's, like a board.
struct TeamAgentsSection: View {
    /// The team's agent definitions (`SessionQueries.agents`), in their sort
    /// order.
    let agents: [AgentDefinitionEntity]
    let isOwner: Bool
    /// nil = a new agent.
    let onOpen: (String?) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(AgentSettings.Copy.title) {
                if isOwner {
                    GlassPill(AgentSettings.Copy.newAgent, icon: AppIcons.uiAdd, mode: .action {
                        onOpen(nil)
                    })
                    .accessibilityIdentifier("new-agent")
                }
            }

            ForEach(agents) { agent in
                Button {
                    onOpen(agent.id)
                } label: {
                    HStack(spacing: 10) {
                        AppIcon(Self.iconName(agent), size: 16)
                            .foregroundStyle(Self.tint(agent))
                        Text(agent.name)
                            .font(.subheadline)
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Spacer(minLength: 8)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .flatRow()
                .accessibilityIdentifier("agent-settings-row")
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(AgentSettings.TestIds.list)
    }

    /// The agent's curated glyph, else the contract default (`bot`).
    static func iconName(_ agent: AgentDefinitionEntity) -> String {
        BoardTypeDisplay.iconName(for: agent.icon) ?? SessionTree.ListCopy.agentDefaultIcon
    }

    static func tint(_ agent: AgentDefinitionEntity) -> Color {
        Color(hex: agent.color ?? SessionTree.ListCopy.agentDefaultColor) ?? .gray
    }
}

/// NEW-2: the team's Events band under the agents (web `AgentEventsBand`):
/// the event stream newest first, capped at `cap`, as flat rows — the
/// trigger-event glyph, the event's name, who emitted it (the session's
/// title, the person's name, or Exponential) and the compact age.
struct TeamEventsSection: View {
    let events: [EventEntity]
    /// An emitting session's title, by id.
    let sessionTitles: [String: String]
    let users: [UserEntity]

    static let title = "Events"
    /// Web `AGENT_EVENTS_CAP`.
    nonisolated static let cap = 50

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(Self.title)
            if events.isEmpty {
                EmptyState(title: "No events yet", icon: AppIcons.triggerEvent, size: .list)
                    .padding(.vertical, 8)
            }
            TimelineView(.periodic(from: .now, by: 60)) { context in
                VStack(spacing: 0) {
                    ForEach(events) { event in
                        row(event, now: context.date)
                    }
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("agent-events")
    }

    private func row(_ event: EventEntity, now: Date) -> some View {
        HStack(spacing: 10) {
            AppIcon(AppIcons.triggerEvent, size: 16)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text(event.name)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white)
                .lineLimit(1)
                .layoutPriority(1)
            Text(emitter(event))
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(RelativeTime.compact(wire: event.createdAt, now: now))
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("agent-event-\(event.id)")
    }

    /// Who emitted it (web `eventEmitterLabel`).
    private func emitter(_ event: EventEntity) -> String {
        switch event.emittedBy {
        case "session":
            let title = event.sessionId.flatMap { sessionTitles[$0] } ?? ""
            return title.isEmpty ? "Session" : title
        case "user":
            guard let userId = event.userId else { return "Someone" }
            let user = users.first { $0.id == userId }
            return user?.name ?? user?.email ?? "Someone"
        default:
            return "Exponential"
        }
    }
}
