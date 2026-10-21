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
