import ExpUI
import ExpCore
import SwiftUI

/// Cross-project "assigned to me" list (masterplan §5a): all issues in the
/// active account with `assigneeId == me`, grouped by status, rows pushing
/// the issue detail. Same glass row language as `IssueListView`, plus a
/// project name per row since rows span projects. No background or navigation
/// chrome of its own — it renders embedded as the My Work tab's My Issues
/// segment (EXP-58; it previously hid inside Search's empty-query state).
struct MyIssuesListContent: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: MyIssuesViewModel?

    var body: some View {
        Group {
            if let vm = viewModel {
                if vm.issues.isEmpty {
                    emptyState
                } else {
                    issueList(vm)
                }
            } else {
                Color.clear
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
            Image(systemName: "person.crop.circle")
                .font(.title2)
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
            ForEach(IssueStatus.displayOrder, id: \.self) { status in
                let statusIssues = vm.issuesForStatus(status)
                if !statusIssues.isEmpty {
                    Section {
                        ForEach(statusIssues, id: \.id) { issue in
                            issueRow(issue: issue, vm: vm)
                                .listRowBackground(Color.clear)
                                .listRowSeparator(.hidden)
                                .listRowInsets(EdgeInsets(top: 1.5, leading: 16, bottom: 1.5, trailing: 16))
                        }
                    } header: {
                        statusHeader(status: status, count: statusIssues.count)
                            .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                            .listRowBackground(Color.clear)
                    }
                }
            }
        }
        .listStyle(.plain)
        // Same compact-list treatment as the project IssueListView (EXP-80):
        // zero the List's own content margins, kill the implicit 44pt row
        // floor, and flow sections without the inter-section band — without
        // these, My Issues rows sit inboard with visibly chunkier spacing
        // than the project list's.
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

    @ViewBuilder
    private func statusHeader(status: IssueStatus, count: Int) -> some View {
        HStack(spacing: 8) {
            Image(systemName: status.sfSymbol)
                .font(.caption)
                .foregroundStyle(status.color)

            Text(status.label)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Text("\(count)")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))

            Spacer()
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .textCase(nil)
    }

    @ViewBuilder
    private func issueRow(issue: IssueEntity, vm: MyIssuesViewModel) -> some View {
        NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
            HStack(spacing: 10) {
                // Priority icon (16pt column, IssueListView/Android parity)
                Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 16)

                // The identifier carries the project prefix ({PREFIX}-{n}) —
                // exactly the cross-project disambiguator this view needs.
                if let identifier = issue.identifier {
                    Text(identifier)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }

                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.from(issue.status).color)
                    .frame(width: 16)

                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)

                Spacer()

                if let project = vm.project(forId: issue.projectId) {
                    Text(project.name)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }

                if let dueDate = issue.dueDate {
                    HStack(spacing: 3) {
                        Image(systemName: "calendar")
                            .font(.caption2)
                        Text(formatDueDate(dueDate))
                            .font(.caption)
                    }
                    .foregroundStyle(dueDateColor(dueDate))
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
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
