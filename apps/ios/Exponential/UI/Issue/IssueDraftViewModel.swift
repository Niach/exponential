import ExpUI
import ExpCore
import Foundation
import GRDB
import Observation
import UniformTypeIdentifiers

/// A file on the New issue page (EXP-327/878). Normally an attachment row
/// already uploaded against the draft, which `issues.create({draftId})`
/// reparents onto the new issue. EXP-1130: a SUB-ISSUE draft has no row to
/// upload against, so its picks ride here (`heldData`) under a local id and
/// go up right after the create.
struct IssueDraftFile: Identifiable, Sendable, Equatable {
    /// The real `attachments` row id, or a local UUID while `heldData` is set.
    let id: String
    let filename: String
    let contentType: String
    let sizeBytes: Int
    /// Non-nil = not uploaded yet (sub-issue mode); nil = a server row.
    let heldData: Data?
}

private enum DraftFileReadFailure: Error {
    case unreadable
    case tooLarge
}

/// File-scope so the off-main read captures nothing but the URL. The size is
/// checked before buffering: never read bytes the cap is going to reject.
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

/// EXP-1231: what the page's store watch reads — the draft row's presence and
/// an issue created from it (`issues.draft_id`), wherever that happened.
private struct DraftWatch: Equatable, Sendable {
    let present: Bool
    let createdIssueId: String?
}

/// EXP-1231: the store watch's task, cancellable from the nonisolated
/// `deinit` (a `@MainActor` model's own state is unreachable there).
private final class DraftWatchHandle: @unchecked Sendable {
    private let lock = NSLock()
    private var task: Task<Void, Never>?

    func set(_ task: Task<Void, Never>?) {
        lock.lock()
        let old = self.task
        self.task = task
        lock.unlock()
        old?.cancel()
    }

    func cancel() { set(nil) }
}

/// EXP-1170 — the New issue page's model: a DRAFT (`issue_drafts` row, id
/// minted by the opener at tap time) edited in the issue face's layout.
///
/// AUTOSAVE is ONE coalesced, single-flight `issueDrafts.upsert` of the FULL
/// row (`saveState`): `IssueDraftPage.autosaveDebounceMs` after the last
/// title/description edit, at once on a chip change, on blur, on leaving and
/// before Create. A save requested while one is in flight marks it dirty and
/// the loop writes once more with the state AS IT THEN STANDS — so an older
/// snapshot never lands last. Only a title, description or attachment is
/// content: chips alone never create a row; a page left with its content
/// emptied deletes the row it had. A sub-issue draft (`parentId`) never
/// writes (EXP-1097/1130).
///
/// EXP-1231 CONCURRENCY: the same draft may be open on other clients. The
/// model watches its row in the local store (`IssueDraftPage.fate`): an issue
/// carrying this draft id = created elsewhere, a seen row gone for
/// `discardedGraceMs` with no such issue = discarded elsewhere. Either stops
/// every write and publishes `consumedElsewhere` for the page to act on.
@MainActor
@Observable
final class IssueDraftViewModel {
    let draftId: String
    private(set) var boardId: String
    let parentId: String?
    /// The route's pre-picked status row (nil = the team's Backlog builtin).
    private let initialStatusId: String?

    // MARK: Fields

    var title = ""
    let editor = IssueEditorModel()
    var status: ResolvedIssueStatus = IssueStatusResolver.builtinDefault(for: .backlog)
    var priority: IssuePriority = .none
    var assigneeId: String?
    var labelIds: Set<String> = []
    /// Wire `yyyy-MM-dd`, like the issue row's.
    var dueDate: String?
    private(set) var attachments: [IssueDraftFile] = []

    // MARK: Team vocabulary

    private(set) var teamStatuses: [ResolvedIssueStatus] = IssueStatusResolver.builtinFallbackTeam
    private(set) var labels: [LabelEntity] = []
    private(set) var users: [UserEntity] = []
    private(set) var teamBoards: [BoardEntity] = []
    private(set) var teamId: String?
    private(set) var singleMemberTeam = false
    private(set) var permissions: TeamPermissions = .denied
    private(set) var parentIssue: IssueEntity?

    // MARK: Page state

    /// A row for `draftId` exists server-side.
    private(set) var rowExists = false
    private(set) var creating = false
    /// One-shot hand-off to the toaster; the view clears it once shown.
    var error: String?
    /// The draft's board is gone — the page pops.
    private(set) var loadFailed = false
    @ObservationIgnored private(set) var issueRefAugmentor: IssueRefAugmentor?

