import ExpCore
import ExpUI
import SwiftUI

/// EXP-878 — the Drafts segment of My Work: every unfiled issue this user has
/// composed, across teams. EXP-818's list language — a filled `GlassSectionBand`
/// over FLAT rows, no cards. Tapping a row reopens the compose page on that
/// draft; a hold opens the glass menu with Discard (web's row Menu).
struct DraftsListContent: View {
    let viewModel: DraftsViewModel
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute

    @ViewBuilder
    var body: some View {
        if viewModel.rows.isEmpty {
            // EXP-1274 `drafts`: the same page state Android and desktop draw.
            EmptyState(.drafts)
                .frame(maxHeight: .infinity)
        } else {
            list
        }
    }

    private var list: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                // EXP-1320: the band carries NO count.
                GlassSectionBand("Drafts")

                ForEach(viewModel.rows) { row in
                    draftRow(row)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
        }
        // Clearance for the floating tab bar (EXP-36).
        .tabBarBottomInset()
    }

    /// Web's `DraftsList` row (`ListRow`: p-3, gap-3): the status glyph, the
    /// title (muted italic while untitled) and the compact age right-aligned
    /// — one line, no buttons. Discard lives in the row's hold menu, the way
    /// web's long-press/right-click Menu carries it (EXP-1647).
    private func draftRow(_ row: IssueDraftRow) -> some View {
        let untitled = row.draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        return HStack(spacing: 12) {
            // The draft's status (NULL `status_id` = the team's Backlog
            // builtin), resolved by IssueDraftQueries.
            AppIcon(row.status.iconName, size: 16)
                .foregroundStyle(row.status.color)
                .frame(width: 16)

            Text(untitled ? IssueDraftPage.untitled : row.draft.title)
                .font(.system(size: 14))
                .italic(untitled)
                .foregroundStyle(.white.opacity(untitled ? TextOpacity.secondary : TextOpacity.primary))
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)

            Text(relativeWireDate(row.draft.updatedAt))
                .font(.system(size: 12).monospacedDigit())
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .lineLimit(1)
        }
        .padding(12)
        .contentShape(Rectangle())
        .flatRow()
        .glassLongPressMenu(onTap: {
            pushRoute(.issueDraft(
                accountId: accountId,
                draftId: row.draft.id,
                boardId: row.draft.boardId
            ))
        }) {
            GlassMenuItem("Discard draft", icon: AppIcons.uiDelete, destructive: true) {
                Task { await viewModel.delete(row) }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }
}
