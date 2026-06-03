import ExpCore
import ExpUI
import SwiftUI

struct MacCreateIssueView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String
    let projectId: String
    let users: [UserEntity]
    var labels: [LabelEntity] = []
    var initialStatus: IssueStatus = .backlog
    var onCreated: () -> Void = {}

    @State private var title = ""
    @State private var status: IssueStatus = .backlog
    @State private var priority: IssuePriority = .none
    @State private var assigneeId: String?
    @State private var hasDueDate = false
    @State private var dueDate = Date()
    @State private var dueTime: String?
    @State private var endTime: String?
    @State private var recurrenceInterval: Int?
    @State private var recurrenceUnit: RecurrenceUnit?
    @State private var selectedLabelIds: Set<String> = []
    @State private var createMore = false
    @State private var showLabelPicker = false
    @State private var editor = IssueEditorModel()
    @State private var seeded = false
    @State private var loading = false
    @State private var error: String?

    private var baseURL: URL? { deps.auth.instanceBaseURL(forAccountId: accountId) }

    private var selectedLabels: [LabelEntity] {
        labels.filter { selectedLabelIds.contains($0.id) }.sorted { $0.name < $1.name }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("New Issue").font(.title3.weight(.semibold))

            TextField("Title", text: $title).textFieldStyle(.roundedBorder)

            HStack(spacing: 12) {
                Picker("Status", selection: $status) {
                    ForEach(IssueStatus.displayOrder, id: \.self) { Text($0.label).tag($0) }
                }
                Picker("Priority", selection: $priority) {
                    ForEach(IssuePriority.displayOrder, id: \.self) { Text($0.label).tag($0) }
                }
            }

            Picker("Assignee", selection: $assigneeId) {
                Text("Unassigned").tag(String?.none)
                ForEach(users) { Text($0.name ?? $0.email).tag(String?.some($0.id)) }
            }

            // Labels
            HStack(spacing: 8) {
                Text("Labels").frame(width: 64, alignment: .leading).foregroundStyle(.secondary)
                Button { showLabelPicker = true } label: { Label("Add", systemImage: "tag") }
                    .buttonStyle(.borderless)
                    .disabled(labels.isEmpty)
                    .popover(isPresented: $showLabelPicker, arrowEdge: .bottom) { labelPicker }
                ForEach(selectedLabels) { label in
                    HStack(spacing: 4) {
                        Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 8, height: 8)
                        Text(label.name).font(.caption)
                    }
                    .padding(.horizontal, 8).padding(.vertical, 3)
                    .glassButton()
                }
                Spacer(minLength: 0)
            }

            // Recurrence
            HStack(spacing: 8) {
                Text("Repeat").frame(width: 64, alignment: .leading).foregroundStyle(.secondary)
                MacRecurrenceMenu(interval: recurrenceInterval, unit: recurrenceUnit?.rawValue) { interval, unit in
                    recurrenceInterval = interval
                    recurrenceUnit = unit
                }
                Spacer()
            }

            // Due date + optional start/end times
            Toggle("Set due date", isOn: $hasDueDate)
            if hasDueDate {
                DatePicker("Due", selection: $dueDate, displayedComponents: [.date]).labelsHidden()
                HStack(spacing: 16) {
                    HStack(spacing: 6) {
                        Text("Start").foregroundStyle(.secondary)
                        MacTimeFieldButton(value: dueTime) { dueTime = $0 }
                    }
                    HStack(spacing: 6) {
                        Text("End").foregroundStyle(.secondary)
                        MacTimeFieldButton(value: endTime) { endTime = $0 }
                    }
                    Spacer()
                }
                .font(.subheadline)
            }

            Text("Description").font(.caption).foregroundStyle(.secondary)
            // Top-align so the toolbar sits directly under the label (the default
            // .center left an empty band above it), and grow from a compact height.
            MacMarkdownEditor(model: editor, baseURL: baseURL, accountId: accountId, httpClient: deps.httpClient)
                .frame(minHeight: 120, maxHeight: 320, alignment: .top)

            if let error { Text(error).foregroundStyle(.red).font(.callout) }

            HStack {
                Toggle("Create more", isOn: $createMore)
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") { Task { await create() } }
                    .buttonStyle(.borderedProminent)
                    .tint(Accent.indigo)
                    .disabled(loading || title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 540)
        .onAppear { if !seeded { status = initialStatus; seeded = true } }
    }

    @ViewBuilder
    private var labelPicker: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(labels.sorted { $0.name < $1.name }) { label in
                Button {
                    if selectedLabelIds.contains(label.id) { selectedLabelIds.remove(label.id) }
                    else { selectedLabelIds.insert(label.id) }
                } label: {
                    HStack(spacing: 8) {
                        Circle().fill(Color(hex: label.color) ?? .gray).frame(width: 9, height: 9)
                        Text(label.name)
                        Spacer()
                        if selectedLabelIds.contains(label.id) {
                            Image(systemName: "checkmark").foregroundStyle(.tint)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(12)
        .frame(width: 220)
    }

    private func create() async {
        loading = true
        error = nil
        let dateStr: String?
        if hasDueDate {
            let f = DateFormatter()
            f.dateFormat = "yyyy-MM-dd"
            f.locale = Locale(identifier: "en_US_POSIX")
            dateStr = f.string(from: dueDate)
        } else {
            dateStr = nil
        }
        // Strip draft images: they must be attached to an existing issue id, so
        // they're uploaded after creation (mirrors iOS CreateIssueSheet).
        let stripped = MarkdownImageUtils
            .stripUnknownDraftImages(editor.currentMarkdown(), keep: [])
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let input = CreateIssueInput(
            projectId: projectId,
            title: title.trimmingCharacters(in: .whitespacesAndNewlines),
            status: status.rawValue,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: stripped.isEmpty ? nil : IssueDescription(text: stripped),
            dueDate: dateStr,
            dueTime: dateStr == nil ? nil : dueTime,
            endTime: dateStr == nil ? nil : endTime,
            labelIds: selectedLabelIds.isEmpty ? nil : Array(selectedLabelIds),
            recurrenceInterval: recurrenceInterval,
            recurrenceUnit: recurrenceUnit?.rawValue
        )
        do {
            let createdId = try await deps.issuesApi.create(accountId: accountId, input)
            if !editor.pendingImages.isEmpty {
                let api = deps.issueImagesApi
                let acc = accountId
                let uploader: @Sendable (PendingImage) async throws -> String = { image in
                    let uploaded = try await api.upload(
                        accountId: acc, issueId: createdId,
                        data: image.data, filename: image.filename, contentType: image.contentType
                    )
                    return uploaded.url
                }
                let allUploaded = await editor.commitPendingImages(uploader: uploader)
                let finalMarkdown = editor.currentMarkdown()
                if allUploaded, !editor.hasUncommittedDrafts, finalMarkdown != stripped {
                    try await deps.issuesApi.update(
                        accountId: accountId,
                        UpdateIssueInput(id: createdId, description: finalMarkdown.isEmpty ? nil : IssueDescription(text: finalMarkdown))
                    )
                }
            }
            onCreated()
            if createMore {
                // Keep the sheet open and reset only the title + description, leaving
                // the chosen status/priority/assignee/labels/due/recurrence in place so
                // a batch of similar issues is quick to file (mirrors iOS CreateIssueSheet).
                title = ""
                editor = IssueEditorModel()
            } else {
                dismiss()
            }
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}
