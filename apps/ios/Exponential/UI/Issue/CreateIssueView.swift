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

/// The four pickers the page can present — ONE `.sheet(item:)` (EXP-240: four
/// `.sheet(isPresented:)` on one node means only the first ever presents).
private enum CreateIssuePicker: String, Identifiable {
    case status
    case priority
    case assignee
    case createLabel

    var id: String { rawValue }
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

/// The geometry of ONE property row on the New-issue page (EXP-698 r4). It
/// lives at file scope because `DueDatePicker`'s embedded row is one of those
/// rows and has to land on the same paddings and glyph size — a due date that
/// sits two points off the Assignee above it is exactly the drift this issue
/// went after.
enum CreateIssueRow {
    static let horizontalPadding: CGFloat = 16
    static let verticalPadding: CGFloat = 12
    /// The glyph leading the VALUE — smaller than the `.body` rung, because it
    /// reads with the `.subheadline` value beside it, not on its own.
    static let glyphSize: CGFloat = 14
}

/// The New-issue PAGE (EXP-687 — it used to be a sheet): back icon top-left,
/// `Create` top-right, exactly like Android's `CreateIssueScreen`.
struct CreateIssueView: View {
    let boardId: String
    /// The page is done: the created issue's id so the host can land on it
    /// (EXP-596), or nil when the draft was abandoned. NOT called in "Create
    /// more" mode — the page stays up for the next issue, and a run of creates
    /// has no single destination.
    let onFinish: (String?) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

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
    /// True once this page's issue was created but the page stayed up to
    /// report a failed attachment — Create is inert from then on, so the only
    /// way out is Back and no second issue can be filed.
    @State private var createCommitted = false
    @State private var loading = false
    @State private var error: String?
    @State private var permissions: TeamPermissions = .denied
    /// ONE presentation for the four pickers — four `.sheet(isPresented:)` on
    /// one node meant only the first ever presented (EXP-240).
    @State private var picker: CreateIssuePicker?
    /// Non-nil once this draft's issue exists (an attachment failed, so the
    /// page stayed up to report it) — Back then still lands on it.
    @State private var createdIssueId: String?
    @State private var confirmDiscard = false
    @FocusState private var titleFocused: Bool

    /// The title as it would be filed: a run of spaces is not a title, and
    /// leading/trailing whitespace never belongs in one.
    private var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Create is live only with a title, nothing in flight, and no issue
    /// already committed from this draft (an attachment failed and the page
    /// stayed up — a second Create would file a duplicate).
    private var canSubmit: Bool {
        !trimmedTitle.isEmpty && !loading && !createCommitted
    }

    /// True while there is unsaved work worth a confirmation.
    private var hasDraftContent: Bool {
        !trimmedTitle.isEmpty
            || !editor.currentMarkdown().trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || !editor.pendingImages.isEmpty
            || !draftFiles.isEmpty
    }

    var body: some View {
        ZStack {
            AppBackground()

            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // Title — the shared input (EXP-698 r4). It used to draw its
                    // own .04 fill and .08 hairline at radius 10, i.e. a
                    // near-miss of `GlassTextField`'s tokens; the recipe brings
                    // the focus-brightened stroke with it. The font and the
                    // focus binding stay the caller's, as every GlassTextField
                    // behaviour modifier does.
                    GlassTextField(
                        "Issue title",
                        text: $title,
                        accessibilityIdentifier: "issue-title-field"
                    )
                    .font(.title3.weight(.medium))
                    .focused($titleFocused)

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
                        // 120pt, matching Android's CreateIssueScreen
                        // (EXP-659 rewritten in EXP-698 r4): enough of a band
                        // to read as the description field, and short enough
                        // that the auto-focused title's keyboard still leaves
                        // the properties card, Labels and "Create more" on
                        // screen — which the 200pt issue-detail band did not.
                        minHeight: 120,
                        onAttachFile: { url in ingestDraftFile(url) }
                    )

                    // Draft files, only once there is one (the section never
                    // announces its own emptiness — EXP-327).
                    if !draftFiles.isEmpty {
                        draftFilesSection
                    }

                    // Metadata + due date, one card (EXP-247): the due-date
                    // row (and, when set, the calendar) attach directly to the
                    // Status/Priority/Assignee card instead of floating as
                    // standalone sections.
                    //
                    // EXP-698 r4 (Android CreateIssueScreen parity): the rows
                    // are hairline-separated FULL rows — label left, glyph +
                    // value right — not a 12pt stack of label/chevron lines.
                    // No leading gutter glyph and no trailing chevron: the
                    // property's own icon rides beside its value, which is
                    // where the eye reads it, and the whole row is the tap
                    // target instead of just the value.
                    VStack(spacing: 0) {
                        // Status
                        metadataRow(
                            label: "Status",
                            icon: status.iconName,
                            iconColor: status.color,
                            value: status.name
                        ) { picker = .status }

                        GlassDivider()

                        // Priority
                        metadataRow(
                            label: "Priority",
                            icon: priority.iconName,
                            iconColor: priority.color,
                            value: priority.label
                        ) { picker = .priority }

                        // Assignee — hidden on solo teams, where the
                        // sole member is pre-assigned (EXP-50).
                        if !singleMemberTeam {
                            GlassDivider()

                            // EXP-698: the registry's assignee concept —
                            // the raw `person.circle` SF Symbol here was
                            // not an `AppIcons` name, so the row simply
                            // rendered no glyph at all.
                            // memberDisplayName falls back to the email for a
                            // blank name (name-less Apple logins); keep the
                            // "Unassigned" sentinel when there is no assignee.
                            let assignee = users.first { $0.id == assigneeId }
                            metadataRow(
                                label: "Assignee",
                                icon: assigneeId == nil ? AppIcons.uiUnassigned : AppIcons.uiAssignee,
                                iconColor: .white.opacity(TextOpacity.secondary),
                                value: assignee.map { memberDisplayName($0, id: $0.id) } ?? "Unassigned"
                            ) { picker = .assignee }
                        }

                        GlassDivider()

                        // Due date — no card of its own, the same row
                        // geometry as the three above.
                        DueDatePicker(date: $dueDate)
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
                        // EXP-698 r4: a plain heading. The leading gutter glyph
                        // went away with the metadata rows' — a section title
                        // above a chip cloud needs no icon to be found.
                        Text("Labels")
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))

