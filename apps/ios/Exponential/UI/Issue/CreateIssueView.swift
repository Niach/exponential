import ExpUI
import ExpCore
import SwiftUI
import GRDB
import UniformTypeIdentifiers

/// A file attached before the issue exists (EXP-327). EXP-878: the bytes are
/// no longer held in memory until the create — the page owns a DRAFT row, so
/// the pick is uploaded against it immediately and this is the uploaded
/// attachment, which `issues.create({draftId})` reparents onto the new issue.
private struct DraftAttachment: Identifiable, Sendable {
    /// The real `attachments` row id.
    let id: String
    let filename: String
    let contentType: String
    let sizeBytes: Int
}

/// EXP-878 — the draft as the page holds it, minus the team (resolved from the
/// board). Sendable so the close-out's fire-and-forget write carries it out of
/// the view instead of reading a `View` struct off the main actor.
private struct DraftSnapshot: Sendable {
    let id: String
    let boardId: String
    let title: String
    let description: String
    let statusId: String?
    let priority: String
    let assigneeId: String?
    let labelIds: [String]
    let dueDate: String?

    func input(teamId: String) -> UpsertIssueDraftInput {
        UpsertIssueDraftInput(
            id: id,
            teamId: teamId,
            boardId: boardId,
            title: title,
            description: description,
            statusId: statusId,
            priority: priority,
            assigneeId: assigneeId,
            labelIds: labelIds,
            dueDate: dueDate
        )
    }
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

/// The New-issue PAGE (EXP-687 — it used to be a sheet): back icon top-left,
/// `Create` top-right, exactly like Android's `CreateIssueScreen`.
struct CreateIssueView: View {
    let boardId: String
    /// EXP-878: the saved draft this page reopens; nil = a blank compose,
    /// which mints its own id up front (`draftKey`) so eager uploads have
    /// something to hang off.
    let draftId: String?
    /// The page is done: the created issue's id so the host can land on it
    /// (EXP-596), or nil when nothing was filed (the draft, if any, was
    /// already persisted by then).
    let onFinish: (String?) -> Void

