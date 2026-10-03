import ExpUI
import ExpCore
import SwiftUI

/// EXP-1170: what the shared property pickers read and write — the issue
/// face's view model (writes go to the server) and the New issue page's draft
/// model (writes stay local and autosave) both adopt it, so the two pages
/// open the SAME pickers over the same vocabulary.
@MainActor
protocol IssuePropertyActions: AnyObject {
    /// The team's statuses in render order (EXP-314).
    var teamStatuses: [ResolvedIssueStatus] { get }
    var pickedStatus: ResolvedIssueStatus { get }
    var pickedPriority: IssuePriority { get }
    var pickedAssigneeId: String? { get }
    /// The assignee picker's members, display-name ordered.
    var teamUsers: [UserEntity] { get }
    /// The team's labels, name-sorted.
    var teamLabels: [LabelEntity] { get }
    var assignedLabelIds: Set<String> { get }
    /// The board picker's rows.
    var pickerBoards: [BoardEntity] { get }
    var pickedBoardId: String { get }

    func pickStatus(_ status: ResolvedIssueStatus)
    func pickPriority(_ priority: IssuePriority)
    func pickAssignee(_ userId: String?)
    func toggleLabelPick(_ labelId: String)
    func createLabelPick(named name: String)
}

/// The per-property TYPED pickers (EXP-1021) — one shared sheet, plain rows,
/// highlight selection — for every host: the face's chip box, the Properties
/// sheet stacked over it, and the New issue page (EXP-1170). Only the binding
/// and the two host-specific hand-offs differ:
/// - `onPickDuplicate`: non-nil offers the duplicate CATEGORY and intercepts
///   it (L27 — the face parks its canonical-issue picker); nil hides it (a
///   draft has nothing to be a duplicate of yet).
/// - `onSelectBoard`: the face parks the board for its "Move issue" confirm;
///   the draft just takes it.
struct IssuePropertyPickers<Model: IssuePropertyActions>: View {
    let model: Model
    let child: Binding<IssuePropertyChild?>
    var boardPickerTitle: String = "Move to board"
    var onPickDuplicate: (() -> Void)? = nil
    let onSelectBoard: (BoardEntity) -> Void
    let onDismiss: () -> Void

    var body: some View {
        ZStack {
            StatusPicker(
                // The team's own statuses in render order — the ONE picker
                // vocabulary (REV2-85, EXP-314).
                statuses: model.teamStatuses
                    .filter { onPickDuplicate != nil || $0.category != .duplicate }
                    .map(StatusPickerStatus.init),
                value: [model.pickedStatus.id],
                onChange: { picked in
                    guard let selected = model.teamStatuses.first(where: { picked.contains($0.id) })
                    else { return }
                    // Duplicate CATEGORY = status interception (L27): picking
                    // it hands off to the canonical-issue picker instead of
                    // writing the status directly. The hand-off is promoted
                    // on THIS picker's dismiss, never on a timer.
                    if selected.category == .duplicate, let onPickDuplicate {
                        onPickDuplicate()
                    } else {
                        model.pickStatus(selected)
                    }
                },
                open: issuePickerOpen(child, .status),
                hideTrigger: true,
                onDismiss: onDismiss,
                trigger: { EmptyView() }
            )

            PriorityPicker(
                options: IssuePriority.displayOrder.map(PriorityPickerOption.init),
                value: [model.pickedPriority.id],
                onChange: { picked in
                    guard let selected = IssuePriority.displayOrder
                        .first(where: { picked.contains($0.id) }) else { return }
                    model.pickPriority(selected)
                },
                open: issuePickerOpen(child, .priority),
                hideTrigger: true,
                onDismiss: onDismiss,
                trigger: { EmptyView() }
            )

            AssigneePicker(
                members: model.teamUsers.map(AssigneePickerMember.init),
                // An EMPTY set is unassigned; the picker offers the row that
                // clears the pick and reports it back as nothing.
                value: model.pickedAssigneeId.map { [$0] } ?? [],
                onChange: { picked in model.pickAssignee(picked.first) },
                open: issuePickerOpen(child, .assignee),
                hideTrigger: true,
                onDismiss: onDismiss,
                trigger: { EmptyView() }
            )

            IssueLabelsPicker(
                labels: model.teamLabels,
                assignedIds: model.assignedLabelIds,
                open: issuePickerOpen(child, .labels),
                onDismiss: onDismiss,
                onToggle: { labelId in model.toggleLabelPick(labelId) },
                onCreate: { name in model.createLabelPick(named: name) }
            )

            MoveBoardPicker(
                boards: model.pickerBoards,
                selectedId: model.pickedBoardId,
                open: issuePickerOpen(child, .moveBoard),
                onDismiss: onDismiss,
                title: boardPickerTitle,
                onSelect: onSelectBoard
            )
        }
    }
}

/// The issue face's side: every pick is a server write.
extension IssueDetailViewModel: IssuePropertyActions {
    var pickedStatus: ResolvedIssueStatus { resolvedStatus }
    var pickedPriority: IssuePriority { IssuePriority.from(issue?.priority) }
    var pickedAssigneeId: String? { issue?.assigneeId }
    var pickerBoards: [BoardEntity] { moveTargetBoards }
    var pickedBoardId: String { issue?.boardId ?? "" }

    func pickStatus(_ status: ResolvedIssueStatus) {
        Task { await setStatus(status) }
    }

    func pickPriority(_ priority: IssuePriority) {
        Task { await setPriority(priority) }
    }

    func pickAssignee(_ userId: String?) {
        Task { await setAssignee(userId) }
    }

    func toggleLabelPick(_ labelId: String) {
        Task { await toggleLabel(labelId) }
    }

    func createLabelPick(named name: String) {
        Task { await createAndAssignLabel(name: name, color: autoLabelColor(for: name)) }
    }
}
