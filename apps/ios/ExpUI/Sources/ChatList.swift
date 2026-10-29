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
    /// #EXP-1388 touch selection: the select badge (`shell.selectBadge`),
    /// its ground ring (`shell.selectBadgeRing`) and check
    /// (`shell.selectBadgeGlyph`); the Archived entry's plain glyph
    /// (`shell.archivedGlyph`).
    public static let selectBadge = CGFloat(Chats.Shell.selectBadge)
    public static let selectBadgeRing = CGFloat(Chats.Shell.selectBadgeRing)
    public static let selectBadgeGlyph = CGFloat(Chats.Shell.selectBadgeGlyph)
    public static let selectBadgeRadius = CGFloat(Chats.Shell.selectBadgeRadius)
    public static let archivedGlyph = CGFloat(Chats.Shell.archivedGlyph)
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
    /// nil = no preview line (#EXP-1388: the Archived entry row).
    private let preview: String?
    /// The preview's leading label (`Question:` / `Plan:`) painted a step
    /// brighter; nil = none.
    private let previewLabel: String?
    private let previewItalic: Bool
    private let needsYou: Bool
    private let unreadCount: Int
    private let word: String?
    private let wordIcon: String?
    /// #EXP-1388: the row is selected (touch: the stronger disc + the
    /// select badge; the host paints the full-width tint).
    private let selected: Bool
    /// false = the lead is a plain glyph centred in the disc column, no
    /// disc, the row as tall as its text (the Archived entry).
    private let leadDisc: Bool
    private let lead: Lead

    public init(
        title: String,
        time: String,
        preview: String?,
        previewLabel: String? = nil,
        previewItalic: Bool = false,
        needsYou: Bool = false,
        unreadCount: Int = 0,
        word: String? = nil,
        wordIcon: String? = nil,
        selected: Bool = false,
        leadDisc: Bool = true,
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
        self.selected = selected
        self.leadDisc = leadDisc
        self.lead = lead()
    }

    private var unread: Bool { unreadCount > 0 }

    private var timeColor: Color {
        if needsYou { return DesignTokens.Semantic.yellow }
        return unread ? .white : .white.opacity(TextOpacity.tertiary)
    }

    public var body: some View {
        HStack(alignment: .center, spacing: ChatListMetrics.gap) {
            leadCell
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
                if let preview {
                HStack(alignment: .center, spacing: 8) {
                    previewText(preview)
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
            }
            .padding(.vertical, ChatListMetrics.rowVerticalPadding)
            .overlay(alignment: .bottom) {
                // A selected row's band has no line inside it (the
                // hairline keeps its height, nothing reflows).
                Rectangle()
                    .fill(selected ? Color.clear : GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var leadCell: some View {
        if leadDisc {
            lead
                .frame(width: ChatListMetrics.leadDisc, height: ChatListMetrics.leadDisc)
                .background {
                    // A selected disc steps one grey notch up (fillActive
                    // over muted) so it never blends into the row's tint.
                    Circle().fill(DesignTokens.Palette.muted)
                        .overlay { if selected { Circle().fill(GlassTokens.fillActive) } }
                }
                .clipShape(Circle())
                .overlay(alignment: .bottomTrailing) {
                    if selected { ChatSelectBadge() }
                }
        } else {
            lead.frame(width: ChatListMetrics.leadDisc)
        }
    }

    private func previewText(_ preview: String) -> Text {
        let muted = Color.white.opacity(TextOpacity.secondary)
        if let previewLabel, preview.hasPrefix(previewLabel) {
            let rest = String(preview.dropFirst(previewLabel.count))
            return Text(previewLabel).foregroundStyle(.white.opacity(0.85)).fontWeight(.medium)
                + Text(rest).foregroundStyle(muted)
        }
        return Text(preview).italic(previewItalic).foregroundStyle(muted)
    }
}

/// #EXP-1388 touch selection (WhatsApp's reference): the select badge on a
/// selected row's disc = a SQUARE in THE checkbox look (`selectBadgeRadius`
/// corner, primary fill, the Checkbox's `ui-check` at `selectBadgeGlyph` in
/// primary-foreground, its normal stroke), ringed in the selected row's
/// ground (the screen background under the fillActive tint; outer corner =
/// radius + ring) so it reads cut out of the disc; its outer box sits flush
/// with the disc's bottom-right corner.
public struct ChatSelectBadge: View {
    public init() {}

    public var body: some View {
        let ring = ChatListMetrics.selectBadgeRing
        let badge = ChatListMetrics.selectBadge
        let radius = ChatListMetrics.selectBadgeRadius
        let outer = RoundedRectangle(cornerRadius: radius + ring, style: .continuous)
        ZStack {
            outer.fill(DesignTokens.Palette.background)
            outer.fill(GlassTokens.fillActive)
            RoundedRectangle(cornerRadius: radius, style: .continuous)
                .fill(DesignTokens.Palette.primary)
                .frame(width: badge, height: badge)
            AppIcon(AppIcons.uiCheck, size: ChatListMetrics.selectBadgeGlyph)
                .foregroundStyle(DesignTokens.Palette.primaryForeground)
        }
        .frame(width: badge + 2 * ring, height: badge + 2 * ring)
        .accessibilityHidden(true)
    }
}

/// #EXP-1388 THE ARCHIVED ENTRY: the same chat row with the PLAIN
/// `ui-archive` glyph (no disc) centred in the disc column, the title
/// `copy.archived.entry` and the archived count in the trailing time slot
/// (muted), no preview line; the row as tall as its text line.
public struct ChatArchivedRow: View {
    let count: Int

    public init(count: Int) {
        self.count = count
    }

    public var body: some View {
        ChatRowView(title: Chats.Copy.Archived.entry, time: "\(count)", preview: nil, leadDisc: false) {
            AppIcon(AppIcons.uiArchive, size: ChatListMetrics.archivedGlyph)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
        .accessibilityIdentifier(Chats.TestIds.chatArchived)
    }
}

/// S-3 (chats.json `copy.sections`): the Agents / Chats header over the
/// list's two sections, only while an agent row exists. A plain muted
/// caption (web `ChatSectionHeader` `pt-2 pb-1 text-xs font-medium
/// text-muted-foreground`, Android `ChatSectionHeader`), never the filled
/// Form band (`GlassSectionHeader`).
public struct ChatSectionHeader: View {
    let title: String

    public init(_ title: String) {
        self.title = title
    }

    public var body: some View {
        Text(title)
            .font(.system(size: DesignTokens.Typography.Size.xs, weight: .medium))
            .foregroundStyle(DesignTokens.Palette.mutedForeground)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 8)
            .padding(.bottom, 4)
            .accessibilityAddTraits(.isHeader)
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
    /// S-3: the viewer's Assistant wears the brand mark, never its stored
    /// icon/colour.
    public let assistant: Bool

    public init(id: String, name: String, icon: String, color: Color, ring: Ring, assistant: Bool = false) {
        self.id = id
        self.name = name
        self.icon = icon
        self.color = color
        self.ring = ring
        self.assistant = assistant
    }

    public var glyph: AgentBubbleCircle.Glyph {
        assistant ? .brand : .icon(icon, color)
    }
}

/// THE agent bubble's circle (chats.json `shell.bubble`), shared by the Chats
/// header's bubbles (66pt) and S-3's composer agent row (composer-home.json
/// `agentRow`, 40pt): the glyph on a bg-muted disc inside a `ringGap`
/// background gap, then the ring.
public struct AgentBubbleCircle: View {
    public enum Glyph: Equatable {
        /// A registry glyph in its colour.
        case icon(String, Color)
        /// S-3: the Assistant's avatar = the Exponential brand mark in the
        /// foreground colour.
        case brand
        /// S-3: the composer row's Temporary session = the concept glyph
        /// (`ui-icon-placeholder`) in the muted foreground.
        case temporary(String)
    }

    let glyph: Glyph
    let size: CGFloat
    let ringGap: CGFloat
    let ringColor: Color
    let ringWidth: CGFloat
    /// The ring's slot outside the gap (the widest ring the circle wears),
    /// so a ring change never resizes the disc.
    let ringSlot: CGFloat

    public init(
        glyph: Glyph,
        size: CGFloat = ChatListMetrics.bubble,
        ringGap: CGFloat = ChatListMetrics.bubbleRing,
        ringColor: Color,
        ringWidth: CGFloat,
        ringSlot: CGFloat = 2
    ) {
        self.glyph = glyph
        self.size = size
        self.ringGap = ringGap
        self.ringColor = ringColor
        self.ringWidth = ringWidth
        self.ringSlot = ringSlot
    }

    private var inner: CGFloat { size - 2 * ringGap - 2 * ringSlot }
    /// 26pt in the 66pt bubble, the same share at any size.
    private var glyphSize: CGFloat { (inner * 26 / 56).rounded() }

    public var body: some View {
        ZStack {
            Circle().stroke(ringColor, lineWidth: ringWidth)
            Circle()
                .fill(DesignTokens.Palette.muted)
                .frame(width: inner, height: inner)
            AgentBubbleGlyph(glyph: glyph, size: glyphSize)
        }
        .frame(width: size, height: size)
    }
}

/// The bubble's glyph alone (`AgentBubbleCircle.Glyph` at `size`): the
/// circle's centre, and S-3's story-ring bubble inside THE StoryRing.
struct AgentBubbleGlyph: View {
    let glyph: AgentBubbleCircle.Glyph
    let size: CGFloat

    var body: some View {
        switch glyph {
        case let .icon(name, color):
            AppIcon(name, size: size, weight: .medium)
                .foregroundStyle(color)
        case .brand:
            // The static logo (FeedbackMark's still frame) without the
            // feedback mark's test id.
            MarkCanvas(color: .white, spans: FeedbackStates.Mark.staticSpans)
                .frame(width: size, height: size)
        case let .temporary(name):
            AppIcon(name, size: size, weight: .medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }
}

/// The Chats header's agents (`agentBubbles`): a 66pt circle each — the
/// icon in its colour on muted inside a 3pt background gap, the ring amber
/// (needs you), foreground (unread) or a hairline — the name under it. The
/// row scrolls sideways; a tap opens the agent's chat, a hold its menu.
/// S-3: the viewer's Assistant first, with the brand mark. S-3 (stories.json
/// STORY RING): a bubble whose agent has a story ring wears THE StoryRing
/// instead and a tap opens the story viewer at it (`onOpenStory`); the rings
/// no bubble wears follow in the same row (`Stories.bubbleRings`).
public struct AgentBubbles<RingAvatar: View>: View {
    let items: [AgentBubbleItem]
    /// The story rings (`Stories.rings`, ring order).
    let rings: [Stories.StoryRing]
    let onTap: (String) -> Void
    let onHold: (String) -> Void
    let onOpenStory: (String) -> Void
    /// A trailing ring's avatar (a root ring's brand mark), by the host.
    let ringAvatar: (Stories.StoryRing) -> RingAvatar

    public init(
        items: [AgentBubbleItem],
        rings: [Stories.StoryRing],
        onTap: @escaping (String) -> Void,
        onHold: @escaping (String) -> Void,
        onOpenStory: @escaping (String) -> Void,
        @ViewBuilder ringAvatar: @escaping (Stories.StoryRing) -> RingAvatar
    ) {
        self.items = items
        self.rings = rings
        self.onTap = onTap
        self.onHold = onHold
        self.onOpenStory = onOpenStory
        self.ringAvatar = ringAvatar
    }

    public var body: some View {
        let split = Stories.bubbleRings(items.map(\.id), rings)
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .top, spacing: ChatListMetrics.bubbleSpacing) {
                ForEach(items) { item in
                    bubbleButton(item, story: split.byAgent[item.id])
                }
                ForEach(split.rest) { ring in
                    StoryRingButton(ring: ring, onOpen: onOpenStory) { ringAvatar(ring) }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 2)
        }
    }

    private func bubbleButton(_ item: AgentBubbleItem, story: Stories.StoryRing?) -> some View {
        bubble(item, story: story)
            .glassLongPress(
                onTap: {
                    if let story { onOpenStory(story.key) } else { onTap(item.id) }
                },
                onHold: { onHold(item.id) }
            )
            .accessibilityLabel(item.name)
            .accessibilityIdentifier(Chats.TestIds.agentBubble)
    }

    private func bubble(_ item: AgentBubbleItem, story: Stories.StoryRing?) -> some View {
        let size = ChatListMetrics.bubble
        let ring: (Color, CGFloat) = switch item.ring {
        case .needsYou: (DesignTokens.Semantic.yellow, 2.5)
        case .unread: (Color.white, 2.5)
        case .none: (GlassTokens.strokeCard, GlassTokens.hairline * 2)
        }
        let lit = item.ring != .none || story?.unseen == true
        return VStack(spacing: 6) {
            if let story {
                // The ring's 54pt inner disc, the bubble's glyph at its 26pt.
                StoryRing(stories: story.stories, size: size) {
                    ZStack {
                        Circle().fill(DesignTokens.Palette.muted)
                        AgentBubbleGlyph(glyph: item.glyph, size: 26)
                    }
                }
            } else {
                AgentBubbleCircle(glyph: item.glyph, ringColor: ring.0, ringWidth: ring.1)
            }
            Text(item.name)
                .font(.subheadline.weight(lit ? .medium : .regular))
                .foregroundStyle(.white.opacity(lit ? 1 : TextOpacity.secondary))
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: size + 12)
        }
        .contentShape(Rectangle())
    }
}

public extension AgentBubbles where RingAvatar == EmptyView {
    /// No story rings: every bubble wears its own ring and opens its chat.
    init(items: [AgentBubbleItem], onTap: @escaping (String) -> Void, onHold: @escaping (String) -> Void) {
        self.init(items: items, rings: [], onTap: onTap, onHold: onHold, onOpenStory: { _ in }) { _ in EmptyView() }
    }
}

public extension View {
    /// EXP-1373: a toolbar item that paints its OWN chrome (the chat
    /// header's title block) opts out of iOS 26's shared Liquid Glass item background.
    /// The availability check lives in a VIEW builder, never inside a
    /// `ToolbarContentBuilder`: the toolbar builder's limited-availability
    /// symbol does not exist below iOS 26 and the app links against 17.4.
    /// `enabled: false` adds nothing (the caller's other toolbar owns the slot).
    func ownChromeToolbarItem<Item: View>(
        placement: ToolbarItemPlacement,
        enabled: Bool = true,
        @ViewBuilder item: @escaping () -> Item
    ) -> some View {
        modifier(OwnChromeToolbarItem(placement: placement, enabled: enabled, item: item))
    }
}

private struct OwnChromeToolbarItem<Item: View>: ViewModifier {
    let placement: ToolbarItemPlacement
    let enabled: Bool
    let item: () -> Item

    func body(content: Content) -> some View {
        if !enabled {
            content
        } else if #available(iOS 26.0, *) {
            content.toolbar {
                ToolbarItem(placement: placement) { item() }
                    .sharedBackgroundVisibility(.hidden)
            }
        } else {
            content.toolbar {
                ToolbarItem(placement: placement) { item() }
            }
        }
    }
}
