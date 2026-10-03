import ExpUI
import ExpCore
import SwiftUI

/// The combined Properties sheet (EXP-240): one glass sheet listing every
/// editable property. EXP-698 r5 made it the New-issue page's own rows —
/// `GlassMetaRow`s in one `.glassSection()` group (Status / Priority /
/// Assignee / Labels / Due date / Estimate / Board), each STACKING its
/// per-property picker over this sheet (EXP-687 retired the
/// dismiss-and-re-present hand-off; dismissing the child returns here, exactly
/// like Android) — followed by `IssueRelationsSection` (EXP-736; the EXP-1097
/// foldable bands). EXP-1170: Labels is a row like the rest (no inline pill
/// cloud), opening the searchable multi-select Labels sheet. The Board row
/// hides when there is nowhere to move.
struct IssuePropertiesSheet<Child: View, Pickers: View>: View {
    let issue: IssueEntity
    /// EXP-314: the issue's status resolved against its team's status rows.
    let status: ResolvedIssueStatus
    let assignee: UserEntity?
    /// The issue's team's labels, name-sorted by the caller.
    let labels: [LabelEntity]
    let assignedIds: Set<String>
    /// EXP-736/EXP-1097: the view model the Relations bands read
    /// (`relationsView`, fold state, counterpart rows). The detail page draws
    /// the parent line + Sub-issues; every other relation is listed here.
    let relationsSource: IssueDetailViewModel
    let singleMemberTeam: Bool
    /// EXP-630: the team's estimate scale; the row hides while it is `none`.
    let estimationType: String
    /// The issue's own board — the row draws its glyph + color (EXP-449).
    let board: BoardEntity?
    let hasMoveTargets: Bool
    let onRemoveRelation: (IssueRelationRow) -> Void
    /// A relation row was tapped: the host dismisses and opens that issue.
    let onOpenRelation: (String) -> Void
    /// The picker stacked OVER this sheet. Host-owned: a picker that hands off
    /// to another one (duplicate status) is promoted by the host on dismiss.
    @Binding var activeChild: IssuePropertyChild?
    /// Fired once a child finished dismissing — the host promotes whatever a
    /// picker parked (a hand-off target, the picked move board).
    let onChildDismiss: () -> Void
    /// The per-property children that still present a VIEW, built by the host
    /// (they need the view model).
    @ViewBuilder let child: (IssuePropertyChild) -> Child
    /// EXP-1021: the typed pickers, also host-built. They own their own sheets
    /// and read `activeChild` through their own bindings, so they ride as a
    /// second presentation node beside the `.sheet(item:)` below.
    @ViewBuilder let pickers: () -> Pickers

    /// The Labels row's value: the assigned labels' names in the team's
    /// label order, or "None".
    private var assignedLabelsValue: String {
        let names = labels.filter { assignedIds.contains($0.id) }.map(\.name)
        return names.isEmpty ? "None" : names.joined(separator: ", ")
    }

    var body: some View {
        let priority = IssuePriority.from(issue.priority)
        GlassSheetChrome(title: "Properties") {
            VStack(alignment: .leading, spacing: 16) {
                // EXP-698 r5: the New-issue page's rows, verbatim — one
                // `.glassSection()` group of `GlassMetaRow`s separated by
                // hairlines, value-led glyphs, no chevrons. The sheet used to
                // draw its own row (leading gutter glyph + trailing chevron),
                // so the same five properties looked like two different lists
                // depending on whether the issue existed yet.
                VStack(spacing: 0) {
                    GlassMetaRow(
                        label: "Status",
                        icon: status.iconName,
                        iconColor: status.color,
                        value: status.name
                    ) { activeChild = .status }

                    GlassDivider()

                    GlassMetaRow(
                        label: "Priority",
                        icon: priority.iconName,
                        iconColor: priority.color,
                        value: priority.label
                    ) { activeChild = .priority }

                    // Solo team: no one else to reassign to (EXP-50).
                    if !singleMemberTeam {
                        GlassDivider()

                        GlassMetaRow(
                            label: "Assignee",
                            icon: issue.assigneeId == nil ? AppIcons.uiUnassigned : AppIcons.uiAssignee,
                            iconColor: .white.opacity(TextOpacity.secondary),
                            value: issue.assigneeId.map { memberDisplayName(assignee, id: $0) } ?? "Unassigned"
                        ) { activeChild = .assignee }
                    }

                    GlassDivider()

                    // EXP-1170: the assigned labels as one value line (team
                    // label order); the row opens the searchable multi-select
                    // sheet, which stays open across toggles.
                    GlassMetaRow(
                        label: "Labels",
                        icon: AppIcons.settingsLabels,
                        iconColor: .white.opacity(TextOpacity.secondary),
                        value: assignedLabelsValue
                    ) { activeChild = .labels }

                    GlassDivider()

                    GlassMetaRow(
                        label: "Due date",
                        icon: AppIcons.uiDueDate,
                        iconColor: .white.opacity(TextOpacity.secondary),
                        value: issue.dueDate.map(dueDateChipLabel) ?? "None"
                    ) { activeChild = .dueDate }

                    // EXP-630: the estimate row, after Due date like the
                    // web's phone sheet; hidden while the team's scale is off.
                    if IssueEstimate.isEnabled(estimationType) {
                        GlassDivider()

                        GlassMetaRow(
                            label: "Estimate",
                            icon: AppIcons.uiEstimate,
                            iconColor: .white.opacity(TextOpacity.secondary),
                            value: estimateLabel(issue.estimate, scale: estimationType)
                        ) { activeChild = .estimate }
                    }

                    // The move picker hides when there is nowhere to move to.
                    if hasMoveTargets {
                        GlassDivider()

                        GlassMetaRow(
                            label: "Board",
                            // The board's own glyph + color, not a generic
                            // boards nav icon (EXP-449).
                            icon: board.map { BoardTypeDisplay.iconName(for: $0) } ?? AppIcons.navBoards,
                            iconColor: board.flatMap { Color(hex: $0.color ?? "#888888") }
                                ?? .white.opacity(TextOpacity.secondary),
                            value: board?.name ?? ""
                        ) { activeChild = .moveBoard }
                    }
                }
                .glassSection()

                // EXP-736/EXP-1097: relations sit under the property rows —
                // the "Relations" heading with its Add, then one foldable band
                // per side.
                IssueRelationsSection(
                    vm: relationsSource,
                    onAdd: { activeChild = .addRelation },
                    onOpen: onOpenRelation,
                    onRemove: onRemoveRelation
                )
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 24)
            // The child rides the INNER node — the chrome root carries the
            // move confirm, and no node may own two presentations (EXP-240).
            .sheet(item: issueViewChild($activeChild), onDismiss: onChildDismiss) { target in
                child(target)
            }
            // A second node for the typed pickers (EXP-240: no node may own
            // two presentations).
            .background { pickers() }
        }
    }
}
