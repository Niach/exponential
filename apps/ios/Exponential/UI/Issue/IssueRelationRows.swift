import ExpCore
import ExpUI
import SwiftUI

// EXP-1097 — the issue detail's relations, drawn off `IssueRelationsView` ×4:
// the "Sub-issue of" parent line above the title, the Sub-issues section
// (completion ring · "Sub-issues" · `done/total` · only a `+`) over FLAT 48pt
// hairline rows, and — in the properties sheet — ONE foldable band per
// remaining relation side (`IssueRelationsSection`). Every row is the SAME
// relation row: status glyph · identifier · title · assignee.

enum IssueRelationRowTokens {
    /// The row's minimum height (web `min-h-12`, Android 48dp).
    static let minHeight: CGFloat = 48
    static let glyphSize: CGFloat = 16
    static let avatarSize: CGFloat = 24
    /// The "Add sub-issues" empty row (web `h-11`).
    static let addRowHeight: CGFloat = 44
    /// "Show N more" / "Show less" (web `h-11`).
    static let moreRowHeight: CGFloat = 44
}

/// THE relation row ×4 — sub-issues and every relation band share it. Tap
/// opens the counterpart; the long-press menu removes the relation.
struct IssueRelationListRow: View {
    let row: IssueRelationsView.Row
    /// The counterpart's status resolved against the team (EXP-314); nil =
    /// its anchor enum.
    let status: ResolvedIssueStatus?
    let assignee: UserEntity?
    let assigneeId: String?
    let removeLabel: String
    let onOpen: () -> Void
    let onRemove: (() -> Void)?

    var body: some View {
        Button(action: onOpen) {
            HStack(spacing: 12) {
                AppIcon(iconName, size: IssueRelationRowTokens.glyphSize)
                    .foregroundStyle(iconColor)
                    .frame(width: 20)
                Text(row.identifier)
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .fixedSize()
                Text(row.title)
                    .font(.subheadline)
                    // A finished sub-task recedes (web `text-foreground/60`).
                    .foregroundStyle(.white.opacity(row.status == IssueStatus.done.rawValue ? 0.6 : 1))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                avatar
            }
            .padding(.horizontal, 12)
            .frame(minHeight: IssueRelationRowTokens.minHeight)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .flatRow()
        .contextMenu {
            if let onRemove {
                Button(role: .destructive, action: onRemove) {
                    Text(removeLabel)
                }
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(row.identifier) \(row.title)")
        .accessibilityAddTraits(.isButton)
    }

    private var iconName: String {
        status?.iconName ?? IssueStatus.from(row.status).iconName
    }

    private var iconColor: Color {
        status?.color ?? IssueStatus.from(row.status).color
    }

    @ViewBuilder
    private var avatar: some View {
        if let assigneeId {
            UserAvatar(user: assignee, id: assigneeId, size: IssueRelationRowTokens.avatarSize)
        } else {
            AppIcon(AppIcons.uiAssignee, size: 20)
                .foregroundStyle(.white.opacity(TextOpacity.quaternary))
                .frame(
                    width: IssueRelationRowTokens.avatarSize,
                    height: IssueRelationRowTokens.avatarSize
                )
        }
    }
}

/// The rows of one section, hairline-divided, flat (EXP-818).
struct IssueRelationRowList: View {
    let rows: [IssueRelationsView.Row]
    let vm: IssueDetailViewModel
    let removeLabel: String
    let onOpen: (String) -> Void
    /// nil = the rows cannot be unlinked from here.
    let onRemove: ((String) -> Void)?

    var body: some View {
        VStack(spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.element.id) { index, row in
                if index > 0 { GlassDivider() }
                IssueRelationListRow(
                    row: row,
                    status: vm.relationStatus(id: row.id),
                    assignee: vm.relationAssignee(id: row.id),
                    assigneeId: vm.relationIssue(id: row.id)?.assigneeId,
                    removeLabel: removeLabel,
                    onOpen: { onOpen(row.id) },
                    onRemove: onRemove.map { remove in { remove(row.id) } }
                )
            }
        }
    }
}

/// "↳ Sub-issue of [EXP-1080 Title]" — the line above the title while the
/// issue has a parent. The chip opens it.
struct IssueParentLine: View {
    let parent: IssueRelationsView.Row
    let status: ResolvedIssueStatus?
    let onOpen: (() -> Void)?

    var body: some View {
        HStack(spacing: 6) {
            AppIcon(AppIcons.relationSubIssue, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text(IssueRelationsView.Copy.subIssueOf)
                .font(.footnote)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .lineLimit(1)
                .fixedSize()
            IssueChip(
                identifier: parent.identifier,
                title: parent.title,
                iconName: status?.iconName ?? IssueStatus.from(parent.status).iconName,
                statusColor: status?.color ?? IssueStatus.from(parent.status).color,
                onTap: onOpen
            )
            .layoutPriority(-1)
            Spacer(minLength: 0)
        }
        .accessibilityIdentifier("issue-parent-line")
    }
}

/// The Sub-issues section: the completion ring in the team's COMPLETED
/// colour, "Sub-issues", `done/total` and ONLY a `+` (no settings), over the
/// flat rows; a lone "Add sub-issues" row while there are none.
struct IssueSubIssuesSection: View {
    let vm: IssueDetailViewModel
    let subIssues: IssueRelationsView.SubIssues
    /// The composer route that files a sub-issue; nil = the reader cannot
    /// create issues here.
    let addRoute: AppRoute?
    let onOpen: (String) -> Void

    var body: some View {
        if subIssues.rows.isEmpty {
            if let addRoute { addRow(addRoute) }
        } else {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(IssueRelationsView.Copy.subIssues) {
                    ProgressRing(
                        done: subIssues.done,
                        total: subIssues.total,
                        color: vm.completedStatus?.color ?? StatusColor.done
                    )
                } trailing: {
                    if let progress = subIssues.progress {
                        Text(progress)
                            .font(.caption.monospaced())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    }
                    if let addRoute {
                        NavigationLink(value: addRoute) {
                            AppIcon(AppIcons.uiAdd, size: 14, weight: .medium)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                .frame(width: 28, height: 24)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Add sub-issue")
                        .accessibilityIdentifier("sub-issues-add")
                    }
                }

                IssueRelationRowList(
                    rows: subIssues.rows,
                    vm: vm,
                    removeLabel: "Remove sub-issue",
                    onOpen: onOpen,
                    onRemove: vm.permissions.isModerator
                        ? { childId in
                            guard let row = vm.subIssueRow(childId: childId) else { return }
                            Task { await vm.removeRelation(row) }
                        }
                        : nil
                )
            }
            .accessibilityIdentifier("sub-issues-section")
        }
    }

    private func addRow(_ route: AppRoute) -> some View {
        NavigationLink(value: route) {
            HStack(spacing: 8) {
                AppIcon(AppIcons.uiAdd, size: 14, weight: .medium)
                Text(IssueRelationsView.Copy.addSubIssues)
                    .font(.subheadline)
                Spacer(minLength: 0)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .padding(.horizontal, 12)
            .frame(height: IssueRelationRowTokens.addRowHeight)
            .background(GlassTokens.fillSection)
            .clipShape(RoundedRectangle(cornerRadius: GlassTokens.rowRadius))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("sub-issues-add-row")
    }
}
