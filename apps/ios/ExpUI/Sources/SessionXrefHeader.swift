import ExpCore
import SwiftUI

/// S-3 inter-session links (session-thread.json, styleguide `session-xref`):
/// the compact header naming ANOTHER session — the muted lead
/// (`copy.xref[kind]`), the agent's mark (its icon in its colour, else the
/// session glyph), the session's name and a muted `›`, one caption line,
/// 4pt gaps. The whole line opens that session (the host pushes it);
/// #EXP-1464: nil `onOpen` = a plain line, no `›`.
/// Named `…Header` beside the ExpCore `SessionXref` model.
public struct SessionXrefHeader: View {
    /// The agent's mark, resolved by the host (nil = the session glyph,
    /// `coding-assistant`, also for an agent not synced yet).
    public struct AgentMark: Equatable {
        public let icon: String
        public let color: Color

        public init(icon: String, color: Color) {
            self.icon = icon
            self.color = color
        }
    }

    let xref: SessionXref
    let agent: AgentMark?
    let onOpen: (() -> Void)?

    public init(xref: SessionXref, agent: AgentMark?, onOpen: (() -> Void)?) {
        self.xref = xref
        self.agent = agent
        self.onOpen = onOpen
    }

    public var body: some View {
        Group {
            if let onOpen {
                Button(action: onOpen) { line(chevron: true).contentShape(Rectangle()) }
                    .buttonStyle(PressableRowStyle())
            } else {
                line(chevron: false)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(SessionTree.ThreadCopy.xrefTestId)
    }

    private func line(chevron: Bool) -> some View {
        let muted = Color.white.opacity(TextOpacity.tertiary)
        return HStack(spacing: 4) {
            Text(xref.lead)
                .foregroundStyle(muted)
                .lineLimit(1)
                .fixedSize()
            if let agent {
                AppIcon(agent.icon, size: 16).foregroundStyle(agent.color)
            } else {
                AppIcon(AppIcons.codingAssistant, size: 16).foregroundStyle(muted)
            }
            Text(xref.name)
                .fontWeight(.medium)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
            if chevron {
                AppIcon(AppIcons.uiChevronRight, size: 12).foregroundStyle(muted)
            }
        }
        .font(.caption)
    }
}
