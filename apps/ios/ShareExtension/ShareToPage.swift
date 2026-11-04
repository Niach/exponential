#if !SHARE_EXTENSION
import ExpCore
import ExpUI
#endif
import SwiftUI
import UIKit

/// Where a Share to row leads.
enum ShareRoute: Hashable {
    case newIssue
    /// THE composer, seeded on a pick (`ComposerAgentRow.temporaryPick` for
    /// New chat, else an agent id).
    case compose(pick: String)
    /// A direct share into an existing chat.
    case chat(id: String)
}

/// A team a share can land in (one per account + team).
struct ShareTeam: Identifiable, Hashable {
    let accountId: String
    let teamId: String
    let name: String

    var id: String { "\(accountId):\(teamId)" }
}

/// EXP-1635 (share-targets.json), STEP 1: the preview line, the two fixed
/// rows, the agents. The extension's own Form rows (it draws no app chrome);
/// the host pushes the `ShareRoute`s.
struct ShareToPage: View {
    let teams: [ShareTeam]
    @Binding var teamKey: String?
    let agents: [MirroredAgent]
    let images: [SharedImage]

    var body: some View {
        Form {
            if !images.isEmpty {
                Section { preview }
                    .listRowBackground(GlassTokens.opaqueCardFill)
            }
            if teams.count > 1 {
                Section {
                    // The share flow's own selector (EXP-60), the system
                    // `Picker` of the board row.
                    Picker("Team", selection: $teamKey) {
                        ForEach(teams) { team in
                            Text(team.name).tag(Optional(team.id))
                        }
                    }
                }
                .listRowBackground(GlassTokens.opaqueCardFill)
            }
            Section {
                row(id: ShareTargets.Ids.newIssue, title: ShareTargets.Copy.newIssue, subtitle: nil, route: .newIssue) {
                    AppIcon(AppIcons.conceptOrName(ShareTargets.Icons.newIssue) ?? AppIcons.uiAdd, size: 20)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                row(
                    id: ShareTargets.Ids.newChat,
                    title: ShareTargets.Copy.newChat,
                    subtitle: ShareTargets.Copy.newChatSubtitle,
                    route: .compose(pick: ComposerAgentRow.temporaryPick)
                ) {
                    AppIcon(AppIcons.conceptOrName(ShareTargets.Icons.newChat) ?? AppIcons.uiAdd, size: 20)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
            .listRowBackground(GlassTokens.opaqueCardFill)
            if !agents.isEmpty {
                Section(ShareTargets.Copy.agents) {
                    ForEach(agents) { agent in
                        row(id: agent.id, title: agent.name, subtitle: nil, route: .compose(pick: agent.id)) {
                            AgentBubbleCircle(
                                glyph: agent.bubbleGlyph,
                                size: Self.bubble,
                                ringGap: 2,
                                ringColor: GlassTokens.strokeCard,
                                ringWidth: GlassTokens.hairline,
                                ringSlot: 1,
                                glyphSize: Self.bubble / 2
                            )
                        }
                    }
                }
                .listRowBackground(GlassTokens.opaqueCardFill)
            }
        }
        .scrollContentBackground(.hidden)
        .background(DesignTokens.Palette.background)
        .accessibilityIdentifier(ShareTargets.TestIds.page)
    }

    private static let bubble: CGFloat = 32

    private var preview: some View {
        HStack(spacing: 8) {
            ForEach(Array(images.prefix(4).enumerated()), id: \.offset) { _, image in
                if let uiImage = UIImage(data: image.data) {
                    Image(uiImage: uiImage)
                        .resizable()
                        .scaledToFill()
                        .frame(width: 40, height: 40)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                }
            }
            Text(ShareTargets.Copy.images(images.count))
                .font(.footnote)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer(minLength: 0)
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier(ShareTargets.TestIds.preview)
    }

    private func row<Lead: View>(
        id: String, title: String, subtitle: String?, route: ShareRoute, @ViewBuilder lead: () -> Lead
    ) -> some View {
        NavigationLink(value: route) {
            HStack(spacing: 12) {
                lead().frame(width: Self.bubble, height: Self.bubble)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .foregroundStyle(.white)
                    if let subtitle {
                        Text(subtitle)
                            .font(.footnote)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
            }
        }
        .accessibilityIdentifier(ShareTargets.TestIds.row(id))
    }
}

extension MirroredAgent {
    /// The chats.json agent bubble: the viewer's Assistant wears the brand
    /// mark, every other agent its glyph in its colour.
    var bubbleGlyph: AgentBubbleCircle.Glyph {
        AgentOrdering.isAssistant(role: role) ? .brand : .icon(glyph, Color(hex: color) ?? .white)
    }
}
