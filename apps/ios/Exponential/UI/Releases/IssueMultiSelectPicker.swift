import ExpUI
import ExpCore
import SwiftUI
import GRDB

/// Shared searchable multi-select issue list for the release sheets (EXP-62):
/// the creation sheet and the detail's add-issues sheet render the same
/// search field + status/identifier/title rows with a trailing check.
/// Selection is hoisted; the search query is internal. Rows toggle membership
/// — tapping never dismisses the enclosing sheet.
struct IssueMultiSelectPicker: View {
    /// nil while the candidates are still loading.
    let candidates: [IssueEntity]?
    @Binding var selected: Set<String>

    @State private var searchText = ""

    private var filtered: [IssueEntity] {
        guard let candidates else { return [] }
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return candidates }
        return candidates.filter {
            $0.title.localizedCaseInsensitiveContains(trimmed)
                || ($0.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
        }
    }

    var body: some View {
        // Inline search field. NOT system .searchable — on iOS 26+ it
        // renders as a bottom-edge glass bar (see IssueListView).
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Search issues", text: $searchText)
                .textFieldStyle(.plain)
                .submitLabel(.search)
            if !searchText.isEmpty {
                Button {
                    searchText = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
        .padding(.horizontal, 16)
        .padding(.vertical, 8)

        Group {
            if candidates == nil {
                ProgressView()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if filtered.isEmpty {
                ContentUnavailableView(
                    "No matching issues",
                    systemImage: "shippingbox",
                    description: Text("Open workspace issues you can add will appear here.")
                )
            } else {
                List {
                    ForEach(filtered, id: \.id) { issue in
                        Button {
                            if selected.contains(issue.id) {
                                selected.remove(issue.id)
                            } else {
                                selected.insert(issue.id)
                            }
                        } label: {
                            HStack(spacing: 10) {
                                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                                    .font(.caption)
                                    .foregroundStyle(IssueStatus.from(issue.status).color)
                                if let identifier = issue.identifier {
                                    Text(identifier)
                                        .font(.caption.monospaced())
                                        .foregroundStyle(.secondary)
                                }
                                Text(issue.title)
                                    .lineLimit(1)
                                Spacer()
                                if selected.contains(issue.id) {
                                    Image(systemName: "checkmark")
                                        .font(.subheadline.weight(.semibold))
                                        .foregroundStyle(.tint)
                                }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
    }
}

/// Candidates for the release issue pickers: every non-archived workspace
/// issue whose status isn't done/cancelled/duplicate and that isn't already
/// in `excludingReleaseId` (other-release issues stay offered — the server
/// records both timeline sides). Pass nil to exclude no release (the
/// creation sheet — the release doesn't exist yet). Archived issues are
/// excluded to match Android's DAO query AND the detail view's own rendering
/// (issuesForStatus hides them — adding one would look like a silent no-op).
/// One-shot read — the sheets are transient. Newest-touched first.
func loadAddableReleaseIssues(
    pool: DatabasePool?,
    workspaceId: String,
    excludingReleaseId: String?
) async -> [IssueEntity] {
    guard let pool else { return [] }
    let excludedStatuses: Set<String> = [
        IssueStatus.done.rawValue,
        IssueStatus.cancelled.rawValue,
        IssueStatus.duplicate.rawValue,
    ]
    let result: [IssueEntity]? = try? await pool.read { db in
        let workspaceProjectIds = try ProjectEntity
            .filter(Column("workspace_id") == workspaceId)
            .fetchAll(db)
            .map(\.id)
        return try IssueEntity
            .filter(workspaceProjectIds.contains(Column("project_id")))
            .fetchAll(db)
            .filter {
                !excludedStatuses.contains($0.status)
                    && ($0.releaseId == nil || $0.releaseId != excludingReleaseId)
                    && $0.archivedAt == nil
            }
            .sorted { $0.updatedAt > $1.updatedAt }
    }
    return result ?? []
}
