import ExpUI
import ExpCore
import SwiftUI

/// The combined Properties sheet (EXP-240): one glass sheet listing every
/// editable property — Status / Priority / Assignee / Due date rows show the
/// current value + chevron and STACK their per-property picker over this sheet
/// (EXP-687 retired the dismiss-and-re-present hand-off; dismissing the child
/// returns here, exactly like Android); the Labels section toggles assigned
/// labels inline (stays open) with an add chip opening the searchable Labels
/// sheet; the Board row (last, after Labels) opens the move-board picker and
/// hides when there is nowhere to move.
struct IssuePropertiesSheet<Child: View>: View {
    let issue: IssueEntity
    /// EXP-314: the issue's status resolved against its team's status rows.
    let status: ResolvedIssueStatus
    let assignee: UserEntity?
    /// The issue's team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let assignedIds: Set<String>
    let singleMemberTeam: Bool
    /// The issue's own board — the row draws its glyph + color (EXP-449).
    let board: BoardEntity?
    let hasMoveTargets: Bool
    let onToggleLabel: (String) -> Void
    /// The picker stacked OVER this sheet. Host-owned: a picker that hands off
    /// to another one (duplicate status) is promoted by the host on dismiss.
    @Binding var activeChild: IssuePropertyChild?
    /// Fired once a child finished dismissing — the host promotes whatever a
    /// picker parked (a hand-off target, the picked move board).
    let onChildDismiss: () -> Void
    /// The per-property pickers, built by the host (they need the view model).
    @ViewBuilder let child: (IssuePropertyChild) -> Child

    var body: some View {
        let priority = IssuePriority.from(issue.priority)
        GlassSheetChrome(title: "Properties") {
            VStack(alignment: .leading, spacing: 2) {
                propertyRow(
                    label: "Status",
                    value: status.name,
                    target: .status
                ) {
                    AppIcon(status.iconName, size: AppIcon.Size.medium)
                        .foregroundStyle(status.color)
                }
                propertyRow(
                    label: "Priority",
                    value: priority.label,
                    target: .priority
                ) {
                    AppIcon(priority.iconName, size: AppIcon.Size.medium)
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
                            AppIcon(AppIcons.uiUnassigned, size: AppIcon.Size.medium)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                    }
                }
                propertyRow(
                    label: "Due date",
                    value: issue.dueDate.map(dueDateChipLabel) ?? "None",
                    target: .dueDate
                ) {
                    AppIcon(AppIcons.uiDueDate, size: AppIcon.Size.medium)
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
                        GlassPill(
                            label.name,
                            mode: .select(isSelected: true) { onToggleLabel(label.id) },
                            dot: Color(hex: label.color) ?? .gray
                        )
                    }
                    GlassPill("Label", icon: AppIcons.uiAdd, mode: .action {
                        activeChild = .labels
                    })
                }
                .padding(.horizontal, 14)

                if hasMoveTargets {
                    propertyRow(
                        label: "Board",
                        value: board?.name ?? "",
                        target: .moveBoard
                    ) {
                        // The board's own glyph + color, not a generic
                        // boards nav icon (EXP-449).
                        if let board = board {
                            AppIcon(BoardTypeDisplay.iconName(for: board), size: AppIcon.Size.medium)
                                .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
                        } else {
                            AppIcon(AppIcons.navBoards, size: AppIcon.Size.medium)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                    }
                    .padding(.top, 8)
                }
            }
            .padding(.horizontal, 6)
            .padding(.bottom, 24)
            // The child rides the INNER node — the chrome root carries the
            // move confirm, and no node may own two presentations (EXP-240).
            .sheet(item: $activeChild, onDismiss: onChildDismiss) { target in
                child(target)
            }
        }
    }

    @ViewBuilder
    private func propertyRow<Leading: View>(
        label: String,
        value: String,
        target: IssuePropertyChild,
        @ViewBuilder leading: () -> Leading
    ) -> some View {
        Button {
            activeChild = target
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
                AppIcon(AppIcons.uiChevronRight, size: 11)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 14)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