    init(boardId: String, draftId: String? = nil, onFinish: @escaping (String?) -> Void) {
        self.boardId = boardId
        self.draftId = draftId
        self.onFinish = onFinish
        // The id is the VIEW's, minted once: a reopened draft keeps its row,
        // a blank compose gets a fresh lowercase uuid it can already upload
        // attachments against.
        _draftKey = State(initialValue: draftId ?? UUID().uuidString.lowercased())
        _draftExists = State(initialValue: draftId != nil)
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId

    @State private var title = ""
    @State private var editor = IssueEditorModel()
    /// EXP-892 — the `#` menu's server half, kept across re-renders so its
    /// debounce survives a keystroke.
    @State private var issueRefAugmentor: IssueRefAugmentor?
    /// Files picked from the editor's attach menu — already uploaded against
    /// the draft (EXP-878).
    @State private var draftAttachments: [DraftAttachment] = []
    /// The draft id every write on this page uses. Never changes.
    @State private var draftKey: String
    /// A row for `draftKey` exists server-side: it was opened from one, or an
    /// eager upload created it. Drives the "clear everything, Back ⇒ delete"
    /// half of the close.
    @State private var draftExists: Bool
    /// The one ensure-upsert per view session is done (attachments upload
    /// against a row that is already there).
    @State private var draftEnsured = false
    /// The draft row was seeded into the fields once; a second `onAppear`
    /// (returning from a picker) must not re-seed over the user's edits.
    @State private var seeded = false
    /// The `status_id` the seeded draft carried, resolved against the team's
    /// rows once they load (nil = the team's Backlog builtin).
    @State private var seededStatusId: String?
    @State private var statusSeeded = false
    /// Close-out ran (create or Back), so `onDisappear` must not run it again.
    @State private var didFinish = false
    /// One eager image/media commit at a time, and never a retry loop: a
    /// failed upload leaves its draft key in place, so the pass only re-runs
    /// when the pending set actually changes.
    @State private var imageCommitInFlight = false
    @State private var lastImageCommitKeys: Set<String> = []
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
    @State private var loading = false
    @State private var error: String?
    @State private var permissions: TeamPermissions = .denied
    /// ONE presentation for the four pickers — four `.sheet(isPresented:)` on
    /// one node meant only the first ever presented (EXP-240).
    @State private var picker: CreateIssuePicker?
    /// Non-nil once this page filed its issue — Back then lands on it.
    @State private var createdIssueId: String?
    @FocusState private var titleFocused: Bool

    /// The title as it would be filed: a run of spaces is not a title, and
    /// leading/trailing whitespace never belongs in one.
    private var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Create is live only with a title, nothing in flight, and nothing filed
    /// from this page yet (a second Create would file a duplicate).
    private var canSubmit: Bool {
        !trimmedTitle.isEmpty && !loading && createdIssueId == nil
    }

    /// The description as it would be stored: `draft://` placeholders (an
    /// image whose eager upload failed) are not interchange markdown and never
    /// reach the server — on a draft row or on the issue.
    private var draftDescription: String {
        MarkdownImageUtils
            .stripUnknownDrafts(editor.currentMarkdown(), keep: [])
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// EXP-878: the page has something worth keeping — a real title, a real
    /// description, or at least one uploaded attachment. Closing with this
    /// true saves a draft SILENTLY; closing with it false deletes the draft
    /// this page opened (and writes nothing at all for a blank compose).
    private var hasDraftContent: Bool {
        !trimmedTitle.isEmpty || !draftDescription.isEmpty || !draftAttachments.isEmpty
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
                    if !draftAttachments.isEmpty {
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
                        GlassMetaRow(
                            label: "Status",
                            icon: status.iconName,
                            iconColor: status.color,
                            value: status.name
                        ) { picker = .status }

                        GlassDivider()

                        // Priority
                        GlassMetaRow(
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
                            GlassMetaRow(
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
                    // EXP-698 r5: the shared block — the Properties sheet
                    // renders the very same pills. "+ Label" here creates a
                    // new team label and pre-selects it on this draft.
                    IssueLabelsSelector(
                        labels: labels,
                        selectedIds: selectedLabelIds,
                        onToggle: { labelId in
                            if selectedLabelIds.contains(labelId) {
                                selectedLabelIds.remove(labelId)
                            } else {
                                selectedLabelIds.insert(labelId)
                            }
                        },
                        onAdd: { picker = .createLabel }
                    )

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
                // A close mid-create would race the draft write against the
                // issue the server is already filing.
                .disabled(loading)
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
        // EXP-878: no discard confirmation any more — leaving with content
        // SAVES a draft (silently), leaving with none deletes it.
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
                    // EXP-878: the draft's own fields FIRST — the status block
                    // below resolves the seeded `status_id` instead of
                    // re-pinning Backlog, and the solo-team pre-assign must
                    // not overwrite the assignee the draft carries.
                    await seedFromDraftIfNeeded(pool: pool)
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
                        // A reopened draft already says who it is for — even
                        // when that is "nobody".
                        if draftId == nil {
                            assigneeId = humanIds.first
                        }
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
                        // EXP-878: the seeded draft's row wins; with none
                        // (or a blank compose) `resolve` lands on the team's
                        // own Backlog builtin, exactly as before. Once only —
                        // a second appear must not undo the user's pick.
                        if !statusSeeded {
                            statusSeeded = true
                            status = IssueStatusResolver.resolve(
                                statusId: seededStatusId, anchor: nil, team: resolved
                            )
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
        // EXP-1021: status / priority / assignee are TYPED pickers driven by
        // `open`; only the label editor still presents a view.
        .sheet(item: viewPicker) { target in
            pickerSheet(target)
        }
        .background { propertyPickers }
        // A close that did not go through the toolbar's Back (a system pop,
        // an account switch) still owes the one write.
        .onDisappear {
            if !didFinish {
                didFinish = true
                persistDraftIfNeeded()
            }
        }
    }

    /// The pickers that still present a VIEW of their own — only the label
    /// editor, which is a name + a swatch strip rather than a row list.
    private var viewPicker: Binding<CreateIssuePicker?> {
        Binding(
            get: { picker == .createLabel ? picker : nil },
            set: { picker = $0 }
        )
    }

    /// One picker's open state (EXP-1021): the metadata row that set `picker`
    /// is the trigger, and it lives in a different part of the tree.
    private func pickerOpen(_ target: CreateIssuePicker) -> Binding<Bool> {
        Binding(
            get: { picker == target },
            set: { isOpen in if !isOpen, picker == target { picker = nil } }
        )
    }

    /// The new issue's property pickers — the SAME typed pickers the issue
    /// face uses, over the same sheet.
    @ViewBuilder
    private var propertyPickers: some View {
        ZStack {
            StatusPicker(
                // Duplicate CATEGORY = status interception (L27): a new issue
                // can't be a duplicate (nothing to link yet), so it's not a
                // create option. The team's own status order — the ONE picker
                // vocabulary (REV2-85).
                statuses: teamStatuses
                    .filter { $0.category != .duplicate }
                    .map(StatusPickerStatus.init),
                value: [status.id],
                onChange: { picked in
                    guard let selected = teamStatuses.first(where: { picked.contains($0.id) })
                    else { return }
                    status = selected
                },
                open: pickerOpen(.status),
                hideTrigger: true,
                trigger: { EmptyView() }
            )

            PriorityPicker(
                options: IssuePriority.displayOrder.map(PriorityPickerOption.init),
                value: [priority.id],
                onChange: { picked in
                    guard let selected = IssuePriority.displayOrder
                        .first(where: { picked.contains($0.id) }) else { return }
                    priority = selected
                },
                open: pickerOpen(.priority),
                hideTrigger: true,
                trigger: { EmptyView() }
            )

            AssigneePicker(
                members: users.map(AssigneePickerMember.init),
                // An EMPTY set is unassigned; the picker offers the row that
                // clears the pick and reports it back as nothing.
                value: assigneeId.map { [$0] } ?? [],
                onChange: { picked in assigneeId = picked.first },
                open: pickerOpen(.assignee),
                hideTrigger: true,
                trigger: { EmptyView() }
            )
        }
    }

    @ViewBuilder
    private func pickerSheet(_ target: CreateIssuePicker) -> some View {
        switch target {
        case .createLabel:
            CreateLabelSheet { name, color in
                Task { await createAndSelectLabel(name: name, color: color) }
            }
        // The typed pickers (EXP-1021) never come through here.
        case .status, .priority, .assignee:
            EmptyView()
        }
    }

    /// EXP-878: Back never asks. It persists the draft (or deletes the one it
    /// opened), exactly once, then hands back. A create in flight owns the
    /// page until it finishes.
    private func attemptClose() {
        guard !loading else { return }
        persistDraftIfNeeded()
        didFinish = true
        onFinish(createdIssueId)
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

    // MARK: - Draft files (EXP-327)

    private var draftFilesSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Files")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
            VStack(spacing: 6) {
                ForEach(draftAttachments) { file in
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
                            Text(Int64(file.sizeBytes).formatted(.byteCount(style: .file)))
                                .font(.caption2)
                                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        }
                        Spacer(minLength: 8)
                        Button {
                            Task { await removeDraftAttachment(file) }
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
    /// a cloud provider streams over the network), then upload it EAGERLY
    /// against this page's draft (EXP-878) — attachments no longer wait for an
    /// issue id. Inline images never reach here; the editor appends those to
    /// the description, and `commitDraftMediaIfNeeded` uploads them the same
    /// way.
    private func ingestDraftFile(_ url: URL) {
        let filename = AttachmentFiles.sanitizedFilename(url.lastPathComponent)
        let contentType = AttachmentFiles.canonicalContentType(
            UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
        )
        Task {
            switch await Task.detached(operation: { readDraftFileBytes(from: url) }).value {
            case let .success(data):
                guard await ensureDraft() else { return }
                do {
                    let uploaded = try await deps.attachmentsApi.uploadDraft(
                        accountId: accountId,
                        draftId: draftKey,
                        data: data,
                        filename: filename,
                        contentType: contentType
                    )
                    draftAttachments.append(DraftAttachment(
                        id: uploaded.id,
                        filename: uploaded.filename,
                        contentType: uploaded.contentType,
                        sizeBytes: uploaded.sizeBytes
                    ))
                } catch {
                    self.error = "Couldn't attach \(filename). \(error.userFacingMessage)"
                }
            case .failure(.tooLarge):
                error = "Files must be 50 MB or smaller."
            case .failure(.unreadable):
                error = "Couldn't read \(filename)."
            }
        }
    }

    /// Drop one already-uploaded draft attachment. It is a real row, so the
    /// removal is a real delete — leaving it would reparent the file onto the
    /// issue the page goes on to file.
    private func removeDraftAttachment(_ file: DraftAttachment) async {
        do {
            try await deps.attachmentsApi.delete(accountId: accountId, attachmentId: file.id)
            draftAttachments.removeAll { $0.id == file.id }
        } catch {
            self.error = error.userFacingMessage
        }
    }

    // MARK: - The draft row (EXP-878)

    /// This page's team, resolved from the board when the onAppear load has
    /// not landed yet — an image pasted in the first second must still have a
    /// draft to hang off.
    private func resolveTeamId() async -> String? {
        if let teamId { return teamId }
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return nil }
        let resolved: String? = (try? await pool.read { db in
            try BoardEntity.fetchOne(db, key: boardId)?.teamId
        }) ?? nil
        if let resolved { teamId = resolved }
        return resolved
    }

    /// The whole draft as it stands, minus its team. A draft is rewritten
    /// WHOLE, never patched: one write carries every field the page owns.
    /// Captured up front so the fire-and-forget close-out write never reads
    /// the view again.
    private var draftSnapshot: DraftSnapshot {
        DraftSnapshot(
            id: draftKey,
            boardId: boardId,
            title: trimmedTitle,
            description: draftDescription,
            // A CONSTRUCTED default has no row id — nil means "the team's
            // Backlog builtin", which is exactly what the column means.
            statusId: status.rowId,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            // Only filter once the team's labels are actually here: an empty
            // `labels` means "not loaded yet", and filtering against it would
            // silently strip a reopened draft's own labels.
            labelIds: labels.isEmpty
                ? Array(selectedLabelIds)
                : Array(selectedLabelIds.filter { id in labels.contains { $0.id == id } }),
            dueDate: dueDate.map { formatDate($0) }
        )
    }

    /// The draft row must exist before anything can be uploaded against it.
    /// ONE upsert per view session (`draftEnsured`), carrying the snapshot as
    /// it stands; the close-out rewrites it with the final one.
    @discardableResult
    private func ensureDraft() async -> Bool {
        if draftEnsured { return true }
        guard let teamId = await resolveTeamId() else { return false }
        do {
            let dto = try await deps.issueDraftsApi.upsert(
                accountId: accountId, draftSnapshot.input(teamId: teamId)
            )
            await mirrorDraft(dto)
            draftEnsured = true
            draftExists = true
            return true
        } catch {
            self.error = error.userFacingMessage
            return false
        }
    }

    /// Mirror the server's draft row locally so the Drafts list renders it
    /// without waiting for the Electric long-poll (the `mirrorCreatedIssue`
    /// pattern — sync re-delivers the same row and overwrites this one).
    private func mirrorDraft(_ dto: IssueDraftDto) async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let entity = dto.entity()
        try? await pool.write { db in try entity.save(db) }
    }

    /// Upload every pending image/media draft against the draft row and swap
    /// the block URLs for the real `/api/attachments/{id}` ones — the
    /// issue-detail paste path, one draft id earlier. Fires off the editor's
    /// edit hook, guarded so typing costs a set comparison and a failed upload
    /// never becomes a retry loop (the block's own Retry button re-runs it).
    private func commitDraftMediaIfNeeded() {
        let keys = Set(editor.pendingImages.keys)
        guard !keys.isEmpty, !imageCommitInFlight, keys != lastImageCommitKeys else { return }
        lastImageCommitKeys = keys
        imageCommitInFlight = true
        Task {
            defer { imageCommitInFlight = false }
            guard await ensureDraft() else { return }
            let api = deps.attachmentsApi
            let acc = accountId
            let key = draftKey
            let uploader: @Sendable (PendingImage) async throws -> String = { image in
                let uploaded = try await api.uploadDraft(
                    accountId: acc,
                    draftId: key,
                    data: image.data,
                    filename: image.filename,
                    contentType: image.contentType,
                    media: image.mediaUploadParts
                )
                return uploaded.url
            }
            if await editor.commitPendingImages(uploader: uploader) {
                error = nil
            } else {
                error = "Some images couldn't be uploaded. Tap an image to retry."
            }
        }
    }

    /// Seed the page from the draft it reopens: fields first, attachments over
    /// the wire (they are NOT synced — the attachments shape excludes
    /// draft-owned rows). Once per view.
    private func seedFromDraftIfNeeded(pool: DatabasePool) async {
        guard let draftId, !seeded else { return }
        seeded = true
        let row: IssueDraftEntity? = (try? await pool.read { db in
            try IssueDraftQueries.draft(db: db, id: draftId)
        }) ?? nil
        if let row {
            title = row.title
            editor.load(markdown: row.description ?? "", baseURL: instanceBaseURL)
            priority = IssuePriority.from(row.priority)
            assigneeId = row.assigneeId
            selectedLabelIds = Set(row.labelIds)
            dueDate = row.dueDate.flatMap { AppDateFormatters.yyyyMMdd.date(from: $0) }
            seededStatusId = row.statusId
        }
        if let files = try? await deps.issueDraftsApi.listAttachments(
            accountId: accountId, id: draftId
        ) {
            draftAttachments = files.map {
                DraftAttachment(
                    id: $0.id,
                    filename: $0.filename,
                    contentType: $0.contentType,
                    sizeBytes: $0.sizeBytes
                )
            }
        }
    }

    /// The ONE write a close owes (EXP-878) — never one per keystroke: content
    /// saves the draft silently, no content deletes the draft this page opened,
    /// and a blank compose writes nothing at all. A filed issue owns its own
    /// clean-up (the server deletes the draft inside `issues.create`).
    private func persistDraftIfNeeded() {
        guard createdIssueId == nil else { return }
        let api = deps.issueDraftsApi
        let db = deps.db
        let account = accountId
        let key = draftKey
        if hasDraftContent {
            let snapshot = draftSnapshot
            let knownTeamId = teamId
            let board = boardId
            Task {
                // The close can beat the onAppear load: resolve the board's
                // team here rather than dropping the draft on the floor.
                var team = knownTeamId
                if team == nil, let pool = try? db.pool(forAccountId: account) {
                    team = (try? await pool.read { db in
                        try BoardEntity.fetchOne(db, key: board)?.teamId
                    }) ?? nil
                }
                guard let team,
                      let dto = try? await api.upsert(
                          accountId: account, snapshot.input(teamId: team)
                      ),
                      let pool = try? db.pool(forAccountId: account) else { return }
                let entity = dto.entity()
                try? await pool.write { db in try entity.save(db) }
            }
        } else if draftExists {
            Task {
                try? await api.delete(accountId: account, id: key)
                guard let pool = try? db.pool(forAccountId: account) else { return }
                _ = try? await pool.write { db in try IssueDraftEntity.deleteOne(db, key: key) }
            }
        }
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
        // EXP-878: every image/media block is already a real attachment on the
        // draft. A block that never uploaded is still a `draft://` placeholder,
        // which no issue may carry — say so instead of silently dropping it.
        guard !editor.hasUncommittedDrafts else {
            error = "Some images couldn't be uploaded. Tap an image to retry."
            return
        }
        loading = true
        error = nil

        let dateStr = dueDate.map { formatDate($0) }
        let description = draftDescription

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
            description: description.isEmpty ? nil : description,
            dueDate: dateStr,
            labelIds: validLabelIds.isEmpty ? nil : Array(validLabelIds),
            // EXP-878: always this page's draft id — the server reparents its
            // attachments and deletes the row in the create's transaction, and
            // tolerates an id no row was ever written for (a compose that
            // never uploaded anything).
            draftId: draftKey
        )

        do {
            let created = try await deps.issuesApi.create(accountId: accountId, input)
            let createdId = created.id

            // Remember the board so the Share Extension defaults its picker to it.
            SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)

            await mirrorCreatedIssue(
                created,
                description: description.isEmpty ? nil : description,
                labelIds: validLabelIds
            )
            // The server dropped the draft inside the same transaction; drop
            // the local mirror too so the Drafts list loses the row now.
            await deleteLocalDraftRow()

            createdIssueId = createdId
            loading = false
            didFinish = true
            onFinish(createdId)
            return
        } catch {
            self.error = error.userFacingMessage
        }
        loading = false
    }

    /// Remove the local mirror of this page's draft (the server-side row is
    /// already gone — `issues.create({draftId})` deleted it).
    private func deleteLocalDraftRow() async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let key = draftKey
        _ = try? await pool.write { db in try IssueDraftEntity.deleteOne(db, key: key) }
        draftExists = false
    }

    /// `#IDENTIFIER` refs resolve/search against the target board's team:
    /// pills for refs that resolve locally, and a #-autocomplete inserting the
    /// plain interchange token.
    private func configureEditor() {
        // EXP-878: an inserted image/media block is uploaded EAGERLY against
        // the draft. `onEdit` is the model's only insertion signal; the commit
        // itself is guarded on the pending set, so plain typing is a no-op.
        editor.onEdit = { commitDraftMediaIfNeeded() }
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
        // EXP-892: the locally ranked rows render instantly; a debounced
        // `issues.search` splices the server's full-text hits in behind them.
        let augmentor = issueRefAugmentor ?? IssueRefAugmentor(
            scope: .board(id: boardId),
            db: deps.db,
            accountId: accountId,
            issuesApi: deps.issuesApi
        )
        issueRefAugmentor = augmentor
        augmentor.attach(to: editor)
    }

    private var instanceBaseURL: URL? {
        deps.auth.instanceBaseURL(forAccountId: accountId)
    }

    private func formatDate(_ date: Date) -> String {
        AppDateFormatters.yyyyMMdd.string(from: date)
    }
}