    /// EXP-1231: how another client consumed this draft.
    enum Consumed: Equatable {
        /// The issue it became: the page replaces itself with it.
        case created(issueId: String)
        /// Discarded: the page toasts and leaves.
        case discarded
    }

    /// EXP-1231: set once another client consumed the draft; the page acts.
    private(set) var consumedElsewhere: Consumed?

    // MARK: Plumbing

    @ObservationIgnored private var deps: AppDependencies?
    @ObservationIgnored private var accountId = ""
    @ObservationIgnored private var started = false
    /// The page is being left: the final write may DELETE an emptied row.
    @ObservationIgnored private var leaving = false
    /// Create filed the issue / Discard dropped the draft: nothing more is
    /// ever written.
    @ObservationIgnored private var finished = false
    /// An upload needs the row even without content yet (`ensureDraft`).
    @ObservationIgnored private var ensureRequested = false
    @ObservationIgnored private var imageCommitInFlight = false
    /// The eager image/media commit in flight — Create waits it out.
    @ObservationIgnored private var mediaCommitTask: Task<Void, Never>?
    /// The draft's attachment list is KNOWN: it had no row, the list loaded,
    /// or this page created the row. Until then "no attachments" may just be
    /// "not loaded yet", so an emptied page must not delete the row.
    @ObservationIgnored private var attachmentsLoaded = false
    /// Create's own pre-request flush — the one save allowed while creating.
    @ObservationIgnored private var preCreateFlush = false
    @ObservationIgnored private var lastImageCommitKeys: Set<String> = []
    /// The newest upsert failed (cleared by the next one that lands):
    /// `flushForKeep` reads it.
    @ObservationIgnored private var lastWriteFailed = false
    /// The newest failed upsert was the server refusing a consumed draft.
    @ObservationIgnored private var lastWriteConsumed = false

    /// The autosave machine: nothing pending, a debounced save waiting, or a
    /// save loop running (`dirtyAgain` = run once more when it lands).
    private enum SaveState {
        case idle
        case scheduled(Task<Void, Never>)
        case saving(dirtyAgain: Bool)
    }

    @ObservationIgnored private var saveState: SaveState = .idle
    @ObservationIgnored private var saveLoopTask: Task<Void, Never>?

    // MARK: Concurrency (EXP-1231)

    private let watchHandle = DraftWatchHandle()
    /// The latest store reading, re-judged whenever the page's own state
    /// changes (a failed Create, coming back on screen).
    @ObservationIgnored private var lastWatch: DraftWatch?
    /// The row was in the local store during this page's life (its own
    /// mirrored upsert counts). A row never seen is never "gone".
    @ObservationIgnored private var rowSeen = false
    /// The seen row is gone: no write while the grace runs (one could
    /// resurrect a discarded draft).
    @ObservationIgnored private var holdWrites = false
    @ObservationIgnored private var graceTask: Task<Void, Never>?
    /// Concluded while the page was off screen (a route pushed over it):
    /// published when it is back, so its exit never pops the wrong screen.
    @ObservationIgnored private var deferredConsumed: Consumed?

    init(draftId: String, boardId: String, statusId: String?, parentId: String?) {
        self.draftId = draftId
        self.boardId = boardId
        self.initialStatusId = statusId
        self.parentId = parentId
    }

    deinit {
        watchHandle.cancel()
    }

    // MARK: - Derived

