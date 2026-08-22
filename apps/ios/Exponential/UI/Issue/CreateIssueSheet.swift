import ExpUI
import ExpCore
import SwiftUI
import GRDB
import UniformTypeIdentifiers

/// A file picked before the issue exists (EXP-327). Attachments need an issue
/// id, so — exactly like draft images — the bytes are held here and uploaded
/// right after the create.
private struct DraftFile: Identifiable, Sendable {
    let id = UUID()
    let filename: String
    let contentType: String
    let data: Data
}

private enum DraftFileReadFailure: Error {
    case unreadable
    case tooLarge
}

/// File-scope (never a view method) so the off-main read captures nothing but
/// the URL — a SwiftUI view struct isn't Sendable and must not ride into a
/// detached task. The size is checked before buffering: never read bytes the
/// cap is going to reject.
private func readDraftFileBytes(from url: URL) -> Result<Data, DraftFileReadFailure> {
    let scoped = url.startAccessingSecurityScopedResource()
    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
    if let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize,
       size > AttachmentFiles.maxFileUploadBytes {
        return .failure(.tooLarge)
    }
    guard let data = try? Data(contentsOf: url) else { return .failure(.unreadable) }
    guard data.count <= AttachmentFiles.maxFileUploadBytes else { return .failure(.tooLarge) }
    return .success(data)
}

struct CreateIssueSheet: View {
    let boardId: String
    /// The sheet is done with a created issue, carrying its id so the host can
    /// land on it (EXP-596). NOT called in "Create more" mode — the sheet stays
    /// up for the next issue, and a run of creates has no single destination.
    let onCreated: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss

    @State private var title = ""
    @State private var editor = IssueEditorModel()
    /// Files picked from the editor's attach menu, uploaded after the create.
    @State private var draftFiles: [DraftFile] = []
    /// EXP-314: the team's statuses in render order — the constructed builtin
    /// defaults until the `issue_statuses` rows load.
    @State private var teamStatuses: [ResolvedIssueStatus] = IssueStatusResolver.builtinFallbackTeam
    /// The picked status. Defaults to the team's backlog builtin.
    @State private var status: ResolvedIssueStatus = IssueStatusResolver.builtinDefault(for: .backlog)
    @State private var priority: IssuePriority = .none
    @State private var dueDate: Date?
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
    /// True once this sheet's issue was created but the sheet stayed up to
    /// report a failed attachment — Create is inert from then on, so the only
    /// way out is Cancel and no second issue can be filed.
    @State private var createCommitted = false
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
                            // EXP-327: the same attach menu as issue detail —
                            // images go into the description, other files
                            // become drafts uploaded once the issue exists.
                            onAttachFile: { url in ingestDraftFile(url) }
                        )

                        // Draft files, only once there is one (the section never
                        // announces its own emptiness — EXP-327).
                        if !draftFiles.isEmpty {
                            draftFilesSection
                        }

                        // Metadata + due date, one card (EXP-247): the due-date
                        // row (and, when set, the time rows) attach directly to
                        // the Status/Priority/Assignee card instead of floating
                        // as standalone sections.
                        VStack(spacing: 0) {
                            VStack(spacing: 12) {
                                // Status
                                metadataRow(label: "Status", icon: status.iconName, iconColor: status.color) {
                                    Button {
                                        showStatusPicker = true
                                    } label: {
                                        Text(status.name)
                                            .font(.subheadline)
                                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                                    }
                                    .buttonStyle(.plain)
                                }

                                // Priority
                                metadataRow(label: "Priority", icon: priority.iconName, iconColor: priority.color) {
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
                                        AppIcon(AppIcons.uiAdd, size: 11)
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
                // EXP-592: the description's `@`/`#`/`:` menu. In the safe-area
                // inset it rides above the keyboard; inside the editor it hung
                // off the end of the description, behind the keyboard and
                // clipped by this scroller. Focus-gated, so a candidate set the
                // user abandoned for the title field does not linger.
                .safeAreaInset(edge: .bottom) {
                    if editor.showsAutocompleteMenu {
                        EditorAutocompleteMenu(model: editor)
                            .padding(.horizontal, 20)
                            .padding(.bottom, 8)
                    }
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
                    .disabled(title.isEmpty || loading || createCommitted)
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
                    let team: TeamEntity? = (try? await pool.read({ db -> TeamEntity? in
                        guard let board = try BoardEntity.fetchOne(db, key: boardId) else {
                            return nil
                        }
                        return try TeamEntity.fetchOne(db, key: board.teamId)
                    })) ?? nil
                    teamId = team?.id
                    // Assignee/mention candidates are the TEAM's members, not
                    // the account-wide users store (EXP-487) — the pool can
                    // hold several teams' people.
                    if let wsId = team?.id,
                       let loaded = try? await pool.read({ db in
                           try teamMemberUsers(teamId: wsId, db: db)
                       }) {
                        users = loaded
                    }
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
                    // Statuses are team-scoped like labels (EXP-314); keep the
                    // constructed defaults until the rows land, and re-pin the
                    // default pick to the team's own backlog builtin.
                    if let wsId = team?.id,
                       let loadedStatuses = try? await pool.read({ db in
                           try IssueStatusEntity
                               .filter(Column("team_id") == wsId)
                               .fetchAll(db)
                       }) {
                        let resolved = IssueStatusResolver.teamStatusesOrFallback(loadedStatuses)
                        teamStatuses = resolved
                        if let backlog = resolved.first(where: { $0.builtinKey == .backlog })
                            ?? resolved.first(where: { $0.category == .backlog }) {
                            status = backlog
                        }
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
                    // Duplicate CATEGORY = status interception (L27): a new issue
                    // can't be a duplicate (nothing to link yet), so it's not a
                    // create option. The team's own status order — the ONE picker
                    // vocabulary (REV2-85), same as the filter sheet.
                    items: teamStatuses.filter { $0.category != .duplicate },
                    selectedID: status.id,
                    idFor: { $0.id },
                    onSelect: { status = $0 }
                ) { s in
                    Label {
                        Text(s.name)
                    } icon: {
                        AppIcon(s.iconName, size: AppIcon.Size.medium)
                            .foregroundStyle(s.color)
                    }
                }
            }
            .sheet(isPresented: $showPriorityPicker) {
                PickerSheet(
                    title: "Priority",
                    items: IssuePriority.displayOrder,
                    selectedID: priority.id,
                    idFor: { $0.id },
                    onSelect: { priority = $0 }
                ) { p in
                    Label {
                        Text(p.label)
                    } icon: {
                        AppIcon(p.iconName, size: AppIcon.Size.medium)
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
                        Label {
                            Text("Unassigned")
                        } icon: {
                            AppIcon(AppIcons.uiUnassigned, size: AppIcon.Size.medium)
                        }
                    } else {
                        Label {
                            Text(option.displayName)
                        } icon: {
                            AppIcon(AppIcons.uiAssignee, size: AppIcon.Size.medium)
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
            AppIcon(icon, size: AppIcon.Size.small)
                .foregroundStyle(iconColor)
                .frame(width: 20)

            Text(label)
                .font(.subheadline)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))

            Spacer()

            content()
        }
    }

    // MARK: - Draft files (EXP-327)

    private var draftFilesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Files")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            VStack(spacing: 6) {
                ForEach(draftFiles) { file in
                    HStack(spacing: 10) {
                        Image(systemName: AttachmentFiles.sfSymbolName(forContentType: file.contentType))
                            .font(.system(size: 15))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .frame(width: 20)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(file.filename)
                                .font(.callout)
                                .foregroundStyle(.white)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Text(Int64(file.data.count).formatted(.byteCount(style: .file)))
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Spacer(minLength: 8)
                        Button {
                            draftFiles.removeAll { $0.id == file.id }
                        } label: {
                            AppIcon(AppIcons.uiClose, size: AppIcon.Size.small)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Remove \(file.filename)")
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .glassRow()
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    /// Read a picked file off-main inside its security scope (a 50 MB pick from
    /// a cloud provider streams over the network) and hold it until the issue
    /// exists. Inline images never reach here — the editor appends those to the
    /// description itself.
    private func ingestDraftFile(_ url: URL) {
        let filename = AttachmentFiles.sanitizedFilename(url.lastPathComponent)
        let contentType = AttachmentFiles.canonicalContentType(
            UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
        )
        Task {
            switch await Task.detached(operation: { readDraftFileBytes(from: url) }).value {
            case let .success(data):
                draftFiles.append(
                    DraftFile(filename: filename, contentType: contentType, data: data)
                )
            case .failure(.tooLarge):
                error = "Files must be 50 MB or smaller."
            case .failure(.unreadable):
                error = "Couldn't read \(filename)."
            }
        }
    }

    /// Upload the held drafts against the now-existing issue. The issue is
    /// already committed, so a rejected attachment surfaces as an error but
    /// never turns a successful create into a failure. Returns false when any
    /// file failed, so the caller can hold the sheet open — dismissing right
    /// after setting `error` unmounted the only report the user ever gets, and
    /// the file vanished silently.
    private func uploadDraftFiles(issueId: String) async -> Bool {
        var failed: [String] = []
        for file in draftFiles {
            do {
                _ = try await deps.attachmentsApi.upload(
                    accountId: accountId,
                    issueId: issueId,
                    data: file.data,
                    filename: file.filename,
                    contentType: file.contentType
                )
            } catch {
                failed.append(file.filename)
            }
        }
        guard failed.isEmpty else {
            self.error = "Issue created, but couldn't attach \(failed.joined(separator: ", "))."
            return false
        }
        return true
    }

    /// Mirror the freshly-created row (and its label joins) into the local
    /// store instead of waiting for the Electric long-poll, so the issue the
    /// sheet lands on (EXP-596) renders immediately — IssueDetail's own
    /// fallback for an unsynced row is a 2s spinner and a server read (EXP-264).
    /// Best-effort and idempotent, exactly like Android's (EXP-19): sync
    /// re-delivers the same row and overwrites this one.
    private func mirrorCreatedIssue(
        _ created: IssueCreateResult,
        description: String?,
        labelIds: Set<String>
    ) async {
        // A row this build couldn't decode is simply not mirrored — the create
        // itself already succeeded.
        guard let fetched = created.issue,
              let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let entity = fetched.entity().replacingDescription(description)
        // `issue_labels` carries its team denormalized; without the team (the
        // board's row hasn't synced) the joins wait for sync like before.
        let labelRows = teamId.map { id in
            labelIds.map { IssueLabelEntity(issueId: entity.id, labelId: $0, teamId: id) }
        } ?? []
        try? await pool.write { db in
            try entity.save(db)
            for row in labelRows {
                try row.save(db)
            }
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
            // A CONSTRUCTED default (statuses shape not synced) has no row id,
            // so it falls back to the anchor enum (EXP-314).
            status: status.rowId == nil ? status.anchor.rawValue : nil,
            statusId: status.rowId,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: stripped.isEmpty ? nil : stripped,
            dueDate: dateStr,
            labelIds: validLabelIds.isEmpty ? nil : Array(validLabelIds)
        )

        do {
            let created = try await deps.issuesApi.create(accountId: accountId, input)
            let createdId = created.id
            // What the issue's description ends up as — the create above sent
            // the image-stripped markdown, the patch below may replace it.
            var finalDescription = stripped.isEmpty ? nil : stripped

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
                    finalDescription = finalMarkdown.isEmpty ? nil : finalMarkdown
                }
            }

            // Draft files last: the issue is committed, so a failed attachment
            // is reported but never fails the create (EXP-327).
            var draftsUploaded = true
            if !draftFiles.isEmpty {
                draftsUploaded = await uploadDraftFiles(issueId: createdId)
            }

            // Remember the board so the Share Extension defaults its picker to it.
            SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)

            await mirrorCreatedIssue(
                created,
                description: finalDescription,
                labelIds: validLabelIds
            )

            if createMore {
                title = ""
                editor = IssueEditorModel()
                draftFiles = []
                selectedLabelIds = []
                configureEditor()
                titleFocused = true
                // No hand-off: a run of creates has no single issue to land on,
                // and the sheet stays up for the next one.
            } else if draftsUploaded {
                onCreated(createdId)
                dismiss()
            } else {
                // The issue exists; only an attachment failed. Hold the sheet
                // so the error is actually read, and latch the create so
                // acknowledging it can't file a duplicate issue. Cancelling it
                // still lands on the issue — it is real, and the attachment can
                // be retried there.
                createCommitted = true
                onCreated(createdId)
            }
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
            IssueRefChipCache.chip(identifier, scope: .board(id: boardId), db: deps.db, accountId: accountId)?
                .issueId
        }
        editor.issueRefTitleResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: .board(id: boardId), db: deps.db, accountId: accountId)?
                .title
        }
        // The chip's status glyph, painted over its `#` (EXP-423).
        editor.issueRefStatusResolver = { identifier in
            IssueRefChipCache.statusInfo(
                identifier, scope: .board(id: boardId), db: deps.db, accountId: accountId)
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