                        FlowLayout(spacing: 6) {
                            ForEach(labels, id: \.id) { label in
                                GlassPill(
                                    label.name,
                                    mode: .select(isSelected: selectedLabelIds.contains(label.id)) {
                                        if selectedLabelIds.contains(label.id) {
                                            selectedLabelIds.remove(label.id)
                                        } else {
                                            selectedLabelIds.insert(label.id)
                                        }
                                    },
                                    dot: Color(hex: label.color) ?? .gray
                                )
                            }
                            // "+ Label" — create a new team label and
                            // pre-select it on this draft in one step.
                            GlassPill("Label", icon: AppIcons.uiAdd, mode: .action {
                                picker = .createLabel
                            })
                        }
                    }

                    // Create more toggle
                    Toggle(isOn: $createMore) {
                        Text("Create more")
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
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
        // The page owns its own Back (Android parity): it has to run the
        // discard confirmation, which the system chevron cannot.
        .navigationBarBackButtonHidden(true)
        // EXP-698 r4: BARE toolbar content, both slots. iOS 26 paints its own
        // Liquid Glass capsule behind every toolbar item, so the drawn circle
        // (`TopBarBackButton`) and the drawn pill (`GlassPill`) sat inside a
        // second one — the "double round". A back glyph and a text button are
        // what the system chrome expects to wrap.
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button {
                    attemptClose()
                } label: {
                    // The board header's search/filter glyphs, exactly: 32pt of
                    // ink at secondary white, and the hit shape grown to the
                    // 44pt target the ink no longer fills on its own.
                    AppIcon(AppIcons.uiBack, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 32, height: 32)
                        .contentShape(Circle().inset(by: -GlassMenuTokens.triggerHitInset))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Back")
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button(loading ? "Creating…" : "Create") {
                    Task { await createIssue() }
                }
                .fontWeight(.semibold)
                .disabled(!canSubmit)
            }
        }
        .alert("Discard this issue?", isPresented: $confirmDiscard) {
            Button("Discard", role: .destructive) { onFinish(createdIssueId) }
            Button("Keep editing", role: .cancel) {}
        } message: {
            Text("Your title, description and attached images will be lost.")
        }
        // Presenting a picker over a focused editor kept the editor first
        // responder — its keyboard-accessory strip then floated over the
        // picker sheet (EXP-246). Resign before each picker lands.
        .onChange(of: picker) { _, shown in
            if shown != nil { UIApplication.endEditing() }
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
        .sheet(item: $picker) { target in
            pickerSheet(target)
        }
    }

    @ViewBuilder
    private func pickerSheet(_ target: CreateIssuePicker) -> some View {
        switch target {
        case .status:
            GlassPickerSheet(
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
        case .priority:
            GlassPickerSheet(
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
        case .assignee:
            GlassPickerSheet(
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
        case .createLabel:
            CreateLabelSheet { name, color in
                Task { await createAndSelectLabel(name: name, color: color) }
            }
        }
    }

    /// Back with unsaved work asks first (Android parity). A committed create
    /// (an attachment failed after the issue landed) still lands on the issue.
    private func attemptClose() {
        if createCommitted || !hasDraftContent {
            onFinish(createdIssueId)
        } else {
            confirmDiscard = true
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
            self.error = error.userFacingMessage
        }
    }

    /// One property row of the card: a secondary label on the left, and the
    /// value on the right led by its own tinted glyph (EXP-698 r4, Android
    /// parity). The whole row is the Button — a tap anywhere opens the picker,
    /// which is why there is no chevron to advertise it.
    private func metadataRow(
        label: String,
        icon: String,
        iconColor: Color,
        value: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            // 6pt between the glyph and its value, 16/12 paddings — Android's
            // `MetaRow`, dp for pt.
            HStack(spacing: 6) {
                Text(label)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))

                Spacer(minLength: 8)

                AppIcon(icon, size: CreateIssueRow.glyphSize)
                    .foregroundStyle(iconColor)

                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
            }
            .padding(.horizontal, CreateIssueRow.horizontalPadding)
            .padding(.vertical, CreateIssueRow.verticalPadding)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
            title: trimmedTitle,
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
                let api = deps.attachmentsApi
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
                onFinish(createdId)
            } else {
                // The issue exists; only an attachment failed. Hold the page
                // so the error is actually read, and latch the create so
                // acknowledging it can't file a duplicate issue. Going Back
                // still lands on the issue — it is real, and the attachment can
                // be retried there.
                createCommitted = true
                createdIssueId = createdId
            }
        } catch {
            self.error = error.userFacingMessage
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
