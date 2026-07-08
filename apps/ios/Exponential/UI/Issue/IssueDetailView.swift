import ExpUI
import ExpCore
import SwiftUI
import GRDB

struct IssueDetailView: View {
    let issueId: String

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @State private var viewModel: IssueDetailViewModel?
    @State private var showDeleteConfirm = false
    @State private var showStatusPicker = false
    @State private var showPriorityPicker = false
    @State private var showAssigneePicker = false
    @State private var showRecurrencePicker = false
    @State private var showDuplicatePicker = false
    @State private var showCreateLabel = false
    @FocusState private var titleFocused: Bool

    var body: some View {
        ZStack {
            AppBackground()

            if let vm = viewModel, let issue = vm.issue {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        // Header: identifier + repo chip (actions live in the nav bar).
                        HStack(spacing: 6) {
                            if let identifier = issue.identifier {
                                Text(identifier)
                                    .font(.caption.monospaced().weight(.medium))
                                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                                    .padding(.horizontal, 8)
                                    .padding(.vertical, 4)
                                    .glassButton()
                            }
                            // Backing repo chip (v4 §6): the project's repositoryId
                            // resolved to owner/name via the repositories API.
                            if let project = vm.project, project.repositoryId != nil {
                                RepoNameChip(
                                    accountId: accountId,
                                    workspaceId: project.workspaceId,
                                    repositoryId: project.repositoryId
                                )
                            }
                            Spacer()
                        }

                        // Canonical-issue banner when marked as a duplicate:
                        // tap-through to the canonical issue + Unmark (§5e).
                        if let duplicateOfId = issue.duplicateOfId {
                            duplicateBanner(vm: vm, duplicateOfId: duplicateOfId)
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

                        // A remote edit arrived while editing locally — offer
                        // a non-blocking reload (field-level last-write-wins).
                        if vm.editor.pendingRemoteMarkdown != nil {
                            Button {
                                vm.reloadRemoteDescription()
                            } label: {
                                Label("Updated by someone else — Reload", systemImage: "arrow.triangle.2.circlepath")
                                    .font(.caption)
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 8)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .background(Color.blue.opacity(0.25), in: RoundedRectangle(cornerRadius: 8))
                            }
                            .buttonStyle(.plain)
                        }

                        // Description (block-based markdown editor with images)
                        MarkdownEditor(
                            model: vm.editor,
                            baseURL: instanceBaseURL,
                            accountId: accountId,
                            httpClient: deps.httpClient,
                            mentionMembers: vm.mentionMembers,
                            onIssueRefTap: { issueId in
                                // Route through the deep-link bus — MainNavigator
                                // observes it and pushes the issue route.
                                deps.deepLinkBus.navigateToIssue(issueId)
                            }
                        )

                        // Coding session: "Coding now" badge + live steer
                        // viewer / remote "Start on my desktop" (§5b/§5c).
                        SteerSessionSection(
                            issue: issue,
                            runningSessions: vm.runningSessions,
                            permissions: vm.permissions,
                            users: vm.users
                        )

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
                                    if let assigneeId = vm.issue?.assigneeId {
                                        Text(memberDisplayName(vm.assignee(), id: assigneeId))
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
                                    .disabled(!vm.permissions.isModerator)
                                }
                                // "+ Label" — create a new workspace label and
                                // assign it in one step (parity with Android).
                                if vm.permissions.isModerator {
                                    Button {
                                        showCreateLabel = true
                                    } label: {
                                        HStack(spacing: 4) {
                                            Image(systemName: "plus")
                                                .font(.caption2)
                                            Text("Label")
                                                .font(.caption)
                                        }
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                        .padding(.horizontal, 10)
                                        .padding(.vertical, 6)
                                        .glassButton()
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

                        // Changes (§4.8, mobile tiers 2–4): PR diff → pushed
                        // branch diff → "Being coded on <device>" steer state.
                        // Only shown when there's something to observe (parity
                        // with Android) — no empty "No changes yet." card.
                        if issue.prUrl?.isEmpty == false
                            || issue.branch?.isEmpty == false
                            || !vm.runningSessions.isEmpty {
                            ChangesSection(
                                issue: issue,
                                runningSessions: vm.runningSessions
                            )
                        }

                        // Attachments (read-only list synced from Electric).
                        // Inline images in the description still preview
                        // inside MarkdownEditor's preview tab — this section
                        // surfaces them as discoverable items.
                        AttachmentListView(issueId: issue.id)

                        // Comments
                        CommentThreadView(issue: issue)
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
                            // Duplicate = status interception (L27): picking
                            // `duplicate` opens the canonical-issue picker instead
                            // of writing the status directly; markDuplicate sets
                            // duplicateOfId + status='duplicate' atomically.
                            // Cancelling the picker leaves the status untouched.
                            // Defer so this sheet finishes dismissing first.
                            if selected == .duplicate {
                                DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                                    showDuplicatePicker = true
                                }
                            } else {
                                Task { await vm.setStatus(selected) }
                            }
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
                .sheet(isPresented: $showCreateLabel) {
                    CreateLabelSheet { name, color in
                        Task { await vm.createAndAssignLabel(name: name, color: color) }
                    }
                    .presentationDetents([.medium])
                    .presentationBackground(.ultraThinMaterial)
                }
                .sheet(isPresented: $showDuplicatePicker) {
                    DuplicatePickerSheet(
                        loadCandidates: { await vm.duplicateCandidates() },
                        onSelect: { canonical in
                            Task { await vm.markDuplicate(of: canonical) }
                        }
                    )
                    .presentationBackground(.ultraThinMaterial)
                }
                // Actions in the nav bar (parity with Android): share link +
                // subscribe bell (always) + a moderator-only overflow menu.
                .toolbar {
                    if let shareURL = vm.shareURL {
                        ToolbarItem(placement: .topBarTrailing) {
                            ShareLink(
                                item: shareURL,
                                subject: Text(vm.shareText),
                                message: Text(vm.shareText)
                            ) {
                                Image(systemName: "square.and.arrow.up")
                                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            }
                        }
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {
                            Task { await vm.toggleSubscribe() }
                        } label: {
                            Image(systemName: vm.isSubscribed ? "bell.fill" : "bell.slash")
                                .foregroundStyle(
                                    vm.isSubscribed
                                        ? Color.accentColor
                                        : .white.opacity(TextOpacity.secondary)
                                )
                        }
                    }
                    if vm.permissions.isModerator {
                        ToolbarItem(placement: .topBarTrailing) {
                            Menu {
                                // Duplicate = status interception (L27): unmark is
                                // the only duplicate action here; marking happens via
                                // the `duplicate` status picker.
                                if issue.duplicateOfId != nil {
                                    Button {
                                        Task { await vm.unmarkDuplicate() }
                                    } label: {
                                        Label("Unmark duplicate", systemImage: "doc.on.doc.fill")
                                    }
                                }
                                Button("Delete issue", role: .destructive) {
                                    showDeleteConfirm = true
                                }
                            } label: {
                                Image(systemName: "ellipsis.circle")
                            }
                        }
                    }
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
                    issueImagesApi: deps.issueImagesApi,
                    labelsApi: deps.labelsApi,
                    subscriptionsApi: deps.subscriptionsApi,
                    auth: deps.auth
                )
                viewModel = vm
                vm.startObserving()
            }
        }
        .onDisappear {
            if let vm = viewModel {
                Task {
                    await vm.saveTitle()
                    await vm.commitDescription()
                    vm.stopObserving()
                }
            }
        }
    }

    private var instanceBaseURL: URL? {
        deps.auth.instanceBaseURL(forAccountId: accountId)
    }

    /// "Duplicate of {IDENTIFIER}" — the identifier pill pushes the canonical
    /// issue's detail; Unmark clears the FK and restores a working status.
    @ViewBuilder
    private func duplicateBanner(vm: IssueDetailViewModel, duplicateOfId: String) -> some View {
        HStack(spacing: 8) {
            Image(systemName: "doc.on.doc")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            Text("Duplicate of")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            NavigationLink(value: AppRoute.issue(accountId: accountId, id: duplicateOfId)) {
                Text(vm.duplicateOf?.identifier ?? vm.duplicateOf?.title ?? "issue")
                    .font(.caption.monospaced().weight(.medium))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .glassButton(isActive: true)
            }
            .buttonStyle(.plain)
            Spacer()
            if vm.permissions.isModerator {
                Button {
                    Task { await vm.unmarkDuplicate() }
                } label: {
                    Text("Unmark")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                }
                .glassButton()
                .buttonStyle(.plain)
            }
        }
        .padding(10)
        .glassSection()
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
        return AppDateFormatters.yyyyMMdd.date(from: dateString)
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
        return AppDateFormatters.HHmm.date(from: value)
    }

    private func formatTime(_ date: Date) -> String {
        AppDateFormatters.HHmm.string(from: date)
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

// MARK: - Create label

// Same suggested palette as Android's LabelPickerSheet / the web's label editor.
private let suggestedLabelColors = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308",
    "#84cc16", "#22c55e", "#10b981", "#14b8a6", "#06b6d4",
    "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7",
    "#ec4899", "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
]

/// Minimal name + color form; Create makes the label and assigns it to the
/// issue in one step (mirrors Android's "New label" section).
private struct CreateLabelSheet: View {
    let onCreate: (String, String) -> Void

    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var color = suggestedLabelColors[0]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New label")
                .font(.headline)
                .foregroundStyle(.white)

            TextField("Label name", text: $name)
                .textFieldStyle(.plain)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Color.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 10))

            FlowLayout(spacing: 8) {
                ForEach(suggestedLabelColors, id: \.self) { swatch in
                    Button {
                        color = swatch
                    } label: {
                        Circle()
                            .fill(Color(hex: swatch) ?? .gray)
                            .frame(width: swatch == color ? 28 : 22, height: swatch == color ? 28 : 22)
                            .overlay {
                                if swatch == color {
                                    Circle().strokeBorder(.white, lineWidth: 2)
                                }
                            }
                    }
                    .buttonStyle(.plain)
                }
            }

            Spacer()

            Button {
                let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !trimmed.isEmpty else { return }
                onCreate(trimmed, color)
                dismiss()
            } label: {
                Text("Create label")
                    .font(.subheadline.weight(.medium))
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 12)
            }
            .buttonStyle(.borderedProminent)
            .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(20)
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
