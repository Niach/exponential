import ExpUI
import ExpCore
import SwiftUI

/// The combined Properties sheet (EXP-240): one glass sheet listing every
/// editable property — Status / Priority / Assignee / Due date rows show the
/// current value + chevron and HAND OFF to their per-property sheet (the
/// parent dismisses this sheet and re-presents the target after 0.4s, the
/// same trick as the duplicate-status interception); the Labels section
/// toggles assigned labels inline (stays open) with an add chip handing off
/// to the searchable Labels sheet; the Board row (last, after Labels) hands
/// off to the existing move-board picker and hides when there is nowhere to
/// move.
struct IssuePropertiesSheet: View {
    let issue: IssueEntity
    let assignee: UserEntity?
    /// The issue's team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let assignedIds: Set<String>
    let singleMemberTeam: Bool
    let boardName: String?
    let hasMoveTargets: Bool
    /// Hand off to a per-property sheet (parent owns the dismiss + reopen).
    let onNavigate: (IssueDetailSheet) -> Void
    let onToggleLabel: (String) -> Void

    var body: some View {
        let status = IssueStatus.from(issue.status)
        let priority = IssuePriority.from(issue.priority)
        GlassSheetChrome(title: "Properties", detents: [.medium, .large]) {
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    propertyRow(
                        label: "Status",
                        value: status.label,
                        target: .status
                    ) {
                        Image(systemName: status.sfSymbol)
                            .font(.body)
                            .foregroundStyle(status.color)
                    }
                    propertyRow(
                        label: "Priority",
                        value: priority.label,
                        target: .priority
                    ) {
                        Image(systemName: priority.sfSymbol)
                            .font(.body)
                            .foregroundStyle(priority.color)
                    }
                    // Solo team: no one else to reassign to (EXP-50).
                    if !singleMemberTeam {
                        propertyRow(
                            label: "Assignee",
                            value: issue.assigneeId.map { memberDisplayName(assignee, id: $0) } ?? "Unassigned",
                            target: .assignee
                        ) {
                            if let assigneeId = issue.assigneeId {
                                UserAvatar(user: assignee, id: assigneeId, size: 22)
                            } else {
                                Image(systemName: "person.crop.circle.badge.xmark")
                                    .font(.body)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                        }
                    }
                    propertyRow(
                        label: "Due date",
                        value: issue.dueDate.map(dueDateChipLabel) ?? "None",
                        target: .dueDate
                    ) {
                        Image(systemName: "calendar")
                            .font(.body)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }

                    // Labels: assigned chips toggle inline (removal), the add
                    // chip hands off to the searchable sheet.
                    Text("Labels")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 14)
                        .padding(.top, 14)
                        .padding(.bottom, 6)

                    FlowLayout(spacing: 6) {
                        ForEach(labels.filter { assignedIds.contains($0.id) }, id: \.id) { label in
                            Button {
                                onToggleLabel(label.id)
                            } label: {
                                HStack(spacing: 5) {
                                    Circle()
                                        .fill(Color(hex: label.color) ?? .gray)
                                        .frame(width: 8, height: 8)
                                    Text(label.name)
                                        .font(.caption)
                                        .foregroundStyle(.white)
                                }
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .glassButton(isActive: true)
                            }
                            .buttonStyle(.plain)
                        }
                        Button {
                            onNavigate(.labels)
                        } label: {
                            HStack(spacing: 4) {
                                Image(systemName: "plus")
                                    .font(.caption2)
                                Text("Label")
                                    .font(.caption)
                            }
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                            .glassButton()
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 14)

                    if hasMoveTargets {
                        propertyRow(
                            label: "Board",
                            value: boardName ?? "",
                            target: .moveBoard
                        ) {
                            Image(systemName: "folder")
                                .font(.body)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        .padding(.top, 8)
                    }
                }
                .padding(.horizontal, 6)
                .padding(.bottom, 24)
            }
        }
    }

    @ViewBuilder
    private func propertyRow<Leading: View>(
        label: String,
        value: String,
        target: IssueDetailSheet,
        @ViewBuilder leading: () -> Leading
    ) -> some View {
        Button {
            onNavigate(target)
        } label: {
            HStack(spacing: 10) {
                leading()
                    .frame(width: 24)
                Text(label)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Spacer(minLength: 0)
                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                Image(systemName: "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 14)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
