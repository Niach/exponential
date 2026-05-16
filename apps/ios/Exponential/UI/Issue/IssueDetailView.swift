import SwiftUI
import GRDB

struct IssueDetailView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: IssueDetailViewModel?
    @State private var showDeleteConfirm = false
    @State private var pendingImages: [String: PendingImage] = [:]
    @State private var descriptionDirty = false
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

                        // Description (markdown editor with image upload)
                        MarkdownEditor(
                            text: Binding(
                                get: { vm.editingDescription },
                                set: { newValue in
                                    vm.editingDescription = newValue
                                    descriptionDirty = true
                                }
                            ),
                            pendingImages: $pendingImages
                        )
                        .onChange(of: titleFocused) { _, focused in
                            if !focused && descriptionDirty {
                                Task {
                                    await uploadPendingAndSaveDescription(vm)
                                }
                            }
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

                        // Times (only when a due date is set; matches the
                        // server-side semantics where dueTime depends on dueDate).
                        if issue.dueDate != nil {
                            VStack(spacing: 0) {
                                detailRow(label: "Start time") {
                                    TimeFieldButton(
                                        value: issue.dueTime,
                                        placeholder: "—",
                                        onChange: { Task { await vm.setDueTime($0) } }
                                    )
                                }
                                Divider().background(Color.white.opacity(0.06))
                                detailRow(label: "End time") {
                                    TimeFieldButton(
                                        value: issue.endTime,
                                        placeholder: "—",
                                        onChange: { Task { await vm.setEndTime($0) } }
                                    )
                                }
                            }
                            .padding(.vertical, 4)
                            .glassSection()
                        }

                        // Recurrence
                        VStack(spacing: 0) {
                            detailRow(label: "Repeat") {
                                Menu {
                                    Button {
                                        Task { await vm.setRecurrence(interval: nil, unit: nil) }
                                    } label: {
                                        Label("Doesn't repeat", systemImage: "xmark")
                                    }
                                    ForEach(RecurrenceUnit.allCases) { unit in
                                        Section(unit.label(for: 2).capitalized) {
                                            ForEach(recurrenceIntervals, id: \.self) { interval in
                                                Button {
                                                    Task { await vm.setRecurrence(interval: interval, unit: unit) }
                                                } label: {
                                                    Text("Every \(interval) \(unit.label(for: interval))")
                                                }
                                            }
                                        }
                                    }
                                } label: {
                                    Text(formatRecurrence(issue.recurrenceInterval, issue.recurrenceUnit))
                                        .font(.subheadline)
                                        .foregroundStyle(
                                            issue.recurrenceInterval == nil
                                                ? .white.opacity(TextOpacity.tertiary)
                                                : .white
                                        )
                                }
                            }
                        }
                        .padding(.vertical, 4)
                        .glassSection()

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

    // Upload draft images, swap their placeholder URLs for the returned
    // real URLs, then save the description through the normal path. This
    // is the iOS analogue of Android's IssueDetailViewModel.uploadImage
    // + description rewrite pattern.
    private func uploadPendingAndSaveDescription(_ vm: IssueDetailViewModel) async {
        let drafts = pendingImages
        for (placeholder, image) in drafts {
            do {
                let uploaded = try await deps.issueImagesApi.upload(
                    issueId: issueId,
                    data: image.data,
                    filename: image.filename,
                    contentType: image.contentType
                )
                vm.editingDescription = MarkdownImageUtils.replaceImageUrl(
                    in: vm.editingDescription,
                    from: placeholder,
                    to: uploaded.url
                )
                pendingImages.removeValue(forKey: placeholder)
            } catch {
                vm.editingDescription = MarkdownImageUtils.stripUnknownDraftImages(
                    vm.editingDescription,
                    keep: Set(pendingImages.keys).subtracting([placeholder])
                )
                pendingImages.removeValue(forKey: placeholder)
            }
        }
        descriptionDirty = false
        await vm.saveDescription()
    }

    private func parseDate(_ dateString: String?) -> Date? {
        guard let dateString else { return nil }
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.date(from: dateString)
    }

}

// Inline time picker that surfaces the iOS wheel picker via a popover
// and a "Clear" affordance for nullable time values.
struct TimeFieldButton: View {
    let value: String?
    let placeholder: String
    let onChange: (String?) -> Void

    @State private var showPicker = false
    @State private var draft = Date()

    var body: some View {
        Button {
            draft = parseTime(value) ?? defaultTime()
            showPicker = true
        } label: {
            Text(value ?? placeholder)
                .font(.subheadline.monospacedDigit())
                .foregroundStyle(value == nil
                    ? .white.opacity(TextOpacity.tertiary)
                    : .white)
        }
        .popover(isPresented: $showPicker) {
            VStack(spacing: 12) {
                DatePicker("Time", selection: $draft, displayedComponents: [.hourAndMinute])
                    .datePickerStyle(.wheel)
                    .labelsHidden()
                HStack {
                    if value != nil {
                        Button("Clear") {
                            onChange(nil)
                            showPicker = false
                        }
                        .tint(.red)
                    }
                    Spacer()
                    Button("Cancel") { showPicker = false }
                    Button("Save") {
                        onChange(formatTime(draft))
                        showPicker = false
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .padding(16)
            .presentationCompactAdaptation(.popover)
        }
    }

    private func parseTime(_ value: String?) -> Date? {
        guard let value else { return nil }
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter.date(from: value)
    }

    private func formatTime(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"
        return formatter.string(from: date)
    }

    private func defaultTime() -> Date {
        var components = Calendar.current.dateComponents([.year, .month, .day], from: Date())
        components.hour = 9
        components.minute = 0
        return Calendar.current.date(from: components) ?? Date()
    }
}

private func formatRecurrence(_ interval: Int?, _ unit: String?) -> String {
    guard let interval, let unitRaw = unit, let unit = RecurrenceUnit(rawValue: unitRaw) else {
        return "Doesn't repeat"
    }
    if interval == 1 {
        switch unit {
        case .day: return "Daily"
        case .week: return "Weekly"
        case .month: return "Monthly"
        }
    }
    return "Every \(interval) \(unit.label(for: interval))"
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
