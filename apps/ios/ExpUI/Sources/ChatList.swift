import ExpCore
import SwiftUI

// Thread 58cb32df (chats.json, ×4: web `ChatRow` / `AgentBubbles`, Android
// `ChatRow.kt`): the phone Chats list's pieces. Content only: the host wraps
// a row in its own tap + long-press (THE menu's sheet) and hands the lead in
// (the run mark, a PR glyph), sized here to the 52pt muted disc.

public enum ChatListMetrics {
    /// The lead disc (`shell.leadDisc`).
    public static let leadDisc = CGFloat(Chats.Shell.leadDisc)
    /// An agent bubble's circle (`shell.bubble`) and its background gap
    /// inside the ring (`shell.bubbleRing`).
    public static let bubble = CGFloat(Chats.Shell.bubble)
    public static let bubbleRing = CGFloat(Chats.Shell.bubbleRing)
    /// Lead → text column.
    public static let gap: CGFloat = 14
    public static let rowVerticalPadding: CGFloat = 12
    /// Between two bubbles in the sideways row.
    public static let bubbleSpacing: CGFloat = 18
}

/// One flat chat row: [52 disc lead] [title · time / preview · word · badge],
/// the hairline under the text column only.
public struct ChatRowView<Lead: View>: View {
    private let title: String
    private let time: String
    private let preview: String
    /// The preview's leading label (`Question:` / `Plan:`) painted a step
    /// brighter; nil = none.
    private let previewLabel: String?
    private let previewItalic: Bool
    private let needsYou: Bool
    private let unreadCount: Int
    private let word: String?
    private let wordIcon: String?
    private let lead: Lead

    public init(
        title: String,
        time: String,
        preview: String,
        previewLabel: String? = nil,
        previewItalic: Bool = false,
        needsYou: Bool = false,
        unreadCount: Int = 0,
        word: String? = nil,
        wordIcon: String? = nil,
        @ViewBuilder lead: () -> Lead
    ) {
        self.title = title
        self.time = time
        self.preview = preview
        self.previewLabel = previewLabel
        self.previewItalic = previewItalic
        self.needsYou = needsYou
        self.unreadCount = unreadCount
        self.word = word
        self.wordIcon = wordIcon
        self.lead = lead()
    }

    private var unread: Bool { unreadCount > 0 }

    private var timeColor: Color {
        if needsYou { return DesignTokens.Semantic.yellow }
        return unread ? .white : .white.opacity(TextOpacity.tertiary)
    }

    public var body: some View {
        HStack(alignment: .center, spacing: ChatListMetrics.gap) {
            lead
                .frame(width: ChatListMetrics.leadDisc, height: ChatListMetrics.leadDisc)
                .background(DesignTokens.Palette.muted, in: Circle())
                .clipShape(Circle())
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(title)
                        .font(.body.weight(unread ? .semibold : .medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 4)
                    Text(time)
                        .font(.subheadline)
                        .monospacedDigit()
                        .foregroundStyle(timeColor)
                        .fixedSize()
                }
                HStack(alignment: .center, spacing: 8) {
                    previewText
                        .font(.subheadline)
                        .lineLimit(2)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    if let word {
                        HStack(spacing: 4) {
                            if let wordIcon {
                                AppIcon(wordIcon, size: 13)
                            }
                            Text(word)
                                .font(.subheadline)
                        }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .fixedSize()
                    }
                    if unread {
                        Text("\(unreadCount)")
                            .font(.caption.weight(.semibold))
                            .monospacedDigit()
                            .foregroundStyle(needsYou ? Color.black : DesignTokens.Palette.background)
                            .padding(.horizontal, 6)
                            .frame(minWidth: 22, minHeight: 22)
                            .background(needsYou ? DesignTokens.Semantic.yellow : .white, in: Capsule())
                            .fixedSize()
                    }
                }
            }
            .padding(.vertical, ChatListMetrics.rowVerticalPadding)
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    private var previewText: Text {
        let muted = Color.white.opacity(TextOpacity.secondary)
        if let previewLabel, preview.hasPrefix(previewLabel) {
            let rest = String(preview.dropFirst(previewLabel.count))
            return Text(previewLabel).foregroundStyle(.white.opacity(0.85)).fontWeight(.medium)
                + Text(rest).foregroundStyle(muted)
        }
        return Text(preview).italic(previewItalic).foregroundStyle(muted)
    }
}

/// One agent of the Chats header's bubble row.
public struct AgentBubbleItem: Identifiable, Equatable {
    public enum Ring: Equatable { case needsYou, unread, none }
    public let id: String
    public let name: String
    /// A registry glyph (`AppIcons`), resolved by the host.
    public let icon: String
    public let color: Color
    public let ring: Ring

