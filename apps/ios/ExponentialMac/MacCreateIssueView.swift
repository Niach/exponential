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
    @State private var description = ""
    @State private var loading = false
    @State private var error: String?

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
            TextEditor(text: $description)
                .frame(minHeight: 100)
                .scrollContentBackground(.hidden)
                .padding(6)
                .glassRow()

            if let error { Text(error).foregroundStyle(.red).font(.callout) }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") { Task { await create() } }
                    .buttonStyle(.borderedProminent)
                    .disabled(loading || title.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 460)
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
        let trimmedDesc = description.trimmingCharacters(in: .whitespacesAndNewlines)
        let input = CreateIssueInput(
            projectId: projectId,
            title: title.trimmingCharacters(in: .whitespacesAndNewlines),
            status: status.rawValue,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: trimmedDesc.isEmpty ? nil : IssueDescription(text: trimmedDesc),
            dueDate: dateStr
        )
        do {
            _ = try await deps.issuesApi.create(accountId: accountId, input)
            onCreated()
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }
}
