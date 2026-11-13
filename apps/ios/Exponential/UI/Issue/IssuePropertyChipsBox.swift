import ExpUI
import ExpCore
import SwiftUI

/// The top property chip box (EXP-240) — one glass box of wrapping capsule
/// chips replacing the old properties/times/labels sections: Status, Priority,
/// Assignee (hidden on solo teams, EXP-50), Due date (only when set), Estimate
/// (EXP-630: only while the team's scale is on), ONE labels chip (web's
/// label-picker trigger: tag glyph + dots + names, "Label" while none) and
/// the board chip. Polish round order ×4: status · priority · assignee · labels · due
/// date · estimate · board. A chip opens its per-property sheet;
/// the box background opens the combined Properties sheet.
/// Non-moderators see it dimmed and inert.
///
/// EXP-1170: the New issue page wears the SAME box over its draft (a
/// `Subject` instead of an issue row): no estimate (the draft passes
/// `issueEstimationNone`), a due-date chip even while unset, a board chip
/// iff the team has more than one board (the face always passes its board),
/// and no Properties sheet behind the box.
struct IssuePropertyChipsBox: View {
    /// The values the chips read — an issue row's, or a draft's.
    struct Subject {
        var priority: String
        var assigneeId: String?
        /// #EXP-1383: an agent assignee (exclusive with `assigneeId`).
        var assigneeAgentId: String?
        var dueDate: String?
        var estimate: Int?

        init(
            priority: String, assigneeId: String?, assigneeAgentId: String? = nil,
            dueDate: String?, estimate: Int? = nil
        ) {
            self.priority = priority
            self.assigneeId = assigneeId
            self.assigneeAgentId = assigneeAgentId
            self.dueDate = dueDate
            self.estimate = estimate
        }

        init(_ issue: IssueEntity) {
            self.init(
                priority: issue.priority,
                assigneeId: issue.assigneeId,
                assigneeAgentId: issue.assigneeAgentId,
                dueDate: issue.dueDate,
                estimate: issue.estimate
            )
        }
    }

    let subject: Subject
    /// EXP-314: the issue's status resolved against its team's status rows.
    let status: ResolvedIssueStatus
    let assignee: UserEntity?
    /// #EXP-1383: the agent assignee's row (nil while unsynced or none).
    var assigneeAgent: AgentDefinitionEntity? = nil
    /// Assigned labels only, name-sorted by the caller.
    let assignedLabels: [LabelEntity]
    /// Solo team without agents (EXP-50, #EXP-1383): no assignee chip.
    let hidesAssignee: Bool
    /// EXP-630: the team's estimate scale (contract `issueEstimation`). The
    /// chip is HIDDEN while it is `none` — the web's `EstimateControl`.
    let estimationType: String
    let isModerator: Bool
    /// The board chip (the face always; the draft page on a multi-board
    /// team): the board's own glyph in its colour + its name, no chevron.
    /// nil = no board chip.
    var board: BoardEntity? = nil
    /// EXP-1170: show the due-date chip ("Due date") while it is unset — the
    /// draft has no Properties sheet to set it from.
    var showsUnsetDueDate: Bool = false
    /// The box's dead space opens Properties too; the draft page turns it
    /// off.
    var backgroundOpensProperties: Bool = true
    /// Polish round P46: labels before the due date, the board chip
    /// last — the web/desktop order, now the default on the face too.
    var labelsBeforeDueDate: Bool = true

    /// The due-date chip's label while unset (the draft page only).
    static let unsetDueDateLabel = "Due date"
    /// P45: the empty chips' copy ×4 (web assignee-picker / label-picker).
    static let unsetAssigneeLabel = "Assignee"
    static let unsetLabelsLabel = "Label"
    /// A chip opens its per-property picker directly (EXP-687: the pickers
    /// are their own enum now — the combined sheet is not one of them).
    let onTapProperty: (IssuePropertyChild) -> Void
    let onOpenProperties: () -> Void

    var body: some View {
        let priority = IssuePriority.from(subject.priority)
        FlowLayout(spacing: GlassPillTokens.chipBoxGap) {
            chip(target: .status, label: status.name) {
                AppIcon(status.iconName, size: GlassPillTokens.glyphSm)
                    .foregroundStyle(status.color)
            }
            chip(target: .priority, label: priority.label) {
                AppIcon(priority.iconName, size: GlassPillTokens.glyphSm)
                    .foregroundStyle(priority.color)
            }
            if !hidesAssignee {
                if subject.assigneeAgentId != nil {
                    // #EXP-1383: an agent assignee reads like a member: the
                    // agent mark + its name.
                    chip(target: .assignee, label: assigneeAgent?.name ?? AgentSettings.AssigneeCopy.agentFallback) {
                        UserAvatar(agent: UserAvatarAgent(icon: assigneeAgent?.icon, color: assigneeAgent?.color), size: GlassPillTokens.avatarSm)
                    }
                } else if let assigneeId = subject.assigneeId {
                    chip(target: .assignee, label: memberDisplayName(assignee, id: assigneeId)) {
                        UserAvatar(user: assignee, id: assigneeId, size: GlassPillTokens.avatarSm)
                    }
                } else {
                    chip(target: .assignee, label: Self.unsetAssigneeLabel) {
                        AppIcon(AppIcons.uiUnassigned, size: GlassPillTokens.glyphSm)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                }
            }
            // Labels before the due date and the board chip LAST (the
            // web/desktop order ×4, P46).
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
                // P44: the board's own glyph tinted with its colour, as the
                // search rows and web/desktop draw it.
                chip(target: .moveBoard, label: board.name) {
                    AppIcon(BoardTypeDisplay.iconName(for: board), size: GlassPillTokens.glyphSm)
                        .foregroundStyle(Color(hex: board.color ?? "#888888") ?? .gray)
                }
            }
        }
        .padding(GlassPillTokens.chipBoxPadding)
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

    /// EXP-1647: ONE labels chip, web's `LabelPicker` trigger ×4: the tag
    /// glyph, then up to three overlapping colour dots and the names joined
    /// by ", " — or "Label" while none are assigned. A tap opens the label
    /// picker; there is no "+" chip.
    private var labelChips: some View {
        chip(
            target: .labels,
            label: assignedLabels.isEmpty
                ? Self.unsetLabelsLabel
                : assignedLabels.map(\.name).joined(separator: ", ")
        ) {
            AppIcon(AppIcons.settingsLabels, size: GlassPillTokens.glyphSm)
            if !assignedLabels.isEmpty {
                HStack(spacing: -2) {
                    ForEach(assignedLabels.prefix(3), id: \.id) { label in
                        // EXP-1822: web `h-2 w-2 ring-1 ring-background`: an
                        // 8pt disc in the label's colour, its 1pt ring painted
                        // OUTSIDE it (it used to be inset, a 6pt disc).
                        Circle()
                            .fill(Color(hex: label.color) ?? .gray)
                            .frame(width: GlassPillTokens.labelDot, height: GlassPillTokens.labelDot)
                            .background(Circle().fill(DesignTokens.Palette.background).padding(-1))
                    }
                }
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
