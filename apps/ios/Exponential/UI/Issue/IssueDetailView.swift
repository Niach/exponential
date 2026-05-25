import SwiftUI
import GRDB

struct IssueDetailView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: IssueDetailViewModel?
    @State private var showDeleteConfirm = false
    @State private var pendingImages: [String: PendingImage] = [:]
    @State private var descriptionDirty = false
    @State private var showStatusPicker = false
    @State private var showPriorityPicker = false
    @State private var showAssigneePicker = false
    @State private var showRecurrencePicker = false
    @FocusState private var titleFocused: Bool

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel, let issue = vm.issue {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        // Header: identifier + agent plan badge + overflow
                        HStack {
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced().weight(.medium))
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .glassButton()
                            }
                            planStateBadge(for: issue)
                            Spacer()
                            Menu {
                                Button(issue.archivedAt == nil ? "Archive" : "Unarchive") {
                                    Task { await vm.toggleArchive() }
                                }
                                Button("Delete issue", role: .destructive) {
                                    showDeleteConfirm = true
                                }
                            } label: {
                                Image(systemName: "ellipsis.circle")
                                    .font(.title3)
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
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
                                Button {
                                    showStatusPicker = true
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
                                .buttonStyle(.plain)
                                .disabled(!vm.permissions.isModerator)
                            }

                            Divider().background(Color.white.opacity(0.06))

                            // Priority
                            detailRow(label: "Priority") {
                                Button {
                                    showPriorityPicker = true
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
                                .buttonStyle(.plain)
                                .disabled(!vm.permissions.isModerator)
                            }

                            Divider().background(Color.white.opacity(0.06))

                            // Assignee
                            detailRow(label: "Assignee") {
                                Button {
                                    showAssigneePicker = true
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
                                .buttonStyle(.plain)
                                .disabled(!vm.permissions.isModerator)
                            }

                        }
                        .padding(.vertical, 4)
                        .glassSection()
                        .opacity(vm.permissions.isModerator ? 1 : 0.55)

                        // Due date — inline calendar
                        DueDatePicker(date: Binding(
                            get: { parseDate(issue.dueDate) },
                            set: { newDate in Task { await vm.setDueDate(newDate) } }
                        ))
                        .disabled(!vm.permissions.isModerator)
                        .opacity(vm.permissions.isModerator ? 1 : 0.55)

                        // Times (only when a due date is set; matches the
                        // server-side semantics where dueTime depends on dueDate).
                        if issue.dueDate != nil {
                            VStack(spacing: 0) {
                                detailRow(label: "Start time") {
                                    TimeFieldButton(
                                        value: issue.dueTime,
                                        placeholder: "—",
                                        onChange: { value in Task { await vm.setDueTime(value) } }
                                    )
                                    .disabled(!vm.permissions.isModerator)
                                }
                                Divider().background(Color.white.opacity(0.06))
                                detailRow(label: "End time") {
                                    TimeFieldButton(
                                        value: issue.endTime,
                                        placeholder: "—",
                                        onChange: { value in Task { await vm.setEndTime(value) } }
                                    )
                                    .disabled(!vm.permissions.isModerator)
                                }
                            }
                            .padding(.vertical, 4)
                            .glassSection()
                            .opacity(vm.permissions.isModerator ? 1 : 0.55)
                        }

                        // Recurrence
                        VStack(spacing: 0) {
                            detailRow(label: "Repeat") {
                                Button {
                                    showRecurrencePicker = true
                                } label: {
                                    Text(formatRecurrence(issue.recurrenceInterval, issue.recurrenceUnit))
                                        .font(.subheadline)
                                        .foregroundStyle(
                                            issue.recurrenceInterval == nil
                                                ? .white.opacity(TextOpacity.tertiary)
                                                : .white
                                        )
                                }
                                .buttonStyle(.plain)
                                .disabled(!vm.permissions.isModerator)
                            }
                        }
                        .padding(.vertical, 4)
                        .glassSection()
                        .opacity(vm.permissions.isModerator ? 1 : 0.55)

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

                        // Attachments (read-only list synced from Electric).
                        // Inline images in the description still preview
                        // inside MarkdownEditor's preview tab — this section
                        // surfaces them as discoverable items.
                        AttachmentListView(issueId: issue.id)

                        // Comments
                        CommentThreadView(
                            issue: issue,
                            canApprovePlan: vm.permissions.canApprovePlan(creatorId: issue.creatorId)
                        )
                    }
                    .padding(20)
                }
                .sheet(isPresented: $showStatusPicker) {
                    PickerSheet(
                        title: "Status",
                        items: IssueStatus.allCases,
                        selectedID: IssueStatus.from(issue.status).id,
                        idFor: { $0.id },
                        onSelect: { selected in
                            Task { await vm.setStatus(selected) }
                        }
                    ) { status in
                        Label {
                            Text(status.label)
                        } icon: {
                            Image(systemName: status.sfSymbol)
                                .foregroundStyle(status.color)
                        }
                    }
                }
                .sheet(isPresented: $showPriorityPicker) {
                    PickerSheet(
                        title: "Priority",
                        items: IssuePriority.allCases,
                        selectedID: IssuePriority.from(issue.priority).id,
                        idFor: { $0.id },
                        onSelect: { selected in
                            Task { await vm.setPriority(selected) }
                        }
                    ) { priority in
                        Label {
                            Text(priority.label)
                        } icon: {
                            Image(systemName: priority.sfSymbol)
                                .foregroundStyle(priority.color)
                        }
                    }
                }
                .sheet(isPresented: $showAssigneePicker) {
                    PickerSheet(
                        title: "Assignee",
                        items: assigneeOptions(users: vm.users),
                        selectedID: issue.assigneeId ?? AssigneeOption.unassigned.id,
                        idFor: { $0.id },
                        onSelect: { option in
                            Task { await vm.setAssignee(option.userId) }
                        }
                    ) { option in
                        if option.userId == nil {
                            Label("Unassigned", systemImage: "person.crop.circle.badge.xmark")
                        } else {
                            Label {
                                Text(option.displayName)
                            } icon: {
                                Image(systemName: "person.circle")
                            }
                        }
                    }
                }
                .sheet(isPresented: $showRecurrencePicker) {
                    RecurrencePickerSheet(
                        currentInterval: issue.recurrenceInterval,
                        currentUnit: issue.recurrenceUnit,
                        onSelect: { interval, unit in
                            Task { await vm.setRecurrence(interval: interval, unit: unit) }
                        }
                    )
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
                    accountId: accountId,
                    issueId: issueId,
                    db: deps.db,
                    issuesApi: deps.issuesApi,
                    labelsApi: deps.labelsApi,
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
                    accountId: accountId,
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

    // The compact pill under the identifier. Hidden when there's no agent
    // plan state to surface. Mirrors the state derivation in
    // apps/web/src/components/issue-timeline.tsx (~lines 421-446).
    @ViewBuilder
    private func planStateBadge(for issue: IssueEntity) -> some View {
        if let label = planStateLabel(issue: issue) {
            HStack(spacing: 4) {
                Image(systemName: label.symbol)
                    .font(.caption2)
                Text(label.text)
                    .font(.caption2.weight(.medium))
            }
            .foregroundStyle(label.color)
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .background(label.color.opacity(0.12), in: RoundedRectangle(cornerRadius: 6))
        }
    }

    private struct PlanLabel {
        let text: String
        let color: Color
        let symbol: String
    }

    private func planStateLabel(issue: IssueEntity) -> PlanLabel? {
        switch issue.agentPlanState {
        case "drafting":
            return PlanLabel(text: "Drafting", color: .orange, symbol: "pencil")
        case "awaiting_answer":
            return PlanLabel(text: "Awaiting answer", color: .purple, symbol: "questionmark.circle")
        case "awaiting_approval":
            return PlanLabel(text: "Awaiting approval", color: .blue, symbol: "hand.raised")
        case "approved":
            // Hide once status moves to done/cancelled — the approval no
            // longer needs surfacing for finished work.
            if issue.status == "done" || issue.status == "cancelled" {
                return nil
            }
            return PlanLabel(text: "Approved", color: .green, symbol: "checkmark.circle")
        default:
            return nil
        }
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
