import ExpCore
import ExpUI
import SwiftUI

/// Drill-down filter sheet, mirroring Android's `IssueFilterSheet` and the web
/// filter popover: a category list (Status / Priority / Labels, each with its
/// active count) drills into a dedicated sub-view; the Labels sub-view adds a
/// search field. All toggles reuse the shared IssueFilters model on the
/// view model.
struct IssueFilterSheet: View {
    let vm: IssueListViewModel

    @Environment(\.dismiss) private var dismiss
    @State private var view: FilterView = .categories
    @State private var labelQuery = ""

    private enum FilterView { case categories, status, priority, labels }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            switch view {
            case .categories: categoriesView
            case .status: statusView
            case .priority: priorityView
            case .labels: labelsView
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 20)
        .padding(.top, 24)
        .animation(nil, value: view)
    }

    // MARK: - Categories

    private var categoriesView: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Filters")
                    .font(.headline)
                    .foregroundStyle(.white)
                Spacer()
                if !vm.filters.isEmpty {
                    Button("Clear all") { vm.clearFilters() }
                        .font(.subheadline)
                        .foregroundStyle(Color.white)
                }
            }
            .padding(.bottom, 8)

            categoryRow("Status", count: vm.filters.statusIds.count) { view = .status }
            categoryRow("Priority", count: vm.filters.priorities.count) { view = .priority }
            categoryRow("Labels", count: vm.filters.labelIds.count) { view = .labels }
        }
    }

    private func categoryRow(_ label: String, count: Int, onTap: @escaping () -> Void) -> some View {
        Button(action: onTap) {
            HStack {
                Text(label)
                    .font(.body)
                    .foregroundStyle(.white)
                Spacer()
                if count > 0 {
                    Text("\(count)")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                AppIcon(AppIcons.uiChevronRight, size: AppIcon.Size.small)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .padding(.vertical, 14)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Sub-views

    private var statusView: some View {
        subView(title: "Status") {
            // EXP-314: the team's own statuses, keyed by group id (a row id, or
            // `builtin:<key>` while the statuses shape is still syncing).
            ForEach(vm.teamStatuses, id: \.id) { status in
                checkRow(selected: vm.isStatusFiltered(status)) {
                    vm.toggleStatus(status)
                } content: {
                    AppIcon(status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(status.color)
                        .frame(width: 18)
                    Text(status.name)
                        .font(.body)
                        .foregroundStyle(.white)
                }
            }
        }
    }

    private var priorityView: some View {
        subView(title: "Priority") {
            ForEach(IssuePriority.displayOrder, id: \.self) { priority in
                checkRow(selected: vm.filters.priorities.contains(priority)) {
                    vm.togglePriority(priority)
                } content: {
                    AppIcon(priority.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(priority.color)
                        .frame(width: 18)
                    Text(priority.label)
                        .font(.body)
                        .foregroundStyle(.white)
                }
            }
        }
    }

    private var labelsView: some View {
        subView(title: "Labels") {
            GlassTextField("Filter labels…", text: $labelQuery)
                .padding(.bottom, 4)

            let query = labelQuery.trimmingCharacters(in: .whitespaces)
            let filtered = vm.teamLabels.filter {
                query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
            }
            if filtered.isEmpty {
                Text(vm.teamLabels.isEmpty ? "No labels yet" : "No labels match")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    .padding(.vertical, 12)
            }
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(filtered, id: \.id) { label in
                        checkRow(selected: vm.filters.labelIds.contains(label.id)) {
                            vm.toggleLabel(label.id)
                        } content: {
                            Circle()
                                .fill(Color(hex: label.color) ?? .gray)
                                .frame(width: 10, height: 10)
                                .frame(width: 18)
                            Text(label.name)
                                .font(.body)
                                .foregroundStyle(.white)
                        }
                    }
                }
            }
        }
    }

    private func subView(title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Button {
                    view = .categories
                } label: {
                    AppIcon(AppIcons.uiChevronLeft, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                Text(title)
                    .font(.headline)
                    .foregroundStyle(.white)
            }
            .padding(.bottom, 8)
            content()
        }
    }

    private func checkRow(
        selected: Bool,
        onTap: @escaping () -> Void,
        @ViewBuilder content: () -> some View
    ) -> some View {
        Button(action: onTap) {
            HStack(spacing: 10) {
                content()
                Spacer()
                if selected {
                    AppIcon(AppIcons.uiCheck, size: 15, weight: .semibold)
                        .foregroundStyle(Color.white)
                }
            }
            .padding(.vertical, 11)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
