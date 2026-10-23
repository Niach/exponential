import ExpCore
import ExpUI
import SwiftUI

// EXP-1373 (chats.json): the Threads tab became the CHATS tab
// (`ChatsView`): one flat list of the active team's chats, no team or group
// bands, the agents as the header's bubbles.

/// An agent's lead: its curated icon in its colour, like a board row
/// (`BoardTypeDisplay`; an unknown name falls back to the default `bot`).
struct AgentRowIcon: View {
    let icon: String
    let color: String

    var body: some View {
        AppIcon(
            BoardTypeDisplay.iconName(for: icon) ?? SessionTree.ListCopy.agentDefaultIcon,
            size: AppIcon.Size.small
        )
        .foregroundStyle(
            Color(hex: color) ?? Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .white
        )
    }
}
