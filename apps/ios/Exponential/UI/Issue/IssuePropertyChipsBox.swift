import ExpUI
import ExpCore
import SwiftUI

/// The top property chip box (EXP-240) — one glass box of wrapping capsule
/// chips replacing the old properties/times/labels sections: Status, Priority,
/// Assignee (hidden on solo teams, EXP-50), Due date (only when set), one chip
/// per assigned label, and a "+" chip. A chip opens its per-property sheet;
/// the box background (and "+") opens the combined Properties sheet.
/// Non-moderators see it dimmed and inert, with the "+" chip hidden.
struct IssuePropertyChipsBox: View {
    let issue: IssueEntity
    /// EXP-314: the issue's status resolved against its team's status rows.
    let status: ResolvedIssueStatus
    let assignee: UserEntity?
    /// Assigned labels only, name-sorted by the caller.
    let assignedLabels: [LabelEntity]
    let singleMemberTeam: Bool
    let isModerator: Bool
    /// A chip opens its per-property picker directly (EXP-687: the pickers
    /// are their own enum now — the combined sheet is not one of them).
    let onTapProperty: (IssuePropertyChild) -> Void
    let onOpenProperties: () -> Void

    var body: some View {
        let priority = IssuePriority.from(issue.priority)
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
                if let assigneeId = issue.assigneeId {
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
            if let dueDate = issue.dueDate {
                // The urgency color rides the GLYPH, the way the status and
                // priority chips carry theirs — a pill's label is always the
                // shared white.
                chip(target: .dueDate, label: dueDateChipLabel(dueDate)) {
                    AppIcon(AppIcons.uiDueDate, size: GlassPillTokens.glyphSm)
                        .foregroundStyle(dueDateUrgencyColor(dueDate))
                }
            }
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
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        // A chip cloud, not a stack of rows: it needs the card's own border.
        .glassCard()
        // Box background opens the combined sheet; chip buttons win the hit
        // test over this tap gesture.
        .onTapGesture {
            guard isModerator else { return }
            onOpenProperties()
        }
        .opacity(isModerator ? 1 : 0.55)
        .disabled(!isModerator)
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
