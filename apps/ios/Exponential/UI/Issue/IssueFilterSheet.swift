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
                        .foregroundStyle(Accent.indigo)
                }
            }
            .padding(.bottom, 8)

            categoryRow("Status", count: vm.filters.statuses.count) { view = .status }
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
                Image(systemName: "chevron.right")
                    .font(.caption)
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
            ForEach(IssueStatus.displayOrder, id: \.self) { status in
                checkRow(selected: vm.filters.statuses.contains(status)) {
                    vm.toggleStatus(status)
                } content: {
                    Image(systemName: status.sfSymbol)
                        .font(.caption)
                        .foregroundStyle(status.color)
                        .frame(width: 18)
                    Text(status.label)
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
                    Image(systemName: priority.sfSymbol)
                        .font(.caption)
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
            TextField("Filter labels…", text: $labelQuery)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .padding(10)
                .background(Color.white.opacity(0.06))
                .clipShape(RoundedRectangle(cornerRadius: 8))
                .padding(.bottom, 4)

            let query = labelQuery.trimmingCharacters(in: .whitespaces)
            let filtered = vm.workspaceLabels.filter {
                query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
            }
            if filtered.isEmpty {
                Text(vm.workspaceLabels.isEmpty ? "No labels yet" : "No labels match")
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
                    Image(systemName: "chevron.left")
                        .font(.body.weight(.medium))
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
                    Image(systemName: "checkmark")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Accent.indigo)
                }
            }
            .padding(.vertical, 11)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
