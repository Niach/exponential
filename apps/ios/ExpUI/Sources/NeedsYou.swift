import ExpCore
import SwiftUI

// #FEED-191 NEEDS YOU ×iOS (fixtures/needs-you.json; web `packages/ui/src/needs-you.tsx`,
// Android `ui/chats/NeedsYou.kt`): the model is ExpCore's `NeedsYou.rows`; these
// only paint it.
//
//   NeedsYouEntryRow  the first row of Chats (the Archived row's box)
//   NeedsYouList      one reading column of THE inbox page's row look, else
//                     THE EmptyState

/// The entry's count Badge: the needs-you tone, 16 pt tall (web `Badge`).
public struct NeedsYouCountBadge: View {
    let count: Int

    public init(count: Int) {
        self.count = count
    }

    public var body: some View {
        Text("\(count)")
            .font(.system(size: 11, weight: .semibold))
            .monospacedDigit()
            .foregroundStyle(Color.black)
            .padding(.horizontal, 4)
            .frame(minWidth: 16, minHeight: 16)
            .background(DesignTokens.Semantic.yellow, in: Capsule())
            .fixedSize()
    }
}

/// THE ENTRY on phones: as tall as the Archived row, the glyph in the
/// needs-you tone centred in the lead disc's column (no disc), the title
/// `copy.entry`, the count Badge in the needs-you tone trailing.
public struct NeedsYouEntryRow: View {
    let count: Int

    public init(count: Int) {
        self.count = count
    }

    public var body: some View {
        ChatRowView(title: NeedsYou.Copy.entry, time: "", preview: nil, needsYouCount: count, leadDisc: false) {
            AppIcon(AppIcons.navNeedsYou, size: ChatListMetrics.archivedGlyph)
                .foregroundStyle(DesignTokens.Semantic.yellow)
        }
        .accessibilityIdentifier(NeedsYou.TestIds.entry)
    }
}

/// THE LIST: the leading glyph in the needs-you tone (waiting-cards.json
/// `icons`, a PR's `pr-open`), line 1 the title then the age muted trailing,
/// line 2 the chat then ` · meta`, muted. No row = THE EmptyState.
public struct NeedsYouList: View {
    let rows: [NeedsYou.Row]
    let onOpen: (NeedsYou.Row) -> Void

    public init(rows: [NeedsYou.Row], onOpen: @escaping (NeedsYou.Row) -> Void) {
        self.rows = rows
        self.onOpen = onOpen
    }

    public var body: some View {
        if rows.isEmpty {
            EmptyState(title: NeedsYou.Copy.empty, icon: AppIcons.navNeedsYou, size: .page)
                .frame(maxWidth: .infinity)
        } else {
            LazyVStack(spacing: 0) {
                ForEach(rows) { row in
                    Button { onOpen(row) } label: { NeedsYouRowView(row: row) }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier(NeedsYou.TestIds.row)
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(NeedsYou.TestIds.list)
        }
    }
}

/// One row (THE inbox page's flat row: `InboxListContent.streamRow`).
struct NeedsYouRowView: View {
    let row: NeedsYou.Row

    private var glyph: String {
        switch row.kind {
        case .plan: AppIcons.conceptOrName(WaitingCards.Icons.plan) ?? AppIcons.codingPlan
        case .question: AppIcons.conceptOrName(WaitingCards.Icons.question) ?? AppIcons.uiHelp
        case .pr: AppIcons.prOpen
        }
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            AppIcon(glyph, size: 16, weight: .medium)
                .foregroundStyle(DesignTokens.Semantic.yellow)
                .padding(.top, 2)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Text(row.title)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 4)
                    Text(row.age)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Palette.mutedForeground)
                        .fixedSize()
                }
                Text(row.meta.isEmpty ? row.chat : "\(row.chat) · \(row.meta)")
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Palette.mutedForeground)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .flatRow()
    }
}
