import ExpCore
import SwiftUI

// Session 58cb32df (chats.json, ×4: web `ChatRow`, Android `ChatRow.kt`): the phone Chats list's pieces. Content only: the host wraps
// a row in its own tap + long-press (THE menu's sheet) and hands the lead in
// (the run mark, a PR glyph), sized here to the 52pt muted disc.

public enum ChatListMetrics {
    /// The lead disc (`shell.leadDisc`).
    public static let leadDisc = CGFloat(Chats.Shell.leadDisc)
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
}

/// One flat chat row: [52 disc lead] [title · time / preview · word · badge],
/// the hairline under the text column only.
public struct ChatRowView<Lead: View>: View {
    private let title: String
    private let time: String
    /// nil = no preview line (#EXP-1388: the Archived entry row).
    private let preview: String?
    /// #FEED-191: a count Badge in the needs-you tone in the time slot (the
    /// Needs you entry row); nil = the time.
    private let needsYouCount: Int?
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
        needsYouCount: Int? = nil,
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
        self.needsYouCount = needsYouCount
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
                    if let needsYouCount {
                        CountBadge(count: needsYouCount, tone: .attention)
                    } else {
                        Text(time)
                            .font(.subheadline)
                            .monospacedDigit()
                            .foregroundStyle(timeColor)
                            .fixedSize()
                    }
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
                    CountBadge(NavBadges.rowBadge(unreadCount: unreadCount, needsYou: needsYou))
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

/// #FEED-191 (chats.json `copy.sections`): a header over one of the list's
/// sections (Agents, `chatSections`' Needs you · Working · Unread · Chats). A plain muted
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
