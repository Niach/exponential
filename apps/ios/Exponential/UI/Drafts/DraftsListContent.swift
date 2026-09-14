import ExpCore
import ExpUI
import SwiftUI

/// EXP-878 — the Drafts segment of My Work: every unfiled issue this user has
/// composed, across teams. EXP-818's list language — a filled `GlassSectionBand`
/// over FLAT rows, no cards. Tapping a row reopens the compose page on that
/// draft; the trash glyph is a SIBLING of the link, so deleting never
/// navigates (the Settings labels list's pattern).
struct DraftsListContent: View {
    let viewModel: DraftsViewModel
    @Environment(\.accountId) private var accountId

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Drafts") {
                    Text("\(viewModel.rows.count)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }

                ForEach(viewModel.rows) { row in
                    draftRow(row)
                }

                if let error = viewModel.error {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red.opacity(0.8))
                        .padding(.top, 8)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 4)
        }
        // Clearance for the floating tab bar (EXP-36).
        .tabBarBottomInset()
    }

    @ViewBuilder
    private func draftRow(_ row: IssueDraftRow) -> some View {
        HStack(spacing: 10) {
            NavigationLink(value: AppRoute.createIssue(
                accountId: accountId,
                boardId: row.draft.boardId,
                draftId: row.draft.id
            )) {
                HStack(spacing: 10) {
                    // The draft's status (NULL `status_id` = the team's
                    // Backlog builtin), resolved by IssueDraftQueries.
                    AppIcon(row.status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(row.status.color)
                        .frame(width: 16)

                    VStack(alignment: .leading, spacing: 2) {
                        if row.draft.title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                            Text("Untitled draft")
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                .lineLimit(1)
                        } else {
                            Text(row.draft.title)
                                .font(.subheadline)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                        }

                        // Drafts span teams here, so the board names the
                        // destination; the stamp is the last edit.
                        Text("\(row.board.name) · \(relativeWireDate(row.draft.updatedAt))")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    }

                    Spacer(minLength: 8)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            GhostIconButton(
                AppIcons.uiDelete,
                accessibilityLabel: "Delete draft",
                tint: DesignTokens.Palette.destructive.opacity(0.7)
            ) {
                Task { await viewModel.delete(row) }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .flatRow()
    }
}
