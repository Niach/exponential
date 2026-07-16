import ExpCore
import GRDB
import SwiftUI

/// A reusable bottom-sheet picker used by the issue editor surfaces in place
/// of `Menu` popovers. Matches Linear's mobile UX where property pickers
/// open as half-height sheets containing a tap-target-friendly list of
/// options.
///
/// Selecting a row commits immediately via `onSelect` and dismisses the
/// sheet. The current selection is shown with a trailing checkmark.
struct PickerSheet<Item, ID: Hashable, Row: View>: View {
    let title: String
    let items: [Item]
    let selectedID: ID?
    let idFor: (Item) -> ID
    let onSelect: (Item) -> Void
    @ViewBuilder let row: (Item) -> Row

    @Environment(\.dismiss) private var dismiss

    private struct IdentifiedItem: Identifiable {
        let id: ID
        let value: Item
    }

    var body: some View {
        let identified = items.map { IdentifiedItem(id: idFor($0), value: $0) }
        NavigationStack {
            List {
                ForEach(identified) { wrapped in
                    Button {
                        onSelect(wrapped.value)
                        dismiss()
                    } label: {
                        HStack {
                            row(wrapped.value)
                            Spacer()
                            if let selectedID, wrapped.id == selectedID {
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
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

// MARK: - Assignee picker helper

/// Row model for the assignee `PickerSheet`. Wraps an optional user so we
/// can render the "Unassigned" sentinel as a first-class option with a
/// stable identifier (`"__unassigned"`).
struct AssigneeOption: Identifiable, Hashable {
    let id: String
    let userId: String?
    let displayName: String

    static let unassigned = AssigneeOption(
        id: "__unassigned",
        userId: nil,
        displayName: "Unassigned"
    )
}

/// User ids of a workspace's HUMAN members (widget/agent bots excluded), from
/// the synced `workspace_members ⋈ users` store. A member whose `users` row
/// hasn't synced yet is counted as human — conservative, so we never wrongly
/// hide the assignee picker on a workspace that actually has other people.
///
/// When this returns exactly one id the workspace is solo: both create + detail
/// surfaces skip the assignee picker and auto-assign that sole member (EXP-50).
func humanWorkspaceMemberIds(workspaceId: String, db: Database) throws -> [String] {
    let members = try WorkspaceMemberEntity
        .filter(Column("workspace_id") == workspaceId)
        .fetchAll(db)
    var ids: [String] = []
    for member in members {
        if let user = try UserEntity.fetchOne(db, key: member.userId) {
            if !user.isAgent { ids.append(member.userId) }
        } else {
            // User row not synced yet → assume human rather than hide the picker.
            ids.append(member.userId)
        }
    }
    return ids
}

// Assignable members — the widget helpdesk bot (is_agent) is excluded.
func assigneeOptions(users: [UserEntity]) -> [AssigneeOption] {
    var options: [AssigneeOption] = [.unassigned]
    for user in users.filter({ !$0.isAgent }) {
        options.append(
            AssigneeOption(
                id: user.id,
                userId: user.id,
                displayName: user.name ?? user.email
            )
        )
    }
    return options
}
