import SwiftUI
import GRDB

struct IssueDetailView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: IssueDetailViewModel?
    @State private var showDeleteConfirm = false
    @FocusState private var titleFocused: Bool

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel, let issue = vm.issue {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        // Header: identifier + status
                        HStack {
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced().weight(.medium))
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .glassButton()
                            }
                            Spacer()
                        }

                        // Title (editable)
                        TextField("Title", text: Binding(
                            get: { vm.editingTitle },
                            set: { vm.editingTitle = $0 }
                        ))
                        .font(.title2.weight(.semibold))
                        .textFieldStyle(.plain)
                        .foregroundStyle(.white)
                        .focused($titleFocused)
                        .onSubmit { Task { await vm.saveTitle() } }
                        .onChange(of: titleFocused) { _, focused in
                            if !focused { Task { await vm.saveTitle() } }
                        }

                        // Description (editable)
                        TextField("Add description...", text: Binding(
                            get: { vm.editingDescription },
                            set: { vm.editingDescription = $0 }
                        ), axis: .vertical)
                        .font(.body)
                        .textFieldStyle(.plain)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(3...20)
                        .onChange(of: titleFocused) { _, _ in
                            Task { await vm.saveDescription() }
                        }

                        // Metadata
                        VStack(spacing: 0) {
                            // Status
                            detailRow(label: "Status") {
                                Menu {
                                    ForEach(IssueStatus.allCases) { s in
                                        Button {
                                            Task { await vm.setStatus(s) }
                                        } label: {
                                            Label(s.label, systemImage: s.sfSymbol)
                                        }
                                    }
                                } label: {
                                    HStack(spacing: 6) {
                                        Image(systemName: IssueStatus.from(issue.status).sfSymbol)
                                            .font(.caption)
                                            .foregroundStyle(IssueStatus.from(issue.status).color)
                                        Text(IssueStatus.from(issue.status).label)
                                            .font(.subheadline)
                                            .foregroundStyle(.white)
                                    }
                                }
                            }

                            Divider().background(Color.white.opacity(0.06))

                            // Priority
                            detailRow(label: "Priority") {
                                Menu {
                                    ForEach(IssuePriority.allCases) { p in
                                        Button {
                                            Task { await vm.setPriority(p) }
                                        } label: {
                                            Label(p.label, systemImage: p.sfSymbol)
                                        }
                                    }
                                } label: {
                                    HStack(spacing: 6) {
                                        Image(systemName: IssuePriority.from(issue.priority).sfSymbol)
                                            .font(.caption)
                                            .foregroundStyle(IssuePriority.from(issue.priority).color)
                                        Text(IssuePriority.from(issue.priority).label)
                                            .font(.subheadline)
                                            .foregroundStyle(.white)
                                    }
                                }
                            }

                            Divider().background(Color.white.opacity(0.06))

                            // Assignee
                            detailRow(label: "Assignee") {
                                Menu {
                                    Button {
                                        Task { await vm.setAssignee(nil) }
                                    } label: {
                                        Label("Unassigned", systemImage: "xmark")
                                    }
                                    ForEach(vm.users, id: \.id) { user in
                                        Button {
                                            Task { await vm.setAssignee(user.id) }
                                        } label: {
                                            Text(user.name ?? user.email)
                                        }
                                    }
                                } label: {
                                    if let assignee = vm.assignee() {
                                        Text(assignee.name ?? assignee.email)
                                            .font(.subheadline)
                                            .foregroundStyle(.white)
                                    } else {
                                        Text("Unassigned")
                                            .font(.subheadline)
                                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    }
                                }
                            }

                        }
                        .padding(.vertical, 4)
                        .glassSection()

                        // Due date — inline calendar
                        DueDatePicker(date: Binding(
                            get: { parseDate(issue.dueDate) },
                            set: { newDate in Task { await vm.setDueDate(newDate) } }
                        ))

                        // Labels
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Labels")
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))

                            FlowLayout(spacing: 6) {
                                ForEach(vm.labels, id: \.id) { label in
                                    Button {
                                        Task { await vm.toggleLabel(label.id) }
                                    } label: {
                                        HStack(spacing: 5) {
                                            Circle()
                                                .fill(Color(hex: label.color) ?? .gray)
                                                .frame(width: 8, height: 8)
                                            Text(label.name)
                                                .font(.caption)
                                                .foregroundStyle(.white)
                                        }
                                        .padding(.horizontal, 10)
                                        .padding(.vertical, 6)
                                        .glassButton(isActive: vm.assignedLabelIds.contains(label.id))
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }

                        // Error
                        if let error = vm.error {
                            Text(error)
                                .font(.callout)
                                .foregroundStyle(.red)
                        }

                        // Delete button
                        Button(role: .destructive) {
                            showDeleteConfirm = true
                        } label: {
                            HStack {
                                Image(systemName: "trash")
                                Text("Delete issue")
                            }
                            .font(.subheadline)
                            .foregroundStyle(.red.opacity(0.8))
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                            .glassRow()
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(20)
                }
            } else {
                ProgressView().tint(.white)
            }
        }
        .navigationTitle("Issue")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
        .alert("Delete Issue", isPresented: $showDeleteConfirm) {
            Button("Delete", role: .destructive) {
                Task {
                    if await viewModel?.deleteIssue() == true {
                        dismiss()
                    }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This action cannot be undone.")
        }
        .onAppear {
            if viewModel == nil {
                let vm = IssueDetailViewModel(
                    issueId: issueId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    labelsApi: deps.labelsApi
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
    private func detailRow<Content: View>(label: String, @ViewBuilder content: () -> Content) -> some View {
        HStack {
            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: 80, alignment: .leading)

            Spacer()

            content()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
    }

    private func parseDate(_ dateString: String?) -> Date? {
        guard let dateString else { return nil }
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.date(from: dateString)
    }

}

// MARK: - Flow Layout for labels

struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let result = arrange(proposal: proposal, subviews: subviews)
        return result.size
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = arrange(proposal: proposal, subviews: subviews)
        for (index, position) in result.positions.enumerated() {
            subviews[index].place(at: CGPoint(x: bounds.minX + position.x, y: bounds.minY + position.y), proposal: .unspecified)
        }
    }

    private func arrange(proposal: ProposedViewSize, subviews: Subviews) -> (positions: [CGPoint], size: CGSize) {
        let maxWidth = proposal.width ?? .infinity
        var positions: [CGPoint] = []
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        var maxX: CGFloat = 0

        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > maxWidth && x > 0 {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            positions.append(CGPoint(x: x, y: y))
            rowHeight = max(rowHeight, size.height)
            x += size.width + spacing
            maxX = max(maxX, x)
        }

        return (positions, CGSize(width: maxX, height: y + rowHeight))
    }
}
