import SwiftUI
import GRDB

struct CreateIssueSheet: View {
    let projectId: String
    let onCreated: () -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss

    @State private var title = ""
    @State private var description = ""
    @State private var status: IssueStatus = .backlog
    @State private var priority: IssuePriority = .none
    @State private var dueDate: Date?
    @State private var dueTime: String?
    @State private var endTime: String?
    @State private var assigneeId: String?
    @State private var recurrenceInterval: Int?
    @State private var recurrenceUnit: RecurrenceUnit?
    @State private var selectedLabelIds: Set<String> = []
    @State private var users: [UserEntity] = []
    @State private var createMore = false
    @State private var loading = false
    @State private var error: String?
    @FocusState private var titleFocused: Bool

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()

                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        // Title
                        TextField("Issue title", text: $title)
                            .font(.title3.weight(.medium))
                            .textFieldStyle(.plain)
                            .foregroundStyle(.white)
                            .focused($titleFocused)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                            .background(Color.white.opacity(0.04))
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                            .overlay(
                                RoundedRectangle(cornerRadius: 10)
                                    .stroke(Color.white.opacity(0.08), lineWidth: 0.5)
                            )

                        // Description
                        TextField("Add description...", text: $description, axis: .vertical)
                            .font(.body)
                            .textFieldStyle(.plain)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(3...8)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                            .background(Color.white.opacity(0.04))
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                            .overlay(
                                RoundedRectangle(cornerRadius: 10)
                                    .stroke(Color.white.opacity(0.08), lineWidth: 0.5)
                            )

                        // Metadata row
                        VStack(spacing: 12) {
                            // Status
                            metadataRow(label: "Status", icon: status.sfSymbol, iconColor: status.color) {
                                Menu {
                                    ForEach(IssueStatus.allCases) { s in
                                        Button {
                                            status = s
                                        } label: {
                                            Label(s.label, systemImage: s.sfSymbol)
                                        }
                                    }
                                } label: {
                                    Text(status.label)
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                            }

                            // Priority
                            metadataRow(label: "Priority", icon: priority.sfSymbol, iconColor: priority.color) {
                                Menu {
                                    ForEach(IssuePriority.allCases) { p in
                                        Button {
                                            priority = p
                                        } label: {
                                            Label(p.label, systemImage: p.sfSymbol)
                                        }
                                    }
                                } label: {
                                    Text(priority.label)
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                            }

                            // Assignee
                            metadataRow(label: "Assignee", icon: "person.circle", iconColor: .white.opacity(0.6)) {
                                Menu {
                                    Button {
                                        assigneeId = nil
                                    } label: {
                                        Label("Unassigned", systemImage: "xmark")
                                    }
                                    ForEach(users, id: \.id) { user in
                                        Button { assigneeId = user.id } label: {
                                            Text(user.name ?? user.email)
                                        }
                                    }
                                } label: {
                                    let assignee = users.first { $0.id == assigneeId }
                                    Text(assignee?.name ?? assignee?.email ?? "Unassigned")
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                            }

                            // Recurrence
                            metadataRow(label: "Repeat", icon: "repeat", iconColor: .white.opacity(0.6)) {
                                Menu {
                                    Button {
                                        recurrenceInterval = nil
                                        recurrenceUnit = nil
                                    } label: {
                                        Label("Doesn't repeat", systemImage: "xmark")
                                    }
                                    ForEach(RecurrenceUnit.allCases) { unit in
                                        Section(unit.label(for: 2).capitalized) {
                                            ForEach(recurrenceIntervals, id: \.self) { interval in
                                                Button {
                                                    recurrenceInterval = interval
                                                    recurrenceUnit = unit
                                                } label: {
                                                    Text("Every \(interval) \(unit.label(for: interval))")
                                                }
                                            }
                                        }
                                    }
                                } label: {
                                    Text(formatCreateRecurrence(recurrenceInterval, recurrenceUnit))
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                            }

                        }
                        .padding(16)
                        .glassSection()

                        // Due date — same inline picker as IssueDetailView
                        DueDatePicker(date: $dueDate)

                        // Times (only when a due date is selected)
                        if dueDate != nil {
                            VStack(spacing: 12) {
                                metadataRow(label: "Start time", icon: "clock", iconColor: .white.opacity(0.6)) {
                                    TimeFieldButton(
                                        value: dueTime,
                                        placeholder: "—",
                                        onChange: { dueTime = $0 }
                                    )
                                }
                                metadataRow(label: "End time", icon: "clock.badge", iconColor: .white.opacity(0.6)) {
                                    TimeFieldButton(
                                        value: endTime,
                                        placeholder: "—",
                                        onChange: { endTime = $0 }
                                    )
                                }
                            }
                            .padding(16)
                            .glassSection()
                        }

                        // Create more toggle
                        Toggle(isOn: $createMore) {
                            Text("Create more")
                                .font(.subheadline)
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        }
                        .tint(.blue)
                        .padding(.horizontal, 4)

                        if let error {
                            Text(error)
                                .font(.callout)
                                .foregroundStyle(.red)
                        }
                    }
                    .padding(20)
                }
            }
            .navigationTitle("New Issue")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task { await createIssue() }
                    } label: {
                        if loading {
                            ProgressView().tint(.white)
                        } else {
                            Text("Create")
                                .fontWeight(.medium)
                        }
                    }
                    .disabled(title.isEmpty || loading)
                }
            }
            .onAppear {
                titleFocused = true
                Task {
                    if let loaded = try? await deps.db.dbPool.read({ db in
                        try UserEntity.fetchAll(db)
                    }) {
                        users = loaded
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func metadataRow<Content: View>(label: String, icon: String, iconColor: Color, @ViewBuilder content: () -> Content) -> some View {
        HStack {
            Image(systemName: icon)
                .font(.caption)
                .foregroundStyle(iconColor)
                .frame(width: 20)

            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Spacer()

            content()
        }
    }

    private func createIssue() async {
        loading = true
        error = nil

        let dateStr = dueDate.map { formatDate($0) }
        let input = CreateIssueInput(
            projectId: projectId,
            title: title,
            status: status.rawValue,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: description.isEmpty ? nil : IssueDescription(text: description),
            dueDate: dateStr,
            dueTime: dateStr == nil ? nil : dueTime,
            endTime: dateStr == nil ? nil : endTime,
            labelIds: selectedLabelIds.isEmpty ? nil : Array(selectedLabelIds),
            recurrenceInterval: recurrenceInterval,
            recurrenceUnit: recurrenceUnit?.rawValue
        )

        do {
            _ = try await deps.issuesApi.create(input)
            if createMore {
                title = ""
                description = ""
                titleFocused = true
            } else {
                dismiss()
            }
            onCreated()
        } catch {
            self.error = error.localizedDescription
        }
        loading = false
    }

    private func formatDate(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }
}

private func formatCreateRecurrence(_ interval: Int?, _ unit: RecurrenceUnit?) -> String {
    guard let interval, let unit else { return "Doesn't repeat" }
    if interval == 1 {
        switch unit {
        case .day: return "Daily"
        case .week: return "Weekly"
        case .month: return "Monthly"
        }
    }
    return "Every \(interval) \(unit.label(for: interval))"
}
