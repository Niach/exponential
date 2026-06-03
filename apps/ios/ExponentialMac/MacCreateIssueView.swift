import ExpCore
import ExpUI
import SwiftUI

struct MacCreateIssueView: View {
    @Environment(MacAppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    let accountId: String
    let projectId: String
    let users: [UserEntity]
    var onCreated: () -> Void = {}

    @State private var title = ""
    @State private var status: IssueStatus = .backlog
    @State private var priority: IssuePriority = .none
    @State private var assigneeId: String?
    @State private var hasDueDate = false
    @State private var dueDate = Date()
    @State private var editor = IssueEditorModel()
    @State private var loading = false
    @State private var error: String?

    private var baseURL: URL? { deps.auth.instanceBaseURL(forAccountId: accountId) }

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

            Toggle("Set due date", isOn: $hasDueDate)
            if hasDueDate {
                DatePicker("Due", selection: $dueDate, displayedComponents: [.date]).labelsHidden()
            }

            Text("Description").font(.caption).foregroundStyle(.secondary)
            MacMarkdownEditor(model: editor, baseURL: baseURL, accountId: accountId, httpClient: deps.httpClient)
                .frame(minHeight: 160, maxHeight: 360)

            if let error { Text(error).foregroundStyle(.red).font(.callout) }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") { Task { await create() } }
                    .buttonStyle(.borderedProminent)
                    .tint(Accent.indigo)
                    .disabled(loading || title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 520)
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
            dueDate: dateStr
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
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}
