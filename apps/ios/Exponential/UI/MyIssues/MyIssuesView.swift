import ExpUI
import ExpCore
import SwiftUI

/// Cross-board "assigned to me" list (masterplan §5a): all issues in the
/// active account with `assigneeId == me`, grouped by status, rows pushing
/// the issue detail. Same glass row and band language as `IssueListView`
/// (polish round: the phone issue row minus the assignee — it is always me —
/// and no board name; the identifier's prefix already names the board). No
/// background or navigation chrome of its own — it renders embedded as the My
/// Work tab's My issues segment (EXP-58).
struct MyIssuesListContent: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute
    @State private var viewModel: MyIssuesViewModel?
    // Same identifier column treatment as IssueListView (EXP-24): min width
    // so status icons/titles align across rows despite varying identifier
    // lengths — extra important here where rows span boards with different
    // prefix lengths (EXP-250).
    @ScaledMetric(relativeTo: .caption) private var identifierMinWidth: CGFloat = 60
    /// EXP-980: the issue whose blocks mini-graph is up (a row badge tap).
    @State private var blocksTarget: IssueGraphTarget?
    /// The board list's row height floor (EXP-251), so the two lists' rows
    /// match.
    @ScaledMetric(relativeTo: .subheadline) private var rowContentMinHeight: CGFloat = 22

    var body: some View {
        Group {
            if let vm = viewModel {
                if vm.renderGroups.isEmpty {
                    emptyState
                } else {
                    issueList(vm)
                }
            } else {
                Color.clear
            }
        }
        // EXP-980: the row badge's mini-graph — a tap on a node opens that
        // issue, the same push the rows make.
        .sheet(item: $blocksTarget) { target in
            if let vm = viewModel {
                IssueGraphSheet(
                    graph: vm.blockGraph(forIssueId: target.id),
                    issues: vm.relationIssues,
                    onOpenIssue: { id in
                        blocksTarget = nil
                        deps.deepLinkBus.navigateToIssue(id, accountId: accountId)
                    }
                )
            }
        }
        .onAppear {
            if viewModel == nil {
                viewModel = MyIssuesViewModel(accountId: accountId, db: deps.db, auth: deps.auth)
            }
            // Re-arm on every appear: pushing an issue detail stops the
            // observation (onDisappear), popping back must resume it.
            viewModel?.startObserving()
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Spacer()
            AppIcon(AppIcons.navAccount, size: 22)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Text("No issues assigned to you")
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder
    private func issueList(_ vm: MyIssuesViewModel) -> some View {
        List {
            // EXP-980: nested rows — a sub-issue follows its root, whatever
            // status it is in itself, so a group the nesting emptied is gone
            // and the header counts what is DISPLAYED here.
            ForEach(vm.renderGroups) { group in
                Section {
                    // P15: the band is an ordinary row (the board list's
                    // EXP-620 shape) that folds its group; it stays outside
                    // the collapse guard so a folded group keeps its band.
                    statusHeader(group: group, vm: vm)
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                        .listRowInsets(EdgeInsets())
                    if !vm.collapsedStatuses.contains(group.id) {
                        // EXP-965: the elbow connectors, off the rows' own
                        // depths; the list spaces rows 3pt apart, which the
                        // branch runs through.
                        let guides = TreeGuides.compute(depths: group.rows.map(\.depth))
                        ForEach(Array(group.rows.enumerated()), id: \.element.id) { index, row in
                            issueRow(issue: row.issue, vm: vm)
                                .treeGuides(guides[index], gap: 3)
                                .listRowBackground(Color.clear)
                                .listRowSeparator(.hidden)
                                .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                        }
                    }
                }
            }
        }
        .listStyle(.plain)
        // Same compact-list treatment as the board IssueListView (EXP-80):
        // zero the List's own content margins, kill the implicit 44pt row
        // floor, and flow sections without the inter-section band — without
        // these, My Issues rows sit inboard with visibly chunkier spacing
        // than the board list's.
        .contentMargins(.horizontal, 0, for: .scrollContent)
        .contentMargins(.top, 0, for: .scrollContent)
        .environment(\.defaultMinListRowHeight, 0)
        .listSectionSpacing(0)
        .scrollContentBackground(.hidden)
        .background(Color.clear)
        // Clearance for the floating tab bar (EXP-36) — this List renders as
        // the My Work tab's My Issues segment.
        .tabBarBottomInset()
    }

    /// The board list's status band (EXP-1248 `GlassSectionBand` with its
    /// count, the filled phone band): P14 the team row's name and glyph, P15
    /// the leading fold chevron — the whole band folds the group.
    private func statusHeader(group: MyIssuesViewModel.RenderGroup, vm: MyIssuesViewModel) -> some View {
        Button {
            vm.toggleStatusCollapsed(group.id)
        } label: {
            GlassSectionBand(group.status.name, count: group.rows.count) {
                AppIcon(vm.collapsedStatuses.contains(group.id) ? AppIcons.uiChevronRight : AppIcons.uiChevronDown,
                        size: 11, weight: .medium)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(width: 12)
                AppIcon(group.status.iconName, size: AppIcon.Size.small)
                    .foregroundStyle(group.status.color)
            } trailing: {
                EmptyView()
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("issue-status-band")
        .padding(.top, 6)
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// The phone issue row ×3 (polish round, pinned): priority · identifier ·
    /// status · title · up to 3 label dots · due date, then the in-card
    /// chevron — the board list's row minus the assignee (always me here) and
    /// with no board name. A plain Button + `pushRoute`, not a
    /// `NavigationLink`: a link in a List row draws the system disclosure
    /// OUTSIDE the glass card (EXP-698 r5).
    @ViewBuilder
    private func issueRow(issue: IssueEntity, vm: MyIssuesViewModel) -> some View {
        Button {
            pushRoute(.issue(accountId: accountId, id: issue.id))
        } label: {
            HStack(spacing: 10) {
                // Priority icon (16pt column, IssueListView/Android parity)
                AppIcon(IssuePriority.from(issue.priority).iconName, size: AppIcon.Size.small)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 16)

                // The identifier carries the board prefix ({PREFIX}-{n}) —
                // exactly the cross-board disambiguator this view needs
                // (EXP-250: a min width, longer ones grow).
                Text(issue.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .frame(minWidth: identifierMinWidth, alignment: .leading)

                // P14: the issue's own team status row, never the anchor.
                let status = vm.resolved(issue)
                AppIcon(status.iconName, size: AppIcon.Size.small)
                    .foregroundStyle(status.color)
                    .frame(width: 16)

                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .layoutPriority(1)

                // EXP-980: the blocks badge, right after the title. Tapping
                // the pill (not the row) opens this issue's mini-graph.
                if let counts = vm.blockCounts[issue.id] {
                    BlocksBadge(counts: counts) {
                        blocksTarget = IssueGraphTarget(id: issue.id)
                    }
                    .fixedSize(horizontal: true, vertical: false)
                }

                HStack(spacing: 10) {
                    HStack(spacing: 4) {
                        ForEach(vm.labelsFor(issueId: issue.id).prefix(3), id: \.id) { label in
                            Circle()
                                .fill(Color(hex: label.color) ?? .gray)
                                .frame(width: 8, height: 8)
                        }
                    }

                    if let dueDate = issue.dueDate {
                        HStack(spacing: 3) {
                            AppIcon(AppIcons.uiDueDate, size: 11)
                            Text(formatDueDate(dueDate))
                                .font(.caption)
                                .lineLimit(1)
                        }
                        .foregroundStyle(dueDateColor(dueDate))
                    }

                    AppIcon(AppIcons.uiChevronRight, size: 16)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
                .fixedSize(horizontal: true, vertical: false)
            }
            .frame(minHeight: rowContentMinHeight)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("issue-row-\(issue.identifier ?? issue.id)")
    }

    private func formatDueDate(_ dateString: String) -> String {
        guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return dateString }
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInTomorrow(date) { return "Tomorrow" }
        return AppDateFormatters.MMMd.string(from: date)
    }

    private func dueDateColor(_ dateString: String) -> Color {
        guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else {
            return .white.opacity(TextOpacity.tertiary)
        }
        // Due-today must win over overdue: the date parses to local midnight, which is already past.
        if Calendar.current.isDateInToday(date) { return DesignTokens.Semantic.orange }
        if date < Date() { return DesignTokens.Semantic.red }
        return .white.opacity(TextOpacity.tertiary)
    }
}
