import ExpUI
import ExpCore
import SwiftUI
import GRDB

struct CreateIssueSheet: View {
    let boardId: String
    let onCreated: () -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    @State private var title = ""
    @State private var editor = IssueEditorModel()
    @State private var status: IssueStatus = .backlog
    @State private var priority: IssuePriority = .none
    @State private var dueDate: Date?
    @State private var dueTime: String?
    @State private var endTime: String?
    @State private var assigneeId: String?
    @State private var selectedLabelIds: Set<String> = []
    @State private var labels: [LabelEntity] = []
    @State private var teamId: String?
    @State private var users: [UserEntity] = []
    /// True when the selected team has exactly one human member (the
    /// creator): the assignee picker is hidden and assigneeId is pre-set to
    /// that member (EXP-50). Multi-member teams keep the picker.
    @State private var singleMemberTeam = false
    @State private var createMore = false
    @State private var loading = false
    @State private var error: String?
    @State private var permissions: TeamPermissions = .denied
    @State private var showStatusPicker = false
    @State private var showPriorityPicker = false
    @State private var showAssigneePicker = false
    @State private var showCreateLabel = false
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
                            .accessibilityIdentifier("issue-title-field")
                            .padding(.horizontal, 16)
                            .padding(.vertical, 12)
                            .background(Color.white.opacity(0.04))
                            .clipShape(RoundedRectangle(cornerRadius: 10))
                            .overlay(
                                RoundedRectangle(cornerRadius: 10)
                                    .stroke(Color.white.opacity(0.08), lineWidth: 0.5)
                            )

