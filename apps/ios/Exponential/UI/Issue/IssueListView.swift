import ExpUI
import ExpCore
import SwiftUI
import GRDB

struct IssueListView: View {
    let projectId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @State private var viewModel: IssueListViewModel?
    @State private var showFilterSheet = false

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                VStack(spacing: 0) {
                    if let project = vm.project, project.repositoryId != nil {
                        HStack {
                            RepoNameChip(
                                accountId: accountId,
                                workspaceId: project.workspaceId,
                                repositoryId: project.repositoryId
                            )
                            Spacer()
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 8)
                        .background(Color.white.opacity(0.04))
                    }
                    issueListContent(vm)
                }
            }
        }
        .navigationTitle(viewModel?.project?.name ?? "Issues")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .sheet(isPresented: $showFilterSheet) {
            if let vm = viewModel {
                IssueFilterSheet(vm: vm)
                    .presentationDetents([.medium, .large])
                    .presentationBackground(.ultraThinMaterial)
            }
        }
        .onAppear {
            if viewModel == nil {
                let vm = IssueListViewModel(
                    accountId: accountId,
                    projectId: projectId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    projectsApi: deps.projectsApi,
                    auth: deps.auth
                )
                viewModel = vm
                vm.startObserving()
            }
        }
        .onDisappear {
            viewModel?.stopObserving()
        }
    }

    @ViewBuilder
    private func issueListContent(_ vm: IssueListViewModel) -> some View {
        VStack(spacing: 0) {
            // Filter bar (search lives in the Search tab, not the issue list)
            filterBar(vm)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

            if !vm.filters.isEmpty {
                activeFilterPills(vm)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 8)
            }

            List {
                ForEach(IssueStatus.displayOrder, id: \.self) { status in
                    let statusIssues = vm.issuesForStatus(status)
                    if !statusIssues.isEmpty {
                        Section {
                            if !vm.collapsedStatuses.contains(status) {
                                ForEach(statusIssues, id: \.id) { issue in
                                    issueRow(issue: issue, vm: vm)
                                        .listRowBackground(Color.clear)
                                        .listRowSeparator(.hidden)
                                        .listRowInsets(EdgeInsets(top: 3, leading: 16, bottom: 3, trailing: 16))
                                        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                                            if vm.permissions.canMutateIssue(creatorId: issue.creatorId) {
                                                Button {
                                                    Task { await vm.setStatus(issueId: issue.id, status: .done) }
                                                } label: {
                                                    Label("Done", systemImage: "checkmark.circle.fill")
                                                }
                                                .tint(.green)

                                                Button {
                                                    Task { await vm.setStatus(issueId: issue.id, status: .cancelled) }
                                                } label: {
                                                    Label("Cancel", systemImage: "xmark.circle.fill")
                                                }
                                                .tint(.gray)
                                            }
                                        }
                                        .swipeActions(edge: .leading, allowsFullSwipe: true) {
                                            if vm.permissions.canMutateIssue(creatorId: issue.creatorId) {
                                                Button {
                                                    Task { await vm.setStatus(issueId: issue.id, status: .backlog) }
                                                } label: {
                                                    Label("Backlog", systemImage: "circle.dashed")
                                                }
                                                .tint(.orange)
                                            }
                                        }
                                }
                            }
                        } header: {
                            statusHeader(status: status, count: statusIssues.count, vm: vm)
                                .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 2, trailing: 16))
                                .listRowBackground(Color.clear)
                        }
                    }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(Color.clear)
            .refreshable {
                await vm.refresh()
            }
            .safeAreaInset(edge: .bottom) {
                Color.clear.frame(height: 16)
            }
        }
    }

    @ViewBuilder
    private func filterBar(_ vm: IssueListViewModel) -> some View {
        HStack(spacing: 8) {
            // Filter sheet trigger with active-count badge (Android parity).
            Button {
                showFilterSheet = true
            } label: {
                Image(systemName: "line.3.horizontal.decrease")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.white.opacity(vm.filters.isEmpty ? TextOpacity.secondary : 1.0))
                    .padding(.horizontal, 12)
                    .padding(.vertical, 9)
                    .overlay(alignment: .topTrailing) {
                        if vm.filters.count > 0 {
                            Text("\(vm.filters.count)")
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(.white)
                                .frame(minWidth: 15, minHeight: 15)
                                .background(Accent.indigo, in: Circle())
                                .offset(x: 4, y: -4)
                        }
                    }
            }
            .glassButton(isActive: !vm.filters.isEmpty)

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(FilterTab.allCases) { tab in
                        Button {
                            vm.setTab(tab)
                        } label: {
                            Text(tab.label)
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white.opacity(vm.activeTab == tab ? 1.0 : TextOpacity.secondary))
                                .padding(.horizontal, 14)
                                .padding(.vertical, 8)
                        }
                        .glassButton(isActive: vm.activeTab == tab)
                    }

                    if !vm.filters.isEmpty {
                        Divider()
                            .frame(height: 20)
                            .tint(.white.opacity(0.1))

                        Button {
                            vm.clearFilters()
                        } label: {
                            HStack(spacing: 4) {
                                Text("Clear")
                                    .font(.caption)
                                Image(systemName: "xmark")
                                    .font(.caption2)
                            }
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 6)
                        }
                        .glassButton()
                    }
                }
            }
        }
    }

    /// Removable pills for every active filter (web/Android parity).
    @ViewBuilder
    private func activeFilterPills(_ vm: IssueListViewModel) -> some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(IssueStatus.displayOrder.filter { vm.filters.statuses.contains($0) }, id: \.self) { status in
                    filterPill(icon: status.sfSymbol, iconColor: status.color, text: status.label) {
                        vm.toggleStatus(status)
                    }
                }
                ForEach(IssuePriority.displayOrder.filter { vm.filters.priorities.contains($0) }, id: \.self) { priority in
                    filterPill(icon: priority.sfSymbol, iconColor: priority.color, text: priority.label) {
                        vm.togglePriority(priority)
                    }
                }
                ForEach(vm.workspaceLabels.filter { vm.filters.labelIds.contains($0.id) }, id: \.id) { label in
                    filterPill(dotColor: Color(hex: label.color) ?? .gray, text: label.name) {
                        vm.toggleLabel(label.id)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func filterPill(
        icon: String? = nil,
        iconColor: Color = .white,
        dotColor: Color? = nil,
        text: String,
        onRemove: @escaping () -> Void
    ) -> some View {
        Button(action: onRemove) {
            HStack(spacing: 5) {
                if let icon {
                    Image(systemName: icon)
                        .font(.caption2)
                        .foregroundStyle(iconColor)
                }
                if let dotColor {
                    Circle()
                        .fill(dotColor)
                        .frame(width: 7, height: 7)
                }
                Text(text)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                Image(systemName: "xmark")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
        }
        .glassButton()
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func statusHeader(status: IssueStatus, count: Int, vm: IssueListViewModel) -> some View {
        Button {
            vm.toggleStatusCollapsed(status)
        } label: {
            HStack(spacing: 8) {
                Image(systemName: vm.collapsedStatuses.contains(status) ? "chevron.right" : "chevron.down")
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .frame(width: 12)

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
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .textCase(nil)
    }

    @ViewBuilder
    private func issueRow(issue: IssueEntity, vm: IssueListViewModel) -> some View {
        NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
            HStack(spacing: 10) {
                // Priority icon
                Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssuePriority.from(issue.priority).color)
                    .frame(width: 20)

                // Identifier
                if let identifier = issue.identifier {
                    Text(identifier)
                        .font(.caption.monospaced())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }

                // Status icon
                Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                    .font(.caption)
                    .foregroundStyle(IssueStatus.from(issue.status).color)
                    .frame(width: 16)

                // Title
                Text(issue.title)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)

                Spacer()

                // Labels
                HStack(spacing: 4) {
                    let issueLabels = vm.labelsFor(issueId: issue.id)
                    ForEach(issueLabels.prefix(3), id: \.id) { label in
                        Circle()
                            .fill(Color(hex: label.color) ?? .gray)
                            .frame(width: 8, height: 8)
                    }
                }

                // Due date
                if let dueDate = issue.dueDate {
                    HStack(spacing: 3) {
                        Image(systemName: "calendar")
                            .font(.caption2)
                        Text(formatDueDate(dueDate))
                            .font(.caption)
                    }
                    .foregroundStyle(dueDateColor(dueDate))
                }

                // Assignee avatar (pseudonym initial when the user row isn't synced)
                if let assigneeId = issue.assigneeId {
                    userAvatar(vm.userFor(id: assigneeId), id: assigneeId, size: 22)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func userAvatar(_ user: UserEntity?, id: String, size: CGFloat) -> some View {
        let initial = memberDisplayName(user, id: id).prefix(1).uppercased()
        Text(initial)
            .font(.system(size: size * 0.45, weight: .medium))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Color.white.opacity(0.15))
            .clipShape(Circle())
    }

    private func formatDueDate(_ dateString: String) -> String {
        guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return dateString }
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInTomorrow(date) { return "Tomorrow" }
        return AppDateFormatters.MMMd.string(from: date)
    }

    private func dueDateColor(_ dateString: String) -> Color {
        guard let date = AppDateFormatters.yyyyMMdd.date(from: dateString) else { return .white.opacity(TextOpacity.tertiary) }
        if date < Date() { return DesignTokens.Semantic.red }
        if Calendar.current.isDateInToday(date) { return DesignTokens.Semantic.orange }
        return .white.opacity(TextOpacity.tertiary)
    }
}