    public init(id: String, name: String, icon: String, color: Color, ring: Ring) {
        self.id = id
        self.name = name
        self.icon = icon
        self.color = color
        self.ring = ring
    }
}

/// The Chats header's agents (`agentBubbles`): a 66pt circle each — the
/// icon in its colour on muted inside a 3pt background gap, the ring amber
/// (needs you), foreground (unread) or a hairline — the name under it. The
/// row scrolls sideways; a tap opens the agent's chat, a hold its menu.
public struct AgentBubbles: View {
    let items: [AgentBubbleItem]
    let onTap: (String) -> Void
    let onHold: (String) -> Void

    public init(items: [AgentBubbleItem], onTap: @escaping (String) -> Void, onHold: @escaping (String) -> Void) {
        self.items = items
        self.onTap = onTap
        self.onHold = onHold
    }

    public var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: ChatListMetrics.bubbleSpacing) {
                ForEach(items) { item in
                    bubble(item)
                        .glassLongPress(onTap: { onTap(item.id) }, onHold: { onHold(item.id) })
                        .accessibilityLabel(item.name)
                        .accessibilityIdentifier(Chats.TestIds.agentBubble)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 2)
        }
    }

    private func bubble(_ item: AgentBubbleItem) -> some View {
        let size = ChatListMetrics.bubble
        let inner = size - 2 * ChatListMetrics.bubbleRing - 4
        return VStack(spacing: 6) {
            ZStack {
                switch item.ring {
                case .needsYou:
                    Circle().stroke(DesignTokens.Semantic.yellow, lineWidth: 2.5)
                case .unread:
                    Circle().stroke(Color.white, lineWidth: 2.5)
                case .none:
                    Circle().stroke(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline * 2)
                }
                Circle()
                    .fill(DesignTokens.Palette.muted)
                    .frame(width: inner, height: inner)
                AppIcon(item.icon, size: 26, weight: .medium)
                    .foregroundStyle(item.color)
            }
            .frame(width: size, height: size)
            Text(item.name)
                .font(.subheadline.weight(item.ring == .none ? .regular : .medium))
                .foregroundStyle(.white.opacity(item.ring == .none ? TextOpacity.secondary : 1))
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: size + 12)
        }
        .contentShape(Rectangle())
    }
}

/// The Chats header's trailing circle (search, the inbox with its dot).
public struct ChatsHeaderCircle: View {
    let icon: String
    let dot: Bool
    let label: String
    let action: () -> Void

    public init(icon: String, dot: Bool = false, label: String, action: @escaping () -> Void) {
        self.icon = icon
        self.dot = dot
        self.label = label
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            AppIcon(icon, size: AppIcon.Size.large)
                .foregroundStyle(.white)
                .frame(width: 44, height: 44)
                .background(GlassTokens.opaqueCardFill, in: Circle())
                .overlay(Circle().stroke(GlassTokens.strokeStrong, lineWidth: GlassTokens.hairline))
                .overlay(alignment: .topTrailing) {
                    if dot {
                        Circle()
                            .fill(DesignTokens.Semantic.blue)
                            .frame(width: 10, height: 10)
                            .offset(x: -3, y: 3)
                    }
                }
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

public extension ToolbarContent {
    /// EXP-1373: a toolbar item that paints its OWN chrome (the board
    /// switcher pill, the chat header's title block, the Chats header
    /// circles) opts out of iOS 26's shared Liquid Glass item background.
    @ToolbarContentBuilder
    func ownChrome() -> some ToolbarContent {
        if #available(iOS 26.0, *) {
            sharedBackgroundVisibility(.hidden)
        } else {
            self
        }
    }
}