                        // Description (block-based markdown editor with images)
                        MarkdownEditor(
                            model: editor,
                            baseURL: instanceBaseURL,
                            accountId: accountId,
                            httpClient: deps.httpClient,
                            mentionMembers: users.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) },
                            showsMentionButton: !singleMemberTeam
                        )

                        // Metadata + due date, one card (EXP-247): the due-date
                        // row (and, when set, the time rows) attach directly to
                        // the Status/Priority/Assignee card instead of floating
                        // as standalone sections.
                        VStack(spacing: 0) {
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
                                }

                                // Assignee — hidden on solo teams, where the
                                // sole member is pre-assigned (EXP-50).
                                if !singleMemberTeam {
                                    metadataRow(label: "Assignee", icon: "person.circle", iconColor: .white.opacity(0.6)) {
                                        Button {
                                            showAssigneePicker = true
                                        } label: {
                                            let assignee = users.first { $0.id == assigneeId }
                                            // memberDisplayName falls back to the email for a
                                            // blank name (name-less Apple logins); keep the
                                            // "Unassigned" sentinel when there is no assignee.
                                            Text(assignee.map { memberDisplayName($0, id: $0.id) } ?? "Unassigned")
                                                .font(.subheadline)
                                                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                        }
                                        .buttonStyle(.plain)
                                    }
                                }
                            }
                            .padding(16)

                            Divider().background(Color.white.opacity(0.06))

                            // Due date — embedded so it carries no card of its own.
                            DueDatePicker(date: $dueDate, embedded: true)

                            // Times (only when a due date is selected)
                            if dueDate != nil {
                                Divider().background(Color.white.opacity(0.06))
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
                            }
                        }
                        .glassSection()
                        .opacity(permissions.isModerator ? 1 : 0.55)
                        .disabled(!permissions.isModerator)

                        // Labels — all team labels as colored-dot toggle
                        // chips + a "+ Label" chip (parity with Android's
                        // CreateIssueScreen and the web create dialog). Toggling
                        // only flips a local selection; the issue doesn't exist
                        // yet, so labelIds rides along on the create call. Not
                        // moderator-gated: issues.create lets any creator set
                        // title/description/labels.
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Labels")
                                .font(.subheadline.weight(.medium))
                                .foregroundStyle(.white.opacity(TextOpacity.secondary))

                            FlowLayout(spacing: 6) {
                                ForEach(labels, id: \.id) { label in
                                    Button {
                                        if selectedLabelIds.contains(label.id) {
                                            selectedLabelIds.remove(label.id)
                                        } else {
                                            selectedLabelIds.insert(label.id)
                                        }
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
                                        .glassButton(isActive: selectedLabelIds.contains(label.id))
                                    }
                                    .buttonStyle(.plain)
                                }
                                // "+ Label" — create a new team label and
                                // pre-select it on this draft in one step.
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
                    // Tap-outside keyboard dismissal (EXP-246): catcher BEHIND
                    // the content — only dead-space taps reach it, interactive
                    // children keep winning hit-testing.
                    .background {
                        Color.clear
                            .contentShape(Rectangle())
                            .onTapGesture { UIApplication.endEditing() }
                    }
                }
                .scrollDismissesKeyboard(.interactively)
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
            // Presenting a picker over a focused editor kept the editor first
            // responder — its keyboard-accessory strip then floated over the
            // picker sheet (EXP-246). Resign before each picker lands.
            .onChange(of: showStatusPicker) { _, shown in
                if shown { UIApplication.endEditing() }
            }
            .onChange(of: showPriorityPicker) { _, shown in
                if shown { UIApplication.endEditing() }
            }
            .onChange(of: showAssigneePicker) { _, shown in
                if shown { UIApplication.endEditing() }
            }
            .onChange(of: showCreateLabel) { _, shown in
                if shown { UIApplication.endEditing() }
            }
            .onAppear {
                titleFocused = true
                configureEditor()
                Task {
                    guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
                    if let loaded = try? await pool.read({ db in
                        try UserEntity.fetchAll(db)
                    }) {
                        users = loaded
                    }
                    let team: TeamEntity? = (try? await pool.read({ db -> TeamEntity? in
                        guard let board = try BoardEntity.fetchOne(db, key: boardId) else {
                            return nil
                        }
                        return try TeamEntity.fetchOne(db, key: board.teamId)
                    })) ?? nil
                    teamId = team?.id
                    // Solo-team assignee shortcut (EXP-50): when this
                    // team has exactly one human member, hide the picker
                    // and pre-assign the creator. Scoped to the selected
                    // team — the pool can hold several.
                    if let wsId = team?.id,
                       let humanIds = try? await pool.read({ db in
                           try humanTeamMemberIds(teamId: wsId, db: db)
                       }), humanIds.count == 1 {
                        singleMemberTeam = true
                        assigneeId = humanIds.first
                    }
                    // Labels are team-scoped; a shared DB pool can hold more
                    // than one team, so filter to this board's team.
                    if let wsId = team?.id,
                       let loadedLabels = try? await pool.read({ db in
                           try LabelEntity
                               .filter(Column("team_id") == wsId)
                               .order(Column("name"))
                               .fetchAll(db)
                       }) {
                        labels = loadedLabels
                    }
                    permissions = TeamPermissions.resolve(
                        team: team,
                        currentUserId: deps.auth.userId,
                        isAdmin: deps.auth.isAdmin,
                        dbPool: pool
                    )
                }
            }
            .sheet(isPresented: $showStatusPicker) {
                PickerSheet(
                    title: "Status",
                    // Duplicate = status interception (L27): a new issue can't be a
                    // duplicate (nothing to link yet), so it's not a create option.
                    items: IssueStatus.allCases.filter { $0 != .duplicate },
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
            .sheet(isPresented: $showCreateLabel) {
                CreateLabelSheet { name, color in
                    Task { await createAndSelectLabel(name: name, color: color) }
                }
            }
        }
    }

    /// Create a team label and pre-select it on this draft. The label is
    /// real immediately (labels.create); only the assignment is deferred — the
    /// create call carries it via labelIds (parity with Android).
    private func createAndSelectLabel(name: String, color: String) async {
        guard let teamId else { return }
        do {
            let labelId = try await deps.labelsApi.create(
                accountId: accountId,
                CreateLabelInput(name: name, color: color, teamId: teamId)
            )
            selectedLabelIds.insert(labelId)
            // Reflect the new label in the chip row without waiting for a sync
            // round-trip; keep it name-ordered to match the initial load.
            if !labels.contains(where: { $0.id == labelId }) {
                labels.append(
                    LabelEntity(
                        id: labelId,
                        teamId: teamId,
                        name: name,
                        color: color,
                        sortOrder: nil,
                        createdAt: "",
                        updatedAt: ""
                    )
                )
                labels.sort { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
            }
        } catch {
            self.error = error.localizedDescription
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
        // associated with an existing issue id). Create with images stripped,
        // then upload + patch them in once the issue exists.
        let fullMarkdown = editor.currentMarkdown()
        let stripped = MarkdownImageUtils
            .stripUnknownDraftImages(fullMarkdown, keep: [])
            .trimmingCharacters(in: .whitespacesAndNewlines)

        // Drop selections for labels deleted while drafting — the server
        // rejects the whole create on an unknown label id (parity with Android).
        let validLabelIds = selectedLabelIds.filter { id in labels.contains { $0.id == id } }

        let input = CreateIssueInput(
            boardId: boardId,
            title: title,
            status: status.rawValue,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: stripped.isEmpty ? nil : stripped,
            dueDate: dateStr,
            dueTime: dateStr == nil ? nil : dueTime,
            endTime: dateStr == nil ? nil : endTime,
            labelIds: validLabelIds.isEmpty ? nil : Array(validLabelIds)
        )

        do {
            let createdId = try await deps.issuesApi.create(accountId: accountId, input)

            // Upload drafts atomically against the new issue id and patch the
            // final markdown (with real attachment URLs swapped in by block).
            if !editor.pendingImages.isEmpty {
                let api = deps.issueImagesApi
                let acc = accountId
                let uploader: @Sendable (PendingImage) async throws -> String = { image in
                    let uploaded = try await api.upload(
                        accountId: acc,
                        issueId: createdId,
                        data: image.data,
                        filename: image.filename,
                        contentType: image.contentType
                    )
                    return uploaded.url
                }
                let allUploaded = await editor.commitPendingImages(uploader: uploader)
                let finalMarkdown = editor.currentMarkdown()
                if allUploaded, !editor.hasUncommittedDrafts, finalMarkdown != stripped {
                    try await deps.issuesApi.update(
                        accountId: accountId,
                        UpdateIssueInput(
                            id: createdId,
                            description: finalMarkdown.isEmpty ? nil : finalMarkdown
                        )
                    )
                }
            }

            // Remember the board so the Share Extension defaults its picker to it.
            SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)

            if createMore {
                title = ""
                editor = IssueEditorModel()
                selectedLabelIds = []
                configureEditor()
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

    /// `#IDENTIFIER` refs resolve/search against the target board's team:
    /// pills for refs that resolve locally, and a #-autocomplete inserting the
    /// plain interchange token. Re-applied when "Create more" resets the model.
    private func configureEditor() {
        editor.issueRefResolver = { identifier in
            IssueRefLookup.resolve(identifier, scope: .board(id: boardId), db: deps.db, accountId: accountId)
        }
        editor.issueRefSearch = { query in
            IssueRefLookup.search(query, scope: .board(id: boardId), db: deps.db, accountId: accountId)
        }
    }

    private var instanceBaseURL: URL? {
        deps.auth.instanceBaseURL(forAccountId: accountId)
    }

    private func formatDate(_ date: Date) -> String {
        AppDateFormatters.yyyyMMdd.string(from: date)
    }
}
