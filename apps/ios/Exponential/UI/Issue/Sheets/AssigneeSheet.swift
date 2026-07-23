import ExpUI
import ExpCore
import SwiftUI

/// Searchable assignee picker (EXP-240): pinned Unassigned row, then member
/// rows (avatar + name) filtered by an inline search field. Selecting commits
/// immediately and dismisses.
struct AssigneeSheet: View {
    let users: [UserEntity]
    /// Current `issue.assigneeId` (nil = unassigned).
    let selectedId: String?
    let onSelect: (String?) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var searchText = ""

    private var filtered: [UserEntity] {
        let options = users.sorted {
            memberDisplayName($0, id: $0.id)
                .localizedCaseInsensitiveCompare(memberDisplayName($1, id: $1.id)) == .orderedAscending
        }
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return options }
        return options.filter {
            memberDisplayName($0, id: $0.id).localizedCaseInsensitiveContains(trimmed)
                || $0.email.localizedCaseInsensitiveContains(trimmed)
        }
    }

    var body: some View {
        GlassSheetChrome(title: "Assignee", detents: [.medium, .large]) {
            GlassSheetSearchField(placeholder: "Search members", text: $searchText)
            ScrollView {
                VStack(spacing: 2) {
                    GlassSheetRow(
                        label: "Unassigned",
                        selected: selectedId == nil,
                        labelOpacity: TextOpacity.secondary,
                        action: {
                            onSelect(nil)
                            dismiss()
                        }
                    ) {
                        Image(systemName: "person.crop.circle.badge.xmark")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }

                    if filtered.isEmpty {
                        Text("No matching members")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.top, 16)
                    }

                    ForEach(filtered, id: \.id) { user in
                        GlassSheetRow(
                            label: memberDisplayName(user, id: user.id),
                            selected: selectedId == user.id,
                            action: {
                                onSelect(user.id)
                                dismiss()
                            }
                        ) {
                            UserAvatar(user: user, id: user.id, size: 24)
                        }
                    }
                }
                .padding(.horizontal, 6)
                .padding(.bottom, 16)
            }
        }
    }
}
