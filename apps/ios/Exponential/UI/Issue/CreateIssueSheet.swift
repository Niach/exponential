import SwiftUI
import GRDB

struct CreateIssueSheet: View {
    let projectId: String
    let onCreated: () -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
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
    @State private var pendingImages: [String: PendingImage] = [:]
    @State private var createMore = false
    @State private var loading = false
    @State private var error: String?
    @State private var permissions: WorkspacePermissions = .denied
    @State private var showStatusPicker = false
    @State private var showPriorityPicker = false
    @State private var showAssigneePicker = false
    @State private var showRecurrencePicker = false
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

                        // Description (markdown editor with image upload)
                        MarkdownEditor(
                            text: $description,
                            pendingImages: $pendingImages
                        )

                        // Metadata row
                        VStack(spacing: 12) {
                            // Status
                            metadataRow(label: "Status", icon: status.sfSymbol, iconColor: status.color) {
                                Button {
                                    showStatusPicker = true
                                } label: {
                                    Text(status.label)
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                                .buttonStyle(.plain)
                                .disabled(!permissions.isModerator)
                            }

                            // Priority
                            metadataRow(label: "Priority", icon: priority.sfSymbol, iconColor: priority.color) {
                                Button {
                                    showPriorityPicker = true
                                } label: {
                                    Text(priority.label)
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                                .buttonStyle(.plain)
                                .disabled(!permissions.isModerator)
                            }

                            // Assignee
                            metadataRow(label: "Assignee", icon: "person.circle", iconColor: .white.opacity(0.6)) {
                                Button {
                                    showAssigneePicker = true
                                } label: {
                                    let assignee = users.first { $0.id == assigneeId }
                                    Text(assignee?.name ?? assignee?.email ?? "Unassigned")
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                                .buttonStyle(.plain)
                                .disabled(!permissions.isModerator)
                            }

                            // Recurrence
                            metadataRow(label: "Repeat", icon: "repeat", iconColor: .white.opacity(0.6)) {
                                Button {
                                    showRecurrencePicker = true
                                } label: {
                                    Text(formatCreateRecurrence(recurrenceInterval, recurrenceUnit))
                                        .font(.subheadline)
                                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                }
                                .buttonStyle(.plain)
                                .disabled(!permissions.isModerator)
                            }

                        }
                        .padding(16)
                        .glassSection()
                        .opacity(permissions.isModerator ? 1 : 0.55)

                        // Due date — same inline picker as IssueDetailView
                        DueDatePicker(date: $dueDate)
                            .disabled(!permissions.isModerator)
                            .opacity(permissions.isModerator ? 1 : 0.55)

                        // Times (only when a due date is selected)
                        if dueDate != nil {
                            VStack(spacing: 12) {
                                metadataRow(label: "Start time", icon: "clock", iconColor: .white.opacity(0.6)) {
                                    TimeFieldButton(
                                        value: dueTime,
                                        placeholder: "—",
                                        onChange: { dueTime = $0 }
                                    )
                                    .disabled(!permissions.isModerator)
                                }
                                metadataRow(label: "End time", icon: "clock.badge", iconColor: .white.opacity(0.6)) {
                                    TimeFieldButton(
                                        value: endTime,
                                        placeholder: "—",
                                        onChange: { endTime = $0 }
                                    )
                                    .disabled(!permissions.isModerator)
                                }
                            }
                            .padding(16)
                            .glassSection()
                            .opacity(permissions.isModerator ? 1 : 0.55)
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
                    let pool = try! deps.db.pool(forAccountId: accountId)
                    if let loaded = try? await pool.read({ db in
                        try UserEntity.fetchAll(db)
                    }) {
                        users = loaded
                    }
                    let workspace: WorkspaceEntity? = (try? await pool.read({ db -> WorkspaceEntity? in
                        guard let project = try ProjectEntity.fetchOne(db, key: projectId) else {
                            return nil
                        }
                        return try WorkspaceEntity.fetchOne(db, key: project.workspaceId)
                    })) ?? nil
                    permissions = WorkspacePermissions.resolve(
                        workspace: workspace,
                        currentUserId: deps.auth.userId,
                        isAdmin: deps.auth.isAdmin,
                        dbPool: pool
                    )
                }
            }
            .sheet(isPresented: $showStatusPicker) {
                PickerSheet(
                    title: "Status",
                    items: IssueStatus.allCases,
                    selectedID: status.id,
                    idFor: { $0.id },
                    onSelect: { status = $0 }
                ) { s in
                    Label {
                        Text(s.label)
                    } icon: {
                        Image(systemName: s.sfSymbol)
                            .foregroundStyle(s.color)
                    }
                }
            }
            .sheet(isPresented: $showPriorityPicker) {
                PickerSheet(
                    title: "Priority",
                    items: IssuePriority.allCases,
                    selectedID: priority.id,
                    idFor: { $0.id },
                    onSelect: { priority = $0 }
                ) { p in
                    Label {
                        Text(p.label)
                    } icon: {
                        Image(systemName: p.sfSymbol)
                            .foregroundStyle(p.color)
                    }
                }
            }
            .sheet(isPresented: $showAssigneePicker) {
                PickerSheet(
                    title: "Assignee",
                    items: assigneeOptions(users: users),
                    selectedID: assigneeId ?? AssigneeOption.unassigned.id,
                    idFor: { $0.id },
                    onSelect: { assigneeId = $0.userId }
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
                    currentInterval: recurrenceInterval,
                    currentUnit: recurrenceUnit?.rawValue,
                    onSelect: { interval, unit in
                        recurrenceInterval = interval
                        recurrenceUnit = unit
                    }
                )
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

        // The server rejects markdown images on creation (they have to be
        // associated with an existing issue id). Strip drafts from the
        // initial create payload; we'll upload + restore them right after
        // the issue exists.
        let stripped = MarkdownImageUtils.stripUnknownDraftImages(
            description,
            keep: []
        ).trimmingCharacters(in: .whitespacesAndNewlines)

        let input = CreateIssueInput(
            projectId: projectId,
            title: title,
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

            // Upload any drafts, swap their URLs into the original
            // description, and patch the issue with the final markdown.
            if !pendingImages.isEmpty {
                var finalDescription = description
                var stillPending = pendingImages
                for (placeholder, image) in pendingImages {
                    do {
                        let uploaded = try await deps.issueImagesApi.upload(
                            accountId: accountId,
                            issueId: createdId,
                            data: image.data,
                            filename: image.filename,
                            contentType: image.contentType
                        )
                        finalDescription = MarkdownImageUtils.replaceImageUrl(
                            in: finalDescription,
                            from: placeholder,
                            to: uploaded.url
                        )
                        stillPending.removeValue(forKey: placeholder)
                    } catch {
                        stillPending.removeValue(forKey: placeholder)
                    }
                }
                finalDescription = MarkdownImageUtils.stripUnknownDraftImages(
                    finalDescription,
                    keep: []
                )
                if finalDescription != stripped {
                    try await deps.issuesApi.update(
                        accountId: accountId,
                        UpdateIssueInput(
                            id: createdId,
                            description: IssueDescription(text: finalDescription)
                        )
                    )
                }
            }

            if createMore {
                title = ""
                description = ""
                pendingImages = [:]
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
