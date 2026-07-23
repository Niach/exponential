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
    let assignee: UserEntity?
    /// Assigned labels only, name-sorted by the caller.
    let assignedLabels: [LabelEntity]
    let singleMemberTeam: Bool
    let isModerator: Bool
    let onTap: (IssueDetailSheet) -> Void

    var body: some View {
        let status = IssueStatus.from(issue.status)
        let priority = IssuePriority.from(issue.priority)
        FlowLayout(spacing: 6) {
            chip(target: .status) {
                Image(systemName: status.sfSymbol)
                    .font(.caption)
                    .foregroundStyle(status.color)
                Text(status.label)
                    .font(.caption)
                    .foregroundStyle(.white)
            }
            chip(target: .priority) {
                Image(systemName: priority.sfSymbol)
                    .font(.caption)
                    .foregroundStyle(priority.color)
                Text(priority.label)
                    .font(.caption)
                    .foregroundStyle(.white)
            }
            if !singleMemberTeam {
                chip(target: .assignee) {
                    if let assigneeId = issue.assigneeId {
                        UserAvatar(user: assignee, id: assigneeId, size: 16)
                        Text(memberDisplayName(assignee, id: assigneeId))
                            .font(.caption)
                            .foregroundStyle(.white)
                    } else {
                        Image(systemName: "person.crop.circle.badge.xmark")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        Text("Unassigned")
                            .font(.caption)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
            }
            if let dueDate = issue.dueDate {
                let tint = dueDateUrgencyColor(dueDate)
                chip(target: .dueDate) {
                    Image(systemName: "calendar")
                        .font(.caption)
                        .foregroundStyle(tint)
                    Text(dueDateChipLabel(dueDate))
                        .font(.caption)
                        .foregroundStyle(tint)
                }
            }
            ForEach(assignedLabels, id: \.id) { label in
                chip(target: .labels) {
                    Circle()
                        .fill(Color(hex: label.color) ?? .gray)
                        .frame(width: 8, height: 8)
                    Text(label.name)
                        .font(.caption)
                        .foregroundStyle(.white)
                }
            }
            if isModerator {
                chip(target: .properties) {
                    Image(systemName: "plus")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
        .glassSection()
        // Box background opens the combined sheet; chip buttons win the hit
        // test over this tap gesture.
        .onTapGesture {
            guard isModerator else { return }
            onTap(.properties)
        }
        .opacity(isModerator ? 1 : 0.55)
        .disabled(!isModerator)
    }

    @ViewBuilder
    private func chip<Content: View>(
        target: IssueDetailSheet,
        @ViewBuilder content: () -> Content
    ) -> some View {
        Button {
            onTap(target)
        } label: {
            HStack(spacing: 5) {
                content()
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .glassButton()
        }
        .buttonStyle(.plain)
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
