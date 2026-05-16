import SwiftUI
import GRDB

struct IssueListView: View {
    let projectId: String

    @Environment(AppDependencies.self) private var deps
    @State private var viewModel: IssueListViewModel?
    @State private var showCreateSheet = false
    @State private var selectedIssueId: String?

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel {
                issueListContent(vm)
            }
        }
        .navigationTitle(viewModel?.project?.name ?? "Issues")
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showCreateSheet = true
                } label: {
                    Image(systemName: "plus")
                        .foregroundStyle(.white)
                }
            }
        }
        .sheet(isPresented: $showCreateSheet) {
            CreateIssueSheet(projectId: projectId, onCreated: {})
                .presentationBackground(.ultraThinMaterial)
        }
        .sheet(item: $selectedIssueId) { issueId in
            NavigationStack {
                IssueDetailView(issueId: issueId)
            }
            .presentationBackground(.ultraThinMaterial)
        }
        .onAppear {
            if viewModel == nil {
                let vm = IssueListViewModel(projectId: projectId, db: deps.db)
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
            // Filter bar
            filterBar(vm)
                .padding(.horizontal, 16)
                .padding(.vertical, 8)

            ScrollView {
                LazyVStack(spacing: 12, pinnedViews: []) {
                    ForEach(IssueStatus.displayOrder, id: \.self) { status in
                        let statusIssues = vm.issuesForStatus(status)
                        if !statusIssues.isEmpty {
                            statusGroup(status: status, issues: statusIssues, vm: vm)
                        }
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 32)
            }
        }
    }

    @ViewBuilder
    private func filterBar(_ vm: IssueListViewModel) -> some View {
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
                        vm.filters = IssueFilters()
                        vm.activeTab = .all
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

    @ViewBuilder
    private func statusGroup(status: IssueStatus, issues: [IssueEntity], vm: IssueListViewModel) -> some View {
        VStack(spacing: 6) {
            // Status header
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

                    Text("\(issues.count)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))

                    Spacer()
                }
                .padding(.vertical, 6)
                .padding(.horizontal, 8)
            }
            .buttonStyle(.plain)

            // Issues
            if !vm.collapsedStatuses.contains(status) {
                ForEach(issues, id: \.id) { issue in
                    issueRow(issue: issue, vm: vm)
                }
            }
        }
    }

    @ViewBuilder
    private func issueRow(issue: IssueEntity, vm: IssueListViewModel) -> some View {
        Button {
            selectedIssueId = issue.id
        } label: {
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

                // Assignee avatar
                if let assignee = vm.userFor(id: issue.assigneeId) {
                    userAvatar(assignee, size: 22)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .glassRow()
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func userAvatar(_ user: UserEntity, size: CGFloat) -> some View {
        let initial = (user.name ?? user.email).prefix(1).uppercased()
        Text(initial)
            .font(.system(size: size * 0.45, weight: .medium))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(Color.white.opacity(0.15))
            .clipShape(Circle())
    }

    private func formatDueDate(_ dateString: String) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        guard let date = formatter.date(from: dateString) else { return dateString }
        let calendar = Calendar.current
        if calendar.isDateInToday(date) { return "Today" }
        if calendar.isDateInTomorrow(date) { return "Tomorrow" }
        formatter.dateFormat = "MMM d"
        return formatter.string(from: date)
    }

    private func dueDateColor(_ dateString: String) -> Color {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        guard let date = formatter.date(from: dateString) else { return .white.opacity(TextOpacity.tertiary) }
        if date < Date() { return .red }
        if Calendar.current.isDateInToday(date) { return .orange }
        return .white.opacity(TextOpacity.tertiary)
    }
}

// MARK: - String identifiable for sheet

extension String: @retroactive Identifiable {
    public var id: String { self }
}

// MARK: - Color from hex

extension Color {
    init?(hex: String) {
        let cleaned = hex.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "#", with: "")
        guard cleaned.count == 6, let rgb = UInt64(cleaned, radix: 16) else { return nil }
        self.init(
            red: Double((rgb >> 16) & 0xFF) / 255.0,
            green: Double((rgb >> 8) & 0xFF) / 255.0,
            blue: Double(rgb & 0xFF) / 255.0
        )
    }
}
