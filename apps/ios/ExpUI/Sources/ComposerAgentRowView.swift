import ExpCore
import SwiftUI

/// S-3 (composer-home.json `agentRow`, ×4: web `ComposerAgentRow`, desktop
/// `composer_agent_row`, Android `ComposerAgentRow.kt`): THE AGENT ROW over
/// the New-chat composer card. The label line (`copy.to` / `copy.temporary`,
/// muted small text) `labelToRow` over the circles; the circles = THE chats
/// agent bubble (`AgentBubbleCircle`) at `size`, single select: the selected
/// one wears the foreground ring (`ringWidth`), the others the glass-stroke
/// hairline; no name under a circle (the name is the label and the
/// accessibility label). The row scrolls sideways when it overflows. The
/// host owns the pick (`ComposerAgentRow.composerAgentRow`) and stands the
/// row `rowToCard` over the card.
public struct ComposerAgentRowView: View {
    public struct Item: Identifiable, Equatable {
        public let id: String
        public let name: String
        public let glyph: AgentBubbleCircle.Glyph

        public init(id: String, name: String, glyph: AgentBubbleCircle.Glyph) {
            self.id = id
            self.name = name
            self.glyph = glyph
        }

        /// The dotted circle: Temporary session.
        public static var temporary: Item {
            Item(
                id: ComposerAgentRow.temporaryPick,
                name: ComposerAgentRow.Copy.temporary,
                glyph: .temporary(AppIcons.conceptOrName(ComposerAgentRow.temporaryIcon) ?? AppIcons.uiIconPlaceholder)
            )
        }
    }

    let label: String
    let items: [Item]
    let selected: String
    let onSelect: (String) -> Void

    public init(label: String, items: [Item], selected: String, onSelect: @escaping (String) -> Void) {
        self.label = label
        self.items = items
        self.selected = selected
        self.onSelect = onSelect
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: CGFloat(ComposerAgentRow.labelToRow)) {
            Text(label)
                .font(.footnote)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
                .padding(.horizontal, 4)
                .accessibilityIdentifier(ComposerAgentRow.TestIds.label)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: CGFloat(ComposerAgentRow.gap)) {
                    ForEach(items) { item in
                        circle(item)
                    }
                }
                // The selected ring straddles the circle's edge.
                .padding(2)
            }
            .padding(-2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(ComposerAgentRow.TestIds.row)
    }

    private func circle(_ item: Item) -> some View {
        let isSelected = item.id == selected
        return Button {
            // Tapping the selected circle keeps it: the row is never empty.
            if !isSelected { onSelect(item.id) }
        } label: {
            AgentBubbleCircle(
                glyph: item.glyph,
                size: CGFloat(ComposerAgentRow.size),
                ringGap: CGFloat(ComposerAgentRow.ringGap),
                ringColor: isSelected ? .white : GlassTokens.strokeCard,
                ringWidth: isSelected ? CGFloat(ComposerAgentRow.ringWidth) : GlassTokens.hairline,
                ringSlot: CGFloat(ComposerAgentRow.ringWidth)
            )
            .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.name)
        .accessibilityAddTraits(isSelected ? .isSelected : [])
        .accessibilityIdentifier(ComposerAgentRow.TestIds.option(item.id))
        .sensoryFeedback(.selection, trigger: isSelected) { _, now in now }
    }
}

#Preview {
    ComposerAgentRowView(
        label: "To Assistant",
        items: [
            .init(id: "as", name: "Assistant", glyph: .brand),
            .init(id: "g-a", name: "Designer", glyph: .icon(AppIcons.actionChat, .purple)),
            .temporary,
        ],
        selected: "as",
        onSelect: { _ in }
    )
    .padding()
    .background(Color.black)
}
