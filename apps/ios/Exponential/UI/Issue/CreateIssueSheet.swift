import SwiftUI

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
    @State private var dueTime = ""
    @State private var endTime = ""
    @State private var assigneeId: String?
    @State private var selectedLabelIds: Set<String> = []
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

                        }
                        .padding(16)
                        .glassSection()

                        // Due date — same inline picker as IssueDetailView
                        DueDatePicker(date: $dueDate)

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
            .onAppear { titleFocused = true }
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

        let input = CreateIssueInput(
            projectId: projectId,
            title: title,
            status: status.rawValue,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: description.isEmpty ? nil : IssueDescription(text: description),
            dueDate: dueDate.map { formatDate($0) },
            labelIds: selectedLabelIds.isEmpty ? nil : Array(selectedLabelIds)
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
