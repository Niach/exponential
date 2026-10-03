import ExpUI
import ExpCore
import SwiftUI

/// The top property chip box (EXP-240) — one glass box of wrapping capsule
/// chips replacing the old properties/times/labels sections: Status, Priority,
/// Assignee (hidden on solo teams, EXP-50), Due date (only when set), Estimate
/// (EXP-630: only while the team's scale is on), one chip per assigned label,
/// and a "+" chip. A chip opens its per-property sheet;
/// the box background (and "+") opens the combined Properties sheet.
/// Non-moderators see it dimmed and inert, with the "+" chip hidden.
///
/// EXP-1170: the New issue page wears the SAME box over its draft (a
/// `Subject` instead of an issue row): no estimate (the draft passes
/// `issueEstimationNone`), a due-date chip even while unset, a board chip
/// iff the team has more than one board, and "+" adds a label (there is no
/// Properties sheet behind a draft).
struct IssuePropertyChipsBox: View {
    /// The values the chips read — an issue row's, or a draft's.
    struct Subject {
        var priority: String
        var assigneeId: String?
        var dueDate: String?
        var estimate: Int?

        init(priority: String, assigneeId: String?, dueDate: String?, estimate: Int? = nil) {
            self.priority = priority
            self.assigneeId = assigneeId
            self.dueDate = dueDate
            self.estimate = estimate
        }

        init(_ issue: IssueEntity) {
            self.init(
                priority: issue.priority,
                assigneeId: issue.assigneeId,
                dueDate: issue.dueDate,
                estimate: issue.estimate
            )
        }
    }

    let subject: Subject
    /// EXP-314: the issue's status resolved against its team's status rows.
    let status: ResolvedIssueStatus
    let assignee: UserEntity?
    /// Assigned labels only, name-sorted by the caller.
    let assignedLabels: [LabelEntity]
    let singleMemberTeam: Bool
    /// EXP-630: the team's estimate scale (contract `issueEstimation`). The
    /// chip is HIDDEN while it is `none` — the web's `EstimateControl`.
    let estimationType: String
    let isModerator: Bool
    /// EXP-1170: the board chip (the draft page, on a multi-board team);
    /// nil = no board chip.
    var board: BoardEntity? = nil
    /// EXP-1170: show the due-date chip ("Due date") while it is unset — the
    /// draft has no Properties sheet to set it from.
    var showsUnsetDueDate: Bool = false
    /// The box's dead space opens Properties too; the draft page turns it
    /// off (its "+" only adds a label).
    var backgroundOpensProperties: Bool = true
    /// EXP-1170: the draft's chip order — labels and "+" before the due
    /// date, the board chip last. The face keeps due date · estimate · labels.
    var labelsBeforeDueDate: Bool = false

    /// The due-date chip's label while unset (the draft page only).
    static let unsetDueDateLabel = "Due date"
    /// A chip opens its per-property picker directly (EXP-687: the pickers
    /// are their own enum now — the combined sheet is not one of them).
    let onTapProperty: (IssuePropertyChild) -> Void
    let onOpenProperties: () -> Void