    var trimmedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// The description as it would be stored: `draft://` placeholders (an
    /// image whose eager upload failed) never reach the server.
    var draftDescription: String {
        MarkdownImageUtils
            .stripUnknownDrafts(editor.currentMarkdown(), keep: [])
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// What the autosave writes: the draft's KNOWN content.
    var hasContent: Bool {
        IssueDraftPage.hasContent(
            title: title, description: draftDescription, attachmentCount: attachments.count
        )
    }

    /// EXP-1212: what an exit asks first. A create in flight owns the draft,
    /// and a draft already created or discarded has nothing left to ask about.
    /// A reopened draft (its row exists) whose file list has not loaded may
    /// be file-only, so it counts as content until the list is known; a
    /// brand-new draft (no row yet) is known to have no files.
    func prompt(for exit: IssueDraftPage.Exit) -> IssueDraftPage.Prompt {
        guard !finished, !creating else { return .none }
        let content = IssueDraftPage.hasContent(
            title: title,
            description: draftDescription,
            attachmentCount: attachments.count,
            attachmentsKnown: attachmentsLoaded || !rowExists
        )
        return IssueDraftPage.prompt(for: exit, hasContent: content)
    }

    /// EXP-1212: the leave dialog's answers. A sub-issue draft never writes
    /// a row, so "Keep as draft" would keep nothing: Create and Discard only.
    var leaveChoices: [IssueDraftPage.LeaveChoice] {
        IssueDraftPage.leaveChoices(canKeep: writesDraft)
    }

    /// EXP-1130: a sub-issue draft holds its uploads in memory until the
    /// create — it has no row to hang them off.
    var defersUploads: Bool { parentId != nil }

    private var writesDraft: Bool { parentId == nil }

    /// A create is in flight: no save may start (its upsert could land after
    /// the create's transaction deleted the row, resurrecting the draft) —
    /// except Create's own flush before the request.
    private var blockedByCreate: Bool { creating && !preCreateFlush }

    var canCreate: Bool { !trimmedTitle.isEmpty && !creating }

    var board: BoardEntity? { teamBoards.first { $0.id == boardId } }

    /// The board chip shows only when there is another board to pick.
    var showsBoardChip: Bool { teamBoards.count > 1 }

    var assignee: UserEntity? { users.first { $0.id == assigneeId } }

    var assignedLabels: [LabelEntity] { labels.filter { labelIds.contains($0.id) } }

    var mentionMembers: [MentionMember] {
        users.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }

    /// The full row as it stands — a draft is rewritten WHOLE, never patched.
    func snapshot(teamId: String) -> UpsertIssueDraftInput {
        UpsertIssueDraftInput(
            id: draftId,
            teamId: teamId,
            boardId: boardId,
            title: trimmedTitle,
            description: draftDescription,
            // A CONSTRUCTED default has no row id — nil means "the team's
            // Backlog builtin", which is exactly what the column means.
            statusId: status.rowId,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            // Only filter once the team's labels are here: an empty `labels`
            // means "not loaded yet", not "every label was deleted".
            labelIds: labels.isEmpty
                ? Array(labelIds)
                : Array(labelIds.filter { id in labels.contains { $0.id == id } }),
            dueDate: dueDate
        )
    }

    // MARK: - Load

    /// Once per page: seed from the draft row (if it exists) ONCE — later sync
    /// echoes never re-seed over the user's edits — then the team vocabulary.
    func start(deps: AppDependencies, accountId: String) {
        guard !started else { return }
        started = true
        self.deps = deps
        self.accountId = accountId
        configureEditor()
        startWatching()
        Task { await load() }
    }

    private var baseURL: URL? {
        deps?.auth.instanceBaseURL(forAccountId: accountId)
    }

    private func load() async {
        guard let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let draftId = draftId
        var seededStatusId = initialStatusId

        if writesDraft {
            let row: IssueDraftEntity? = (try? await pool.read { db in
                try IssueDraftQueries.draft(db: db, id: draftId)
            }) ?? nil
            if let row {
                rowExists = true
                boardId = row.boardId
                title = row.title
                editor.load(markdown: row.description ?? "", baseURL: baseURL)
                priority = IssuePriority.from(row.priority)
                assigneeId = row.assigneeId
                labelIds = Set(row.labelIds)
                dueDate = row.dueDate
                seededStatusId = row.statusId
                if let files = try? await deps.issueDraftsApi.listAttachments(
                    accountId: accountId, id: draftId
                ) {
                    attachmentsLoaded = true
                    attachments = files.map {
                        IssueDraftFile(
                            id: $0.id,
                            filename: $0.filename,
                            contentType: $0.contentType,
                            sizeBytes: $0.sizeBytes,
                            heldData: nil
                        )
                    }
                }
            } else {
                attachmentsLoaded = true
            }
        } else {
            attachmentsLoaded = true
        }

        if let parentId {
            parentIssue = (try? await pool.read { db in
                try IssueEntity.fetchOne(db, key: parentId)
            }) ?? nil
        }

        let boardId = boardId
        let board: BoardEntity? = (try? await pool.read { db in
            try BoardEntity.fetchOne(db, key: boardId)
        }) ?? nil
        guard let board else {
            // The draft's board is gone (deleted, archived, or no longer
            // ours): there is nothing to file it onto.
            loadFailed = true
            return
        }
        let teamId = board.teamId
        self.teamId = teamId
        let team: TeamEntity? = (try? await pool.read { db in
            try TeamEntity.fetchOne(db, key: teamId)
        }) ?? nil

        // Assignee/mention candidates are the TEAM's members (EXP-487).
        let members = (try? await pool.read { db in
            try teamMemberUsers(teamId: teamId, db: db)
        }) ?? []
        users = membersByDisplayName(members)
        // Solo team (EXP-50): no assignee chip, and a NEW draft is pre-assigned
        // to the one member. A reopened draft already says who it is for.
        if let humanIds = try? await pool.read({ db in
            try humanTeamMemberIds(teamId: teamId, db: db)
        }), humanIds.count == 1 {
            singleMemberTeam = true
            if !rowExists { assigneeId = humanIds.first }
        }
        // Drop an assignee who is no longer on the team (once it loaded).
        if !members.isEmpty, let id = assigneeId, !members.contains(where: { $0.id == id }) {
            assigneeId = nil
        }

        if let rows = try? await pool.read({ db in
            try IssueStatusEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) {
            let resolved = IssueStatusResolver.teamStatusesOrFallback(rows)
            teamStatuses = resolved
            var picked = IssueStatusResolver.resolve(statusId: seededStatusId, anchor: nil, team: resolved)
            // A draft can't be a duplicate (nothing to link yet).
            if picked.category == .duplicate {
                picked = IssueStatusResolver.resolve(statusId: nil, anchor: nil, team: resolved)
            }
            status = picked
        }

        if let loaded = try? await pool.read({ db in
            try LabelEntity
                .filter(Column("team_id") == teamId)
                .order(Column("name"))
                .fetchAll(db)
        }) {
            labels = loaded.sorted {
                $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
            }
            // Drop labels deleted while the draft sat (once they loaded).
            if !loaded.isEmpty {
                labelIds = labelIds.filter { id in loaded.contains { $0.id == id } }
            }
        }

        if let boards = try? await pool.read({ db in
            try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
        }) {
            teamBoards = boards.sorted {
                $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
            }
        }

        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: deps.auth.userId,
            isAdmin: deps.auth.isAdmin,
            dbPool: pool
        )
    }

    private func configureEditor() {
        guard let deps else { return }
        let accountId = accountId
        let scopeBoard = boardId
        // A user edit: an inserted image/media block uploads EAGERLY against
        // the draft (guarded on the pending set, so typing is a set
        // comparison), and the text schedules the debounced save.
        editor.onEdit = { [weak self] in
            self?.commitDraftMediaIfNeeded()
            self?.noteTextEdit()
        }
        editor.issueRefResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: .board(id: scopeBoard), db: deps.db, accountId: accountId)?
                .issueId
        }
        editor.issueRefTitleResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: .board(id: scopeBoard), db: deps.db, accountId: accountId)?
                .title
        }
        editor.issueRefStatusResolver = { identifier in
            IssueRefChipCache.statusInfo(
                identifier, scope: .board(id: scopeBoard), db: deps.db, accountId: accountId)
        }
        // EXP-892: local rows instantly, a debounced `issues.search` behind.
        let augmentor = issueRefAugmentor ?? IssueRefAugmentor(
            scope: .board(id: scopeBoard),
            db: deps.db,
            accountId: accountId,
            issuesApi: deps.issuesApi
        )
        issueRefAugmentor = augmentor
        augmentor.attach(to: editor)
    }

    // MARK: - Edits

    func setTitle(_ value: String) {
        guard value != title else { return }
        title = value
        noteTextEdit()
    }

    func setDueDate(_ date: Date?) {
        dueDate = date.map { AppDateFormatters.yyyyMMdd.string(from: $0) }
        noteChipChange()
    }

    func setBoard(_ id: String) {
        guard id != boardId, teamBoards.contains(where: { $0.id == id }) else { return }
        boardId = id
        noteChipChange()
    }

    /// Create a team label and pick it on this draft. The label is real at
    /// once; only its assignment waits for the create (via `labelIds`).
    func createAndSelectLabel(name: String, color: String) async {
        guard let deps, let teamId else { return }
        do {
            let labelId = try await deps.labelsApi.create(
                accountId: accountId,
                CreateLabelInput(name: name, color: color, teamId: teamId)
            )
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
            labelIds.insert(labelId)
            noteChipChange()
        } catch {
            self.error = error.userFacingMessage
        }
    }

    // MARK: - Autosave

    /// A title/description edit: save `autosaveDebounceMs` after the LAST one.
    func noteTextEdit() {
        guard writesDraft, !finished, !blockedByCreate else { return }
        switch saveState {
        case .saving:
            saveState = .saving(dirtyAgain: true)
        case let .scheduled(task):
            task.cancel()
            scheduleSave()
        case .idle:
            scheduleSave()
        }
    }

    /// A chip/board change: save at once.
    func noteChipChange() {
        saveNow()
    }

    private func scheduleSave() {
        let task = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(Int(IssueDraftPage.autosaveDebounceMs)))
            guard !Task.isCancelled else { return }
            self?.fireScheduledSave()
        }
        saveState = .scheduled(task)
    }

    private func fireScheduledSave() {
        guard case .scheduled = saveState, !blockedByCreate else {
            if case .scheduled = saveState { saveState = .idle }
            return
        }
        startSaveLoop()
    }

    /// Save now: a debounced save is pulled forward; a running one is marked
    /// dirty so its loop writes the newest state once more.
    func saveNow() {
        guard writesDraft, !finished, !blockedByCreate else { return }
        switch saveState {
        case .saving:
            saveState = .saving(dirtyAgain: true)
        case let .scheduled(task):
            task.cancel()
            startSaveLoop()
        case .idle:
            startSaveLoop()
        }
    }

    /// Save now and wait for the write (and any re-run) to land.
    func flush() async {
        saveNow()
        await saveLoopTask?.value
    }

    /// EXP-1212 "Keep as draft": save now and report whether the draft
    /// stands saved. A failed write already toasted its error (`error`);
    /// false = the page stays.
    func flushForKeep() async -> Bool {
        guard writesDraft, !finished, !creating else { return false }
        lastWriteFailed = false
        await flush()
        if lastWriteFailed {
            if error == nil, !lastWriteConsumed { error = "Couldn't save the draft." }
            return false
        }
        return true
    }

    private func startSaveLoop() {
        saveState = .saving(dirtyAgain: false)
        // Strong capture: a save started by the page's exit must outlive it.
        saveLoopTask = Task {
            while true {
                await self.writeOnce()
                if case .saving(dirtyAgain: true) = self.saveState {
                    self.saveState = .saving(dirtyAgain: false)
                    continue
                }
                self.saveState = .idle
                return
            }
        }
    }

    /// ONE write of the row as it stands now: upsert when there is content
    /// (or an upload needs the row), delete an emptied row on the way out,
    /// nothing otherwise.
    private func writeOnce() async {
        guard writesDraft, !finished, !blockedByCreate, !holdWrites, let deps else { return }
        guard hasContent || ensureRequested else {
            // Only a KNOWN-empty draft is deleted: an attachment list that
            // never loaded may still hold files.
            if rowExists, leaving, attachmentsLoaded {
                await deleteRow()
            }
            return
        }
        guard let teamId = await resolveTeamId() else {
            lastWriteFailed = true
            return
        }
        let input = snapshot(teamId: teamId)
        do {
            let dto = try await deps.issueDraftsApi.upsert(accountId: accountId, input)
            // This page created the row: its attachment list is the truth.
            if !rowExists { attachmentsLoaded = true }
            rowExists = true
            ensureRequested = false
            lastWriteFailed = false
            await mirrorDraft(dto)
        } catch {
            ensureRequested = false
            lastWriteFailed = true
            // EXP-1231: CONFLICT = the draft was consumed elsewhere (created
            // or discarded); the store watch carries the page out, no toast.
            lastWriteConsumed = error.trpcErrorCode == "CONFLICT"
            if !leaving, !lastWriteConsumed { self.error = error.userFacingMessage }
        }
    }

    /// The board's team, read from the board when the load hasn't landed —
    /// a paste in the first second, or a Back before the load, must still
    /// have a draft to write.
    private func resolveTeamId() async -> String? {
        if let teamId { return teamId }
        guard let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return nil }
        let boardId = boardId
        let resolved: String? = (try? await pool.read { db in
            try BoardEntity.fetchOne(db, key: boardId)?.teamId
        }) ?? nil
        if let resolved { teamId = resolved }
        return resolved
    }

    /// Mirror the server's row locally so the Drafts list renders it without
    /// waiting for the long-poll (sync re-delivers the same row).
    private func mirrorDraft(_ dto: IssueDraftDto) async {
        guard let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let entity = dto.entity()
        try? await pool.write { db in try entity.save(db) }
    }

    private func deleteRow() async {
        guard let deps else { return }
        let key = draftId
        // A failed server delete keeps the local mirror: the row is still
        // there, and the Drafts list must keep showing it.
        do {
            try await deps.issueDraftsApi.delete(accountId: accountId, id: key)
        } catch {
            return
        }
        await deleteLocalRow()
    }

    private func deleteLocalRow() async {
        rowExists = false
        // EXP-1231: this page's own delete is never "discarded elsewhere".
        rowSeen = false
        guard let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let key = draftId
        _ = try? await pool.write { db in try IssueDraftEntity.deleteOne(db, key: key) }
    }

    /// The row must exist before anything can be uploaded against it — the
    /// upload route 404s without it. Goes through the same single-flight path.
    func ensureDraft() async -> Bool {
        if rowExists { return true }
        guard writesDraft, !finished else { return false }
        ensureRequested = true
        await flush()
        return rowExists
    }

    // MARK: - Leaving

    /// The page is being left (Back, a pop, the app backgrounding it away):
    /// the final write — save, delete an emptied row, or nothing. Idempotent.
    func leave() {
        guard !finished else { return }
        leaving = true
        saveNow()
    }

    /// The page is on screen again (a route pushed over it was popped):
    /// editing resumes, so the next write is a plain save again.
    func resume() {
        if let deferred = deferredConsumed {
            deferredConsumed = nil
            consumedElsewhere = deferred
            return
        }
        guard !finished else { return }
        leaving = false
        judgeFate()
    }

    /// Discard draft: drop the row (server + local) if there is one. The
    /// page asks first (EXP-1212). Waits out a save in flight, which may be
    /// creating it.
    func discard() {
        // A create in flight owns the draft until it lands or fails.
        guard !finished, !creating else { return }
        finished = true
        leaving = true
        if case let .scheduled(task) = saveState {
            task.cancel()
            saveState = .idle
        }
        stopWatching()
        guard writesDraft else { return }
        let inFlight = saveLoopTask
        Task {
            await inFlight?.value
            if self.rowExists { await self.deleteRow() }
        }
    }

    // MARK: - Files

    /// Read a picked file off-main inside its security scope, then upload it
    /// EAGERLY against the draft (EXP-878) — or hold it (sub-issue mode).
    func ingestFile(_ url: URL) {
        guard let deps else { return }
        let filename = AttachmentFiles.sanitizedFilename(url.lastPathComponent)
        let contentType = AttachmentFiles.canonicalContentType(
            UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
        )
        Task {
            switch await Task.detached(operation: { readDraftFileBytes(from: url) }).value {
            case let .success(data):
                if defersUploads {
                    attachments.append(IssueDraftFile(
                        id: UUID().uuidString.lowercased(),
                        filename: filename,
                        contentType: contentType,
                        sizeBytes: data.count,
                        heldData: data
                    ))
                    return
                }
                guard await ensureDraft() else { return }
                do {
                    let uploaded = try await deps.attachmentsApi.uploadDraft(
                        accountId: accountId,
                        draftId: draftId,
                        data: data,
                        filename: filename,
                        contentType: contentType
                    )
                    attachments.append(IssueDraftFile(
                        id: uploaded.id,
                        filename: uploaded.filename,
                        contentType: uploaded.contentType,
                        sizeBytes: uploaded.sizeBytes,
                        heldData: nil
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

    /// Drop one file: a real delete for an uploaded row (it would otherwise
    /// be reparented onto the issue), a list removal for a held pick.
    func removeAttachment(_ file: IssueDraftFile) async {
        if file.heldData != nil {
            attachments.removeAll { $0.id == file.id }
            return
        }
        guard let deps else { return }
        do {
            try await deps.attachmentsApi.delete(accountId: accountId, attachmentId: file.id)
            attachments.removeAll { $0.id == file.id }
            noteTextEdit()
        } catch {
            self.error = error.userFacingMessage
        }
    }

    /// Upload every pending image/media block against the draft and swap the
    /// block URLs for the real ones. Guarded so typing costs a set
    /// comparison and a failed upload never becomes a retry loop.
    private func commitDraftMediaIfNeeded() {
        guard !defersUploads, let deps else { return }
        let keys = Set(editor.pendingImages.keys)
        guard !keys.isEmpty, !imageCommitInFlight, keys != lastImageCommitKeys else { return }
        lastImageCommitKeys = keys
        imageCommitInFlight = true
        mediaCommitTask = Task {
            defer { imageCommitInFlight = false }
            guard await ensureDraft() else { return }
            let api = deps.attachmentsApi
            let acc = accountId
            let key = draftId
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
                noteTextEdit()
            } else {
                error = "Some images couldn't be uploaded. Tap an image to retry."
            }
        }
    }

    // MARK: - Create

    /// File the issue. Returns its id, or nil when it failed (the page stays,
    /// the error toasts).
    func create() async -> String? {
        guard canCreate, let deps else { return nil }
        creating = true
        // An image pasted a moment ago may still be uploading against the
        // draft: let it land before judging the placeholders.
        if !defersUploads { await mediaCommitTask?.value }
        // Every image/media block is already an attachment on the draft; one
        // that never uploaded is still a `draft://` placeholder no issue may
        // carry. A sub-issue draft uploads AFTER the create instead.
        guard defersUploads || !editor.hasUncommittedDrafts else {
            error = "Some images couldn't be uploaded. Tap an image to retry."
            creating = false
            return nil
        }
        preCreateFlush = true
        await flush()
        preCreateFlush = false
        // Nothing more may be written: an upsert landing after the create's
        // transaction deleted the row would resurrect it.
        finished = true

        let description = draftDescription
        // The server rejects the whole create on an unknown label id.
        let validLabelIds = labelIds.filter { id in labels.contains { $0.id == id } }
        let input = CreateIssueInput(
            boardId: boardId,
            title: trimmedTitle,
            // A CONSTRUCTED default has no row id: the anchor enum instead.
            status: status.rowId == nil ? status.anchor.rawValue : nil,
            statusId: status.rowId,
            priority: priority.rawValue,
            assigneeId: assigneeId,
            description: description.isEmpty ? nil : description,
            dueDate: dueDate,
            labelIds: validLabelIds.isEmpty ? nil : Array(validLabelIds),
            // The server reparents the draft's attachments and deletes the
            // row inside the create's transaction.
            draftId: rowExists && !defersUploads ? draftId : nil,
            parentId: parentId
        )

        do {
            let created = try await deps.issuesApi.create(accountId: accountId, input)
            var finalDescription = description
            if defersUploads {
                let media = await uploadDeferredMedia(issueId: created.id, stripped: description)
                if let patched = media.patched { finalDescription = patched }
                await uploadDeferredFiles(issueId: created.id)
            }
            // The Share Extension defaults its picker to the last board used.
            SharedBoardMirror.writeLastUsed(accountId: accountId, boardId: boardId)
            await mirrorCreatedIssue(
                created,
                description: finalDescription.isEmpty ? nil : finalDescription,
                labelIds: validLabelIds
            )
            // The server dropped the draft row; drop the local mirror too.
            stopWatching()
            await deleteLocalRow()
            creating = false
            return created.id
        } catch {
            self.error = error.userFacingMessage
            creating = false
            // The draft lives on: edits typed during the failed create save.
            finished = false
            saveNow()
            // EXP-1231: whatever the store saw meanwhile is judged now.
            judgeFate()
            return nil
        }
    }

    // MARK: - Concurrency (EXP-1231)

    /// Watch this draft's row and any issue made from it in the local store.
    /// A sub-issue draft writes no row: nothing to watch.
    private func startWatching() {
        guard writesDraft, let deps, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let draftId = draftId
        let observation = ValueObservation.tracking { db -> DraftWatch in
            let present = try IssueDraftEntity.exists(db, key: draftId)
            let createdIssueId = try IssueEntity
                .filter(Column("draft_id") == draftId)
                .select(Column("id"), as: String.self)
                .fetchOne(db)
            return DraftWatch(present: present, createdIssueId: createdIssueId)
        }
        .removeDuplicates()
        watchHandle.set(Task { [weak self] in
            do {
                for try await watch in observation.values(in: pool) {
                    guard let self else { return }
                    self.observe(watch)
                }
            } catch {}
        })
    }

    private func stopWatching() {
        watchHandle.cancel()
        graceTask?.cancel()
        graceTask = nil
    }

    private func observe(_ watch: DraftWatch) {
        lastWatch = watch
        if watch.present { rowSeen = true }
        judgeFate()
    }

    private var fate: IssueDraftPage.Fate {
        guard let lastWatch else { return .open }
        return IssueDraftPage.fate(
            seen: rowSeen, present: lastWatch.present, createdIssueId: lastWatch.createdIssueId
        )
    }

    /// Apply the store's verdict. Never while the page's own Create is in
    /// flight or after its own Create/Discard. Off screen (`leaving`) a
    /// creation still stops every write but is published only once the page
    /// is back; a gone row waits for the page to be back.
    private func judgeFate() {
        guard writesDraft, !finished, !creating else { return }
        switch fate {
        case let .created(issueId):
            conclude(.created(issueId: issueId))
        case .gone:
            guard !leaving else { return }
            holdWrites = true
            guard graceTask == nil else { return }
            graceTask = Task { [weak self] in
                try? await Task.sleep(for: .milliseconds(Int(IssueDraftPage.discardedGraceMs)))
                guard !Task.isCancelled, let self else { return }
                self.graceTask = nil
                guard !self.finished, !self.creating, !self.leaving, self.fate == .gone else { return }
                self.conclude(.discarded)
            }
        case .open:
            graceTask?.cancel()
            graceTask = nil
            if holdWrites {
                // The row is back (a resync, this page's own racing write):
                // editing resumes, and edits held meanwhile save.
                holdWrites = false
                saveNow()
            }
        }
    }

    /// Consumed elsewhere: nothing is ever written again, and the page acts
    /// (now, or once it is back on screen).
    private func conclude(_ consumed: Consumed) {
        finished = true
        if case let .scheduled(task) = saveState {
            task.cancel()
            saveState = .idle
        }
        stopWatching()
        if leaving {
            deferredConsumed = consumed
        } else {
            consumedElsewhere = consumed
        }
    }

    /// EXP-1130: upload the held image/media blocks against the issue just
    /// filed and patch the final markdown in.
    private func uploadDeferredMedia(
        issueId: String, stripped: String
    ) async -> (patched: String?, ok: Bool) {
        guard let deps, !editor.pendingImages.isEmpty else { return (nil, true) }
        let api = deps.attachmentsApi
        let acc = accountId
        let uploader: @Sendable (PendingImage) async throws -> String = { image in
            let uploaded = try await api.upload(
                accountId: acc,
                issueId: issueId,
                data: image.data,
                filename: image.filename,
                contentType: image.contentType,
                media: image.mediaUploadParts
            )
            return uploaded.url
        }
        let allUploaded = await editor.commitPendingImages(uploader: uploader)
        let finalMarkdown = editor.currentMarkdown()
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard allUploaded, !editor.hasUncommittedDrafts else {
            error = "Issue created, but some images couldn't be uploaded."
            return (nil, false)
        }
        guard finalMarkdown != stripped else { return (nil, true) }
        do {
            try await deps.issuesApi.update(
                accountId: accountId,
                UpdateIssueInput(id: issueId, description: finalMarkdown.isEmpty ? nil : finalMarkdown)
            )
            return (finalMarkdown, true)
        } catch {
            self.error = "Issue created, but its images couldn't be saved. \(error.userFacingMessage)"
            return (nil, false)
        }
    }

    /// EXP-1130: upload the held file picks against the issue just filed. A
    /// rejected file surfaces as an error; the create already succeeded.
    private func uploadDeferredFiles(issueId: String) async {
        guard let deps else { return }
        var failed: [String] = []
        for file in attachments {
            guard let data = file.heldData else { continue }
            do {
                _ = try await deps.attachmentsApi.upload(
                    accountId: accountId,
                    issueId: issueId,
                    data: data,
                    filename: file.filename,
                    contentType: file.contentType
                )
            } catch {
                failed.append(file.filename)
            }
        }
        if !failed.isEmpty {
            error = "Issue created, but couldn't attach \(failed.joined(separator: ", "))."
        }
    }

    /// Mirror the created row (and its label joins) locally so the issue the
    /// page lands on renders at once (EXP-596). Best-effort and idempotent.
    private func mirrorCreatedIssue(
        _ created: IssueCreateResult,
        description: String?,
        labelIds: Set<String>
    ) async {
        guard let deps,
              let fetched = created.issue,
              let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let entity = fetched.entity().replacingDescription(description)
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
}

// MARK: - The shared pickers (EXP-1170)

/// The draft's side of the shared pickers: every pick stays local and
/// autosaves at once.
extension IssueDraftViewModel: IssuePropertyActions {
    var pickedStatus: ResolvedIssueStatus { status }
    var pickedPriority: IssuePriority { priority }
    var pickedAssigneeId: String? { assigneeId }
    var teamUsers: [UserEntity] { users }
    var teamLabels: [LabelEntity] { labels }
    var assignedLabelIds: Set<String> { labelIds }
    var pickerBoards: [BoardEntity] { teamBoards }
    var pickedBoardId: String { boardId }

    func pickStatus(_ status: ResolvedIssueStatus) {
        self.status = status
        noteChipChange()
    }

    func pickPriority(_ priority: IssuePriority) {
        self.priority = priority
        noteChipChange()
    }

    func pickAssignee(_ userId: String?) {
        assigneeId = userId
        noteChipChange()
    }

    func toggleLabelPick(_ labelId: String) {
        if labelIds.contains(labelId) {
            labelIds.remove(labelId)
        } else {
            labelIds.insert(labelId)
        }
        noteChipChange()
    }

    func createLabelPick(named name: String) {
        Task { await createAndSelectLabel(name: name, color: autoLabelColor(for: name)) }
    }
}
