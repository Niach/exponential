import ExpCore
import SwiftUI

// #FEED-191 NEEDS YOU ×iOS (fixtures/needs-you.json; web `packages/ui/src/needs-you.tsx`,
// Android `ui/chats/NeedsYou.kt`): the model is ExpCore's `NeedsYou.rows`; these
// only paint it.
//
//   NeedsYouEntryRow  the first row of Chats (the Archived row's box)
//   NeedsYouList      one reading column of THE inbox page's row look, else
//                     THE EmptyState

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
/// #EXP-1710: a stuck PR's row carries THE Button sm row under its text
/// (Retry, Merge now, Close PR); `onAction` runs a press.
public struct NeedsYouList: View {
    let rows: [NeedsYou.Row]
    let onOpen: (NeedsYou.Row) -> Void
    var onAction: ((NeedsYou.Row, MergeTrain.StuckRow.Action) -> Void)?

    public init(
        rows: [NeedsYou.Row],
        onOpen: @escaping (NeedsYou.Row) -> Void,
        onAction: ((NeedsYou.Row, MergeTrain.StuckRow.Action) -> Void)? = nil
    ) {
        self.rows = rows
        self.onOpen = onOpen
        self.onAction = onAction
    }

    public var body: some View {
        if rows.isEmpty {
            EmptyState(title: NeedsYou.Copy.empty, icon: AppIcons.navNeedsYou, size: .page)
                .frame(maxWidth: .infinity)
        } else {
            LazyVStack(spacing: 0) {
                ForEach(rows) { row in
                    if row.actions.isEmpty {
                        Button { onOpen(row) } label: { NeedsYouRowView(row: row) }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier(NeedsYou.TestIds.row)
                    } else {
                        VStack(alignment: .leading, spacing: 0) {
                            Button { onOpen(row) } label: { NeedsYouRowView(row: row) }
                                .buttonStyle(.plain)
                                .accessibilityIdentifier(NeedsYou.TestIds.row)
                            NeedsYouStuckActions(row: row) { onAction?(row, $0) }
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier(MergeTrain.TestIds.stuckRow)
                    }
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

/// #EXP-1710 (merge-train.json NEEDS YOU): THE Button sm row of a stuck PR,
/// OUTLINE for all three (web `variant="outline" size="sm"`), ONE wrapping
/// row under the meta line (web `flex flex-wrap gap-2`).
struct NeedsYouStuckActions: View {
    let row: NeedsYou.Row
    let onAction: (MergeTrain.StuckRow.Action) -> Void

    private func label(_ action: MergeTrain.StuckRow.Action) -> String {
        switch action {
        case .retry: MergeTrain.Copy.NeedsYou.retry
        case .mergeNow: MergeTrain.Copy.NeedsYou.mergeNow
        case .closePr: MergeTrain.Copy.NeedsYou.close
        }
    }

    var body: some View {
        FlowLayout(spacing: 8) {
            ForEach(row.actions, id: \.self) { action in
                Button { onAction(action) } label: {
                    GlassSubmitLabel(label(action), small: true, outline: true) { EmptyView() }
                        .contentShape(Rectangle())
                }
                .buttonStyle(.pressScale)
                .fixedSize()
                .accessibilityIdentifier("\(MergeTrain.TestIds.stuckRow)-\(action.rawValue)")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        // The text column: the row's padding + the glyph + its gap.
        .padding(.leading, 12 + 16 + 12)
        .padding(.trailing, 12)
        .padding(.bottom, 8)
    }
}