    var body: some View {
        let priority = IssuePriority.from(subject.priority)
        FlowLayout(spacing: 6) {
            chip(target: .status, label: status.name) {
                AppIcon(status.iconName, size: GlassPillTokens.glyphSm)
                    .foregroundStyle(status.color)
            }
            chip(target: .priority, label: priority.label) {
                AppIcon(priority.iconName, size: GlassPillTokens.glyphSm)
                    .foregroundStyle(priority.color)
            }
            if !singleMemberTeam {
                if let assigneeId = subject.assigneeId {
                    chip(target: .assignee, label: memberDisplayName(assignee, id: assigneeId)) {
                        UserAvatar(user: assignee, id: assigneeId, size: 16)
                    }
                } else {
                    chip(target: .assignee, label: "Unassigned") {
                        AppIcon(AppIcons.uiUnassigned, size: GlassPillTokens.glyphSm)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
            }
            // EXP-1170: the draft draws the labels before the due date and
            // the board chip LAST (web/desktop order); the face keeps its own.
            if labelsBeforeDueDate { labelChips }
            if let dueDate = subject.dueDate {
                // The urgency color rides the GLYPH, the way the status and
                // priority chips carry theirs — a pill's label is always the
                // shared white.
                chip(target: .dueDate, label: dueDateChipLabel(dueDate)) {
                    AppIcon(AppIcons.uiDueDate, size: GlassPillTokens.glyphSm)
                        .foregroundStyle(dueDateUrgencyColor(dueDate))
                }
            } else if showsUnsetDueDate {
                chip(target: .dueDate, label: Self.unsetDueDateLabel) {
                    AppIcon(AppIcons.uiDueDate, size: GlassPillTokens.glyphSm)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            if IssueEstimate.isEnabled(estimationType) {
                // Reads "Estimate" until set, then the short form ("L", "5 pt")
                // — byte-identical to the web chip.
                chip(
                    target: .estimate,
                    label: subject.estimate.map { estimateShortLabel($0, scale: estimationType) } ?? "Estimate"
                ) {
                    AppIcon(AppIcons.uiEstimate, size: GlassPillTokens.glyphSm)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
            if !labelsBeforeDueDate { labelChips }
            if let board {
                chip(target: .moveBoard, label: board.name) {
                    AppIcon(AppIcons.navBoards, size: GlassPillTokens.glyphSm)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        // A chip cloud, not a stack of rows: it needs the card's own border.
        .glassCard()
        // Box background opens the combined sheet; chip buttons win the hit
        // test over this tap gesture.
        .onTapGesture {
            guard isModerator, backgroundOpensProperties else { return }
            onOpenProperties()
        }
        .opacity(isModerator ? 1 : 0.55)
        .disabled(!isModerator)
    }

    /// The assigned labels' chips, then "+".
    @ViewBuilder
    private var labelChips: some View {
        ForEach(assignedLabels, id: \.id) { label in
            GlassPill(
                label.name,
                mode: .action { onTapProperty(.labels) },
                dot: Color(hex: label.color) ?? .gray
            )
        }
        if isModerator {
            GlassPill("", mode: .action { onOpenProperties() }) {
                AppIcon(AppIcons.uiAdd, size: GlassPillTokens.glyphSm, weight: .medium)
            }
        }
    }

    private func chip<Leading: View>(
        target: IssuePropertyChild,
        label: String,
        @ViewBuilder leading: () -> Leading
    ) -> some View {
        GlassPill(label, mode: .action { onTapProperty(target) }, leading: leading)
    }

}

/// Shared due-date display label (Today/Tomorrow/"MMM d", no year) — used by
/// the chip box and the Properties sheet so both surfaces read identically.
func dueDateChipLabel(_ wire: String) -> String {
    guard let date = AppDateFormatters.yyyyMMdd.date(from: wire) else { return wire }
    let cal = Calendar.current
    if cal.isDateInToday(date) { return "Today" }
    if cal.isDateInTomorrow(date) { return "Tomorrow" }
    return AppDateFormatters.MMMd.string(from: date)
}

/// Due-date urgency tint (Android `dueDateColor` parity): red overdue, orange
/// today, muted otherwise.
func dueDateUrgencyColor(_ wire: String) -> Color {
    guard let date = AppDateFormatters.yyyyMMdd.date(from: wire) else {
        return .white.opacity(TextOpacity.secondary)
    }
    // Due-today must win over overdue: the date parses to local midnight,
    // which is already past.
    if Calendar.current.isDateInToday(date) { return DesignTokens.Semantic.orange }
    if date < Date() { return DesignTokens.Semantic.red }
    return .white.opacity(TextOpacity.secondary)
}
