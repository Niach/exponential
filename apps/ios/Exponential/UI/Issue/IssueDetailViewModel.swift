import ExpUI
import ExpCore
import Foundation
import GRDB

@MainActor @Observable
final class IssueDetailViewModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    var issueLabels: [IssueLabelEntity] = []
    var users: [UserEntity] = []
    /// Live coding sessions for this issue (synced coding_sessions shape) —
    /// drives the "Coding now" badge + Watch/Steer entry (masterplan §5c).
    var runningSessions: [CodingSessionEntity] = []
    /// The canonical issue when this one is marked a duplicate — resolves the
    /// "Duplicate of {IDENTIFIER}" banner (masterplan §5e).
    var duplicateOf: IssueEntity?
    /// The issue's board — carries `teamId` + `repositoryId` so the repo
    /// name chip can resolve the backing repo (masterplan §6, R4).
    var board: BoardEntity?
    /// The issue's team — needed (with the board + issue identifier) to
    /// build the shareable web URL.
    var team: TeamEntity?
    /// Every synced board (all teams) — filtered to the issue's
    /// team by `moveTargetBoards` for the "Move to board" picker
    /// (EXP-57). Trashed boards never reach the local store (the boards
    /// shape filters `deleted_at IS NULL` server-side).
    var boards: [BoardEntity] = []

    // Non-agent members offered by the editor's @-mention autocomplete.
    var mentionMembers: [MentionMember] {
        users.filter { !$0.isAgent }.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }
    var editingTitle: String = ""
    /// Single source of truth for the description editor (blocks + pending images).
    let editor = IssueEditorModel()
    var saving = false
    var error: String?
    var permissions: TeamPermissions = .denied
    // True while a signed-in viewer looks like a non-member ONLY because the
    // team_members shape hasn't synced yet (drives a "Syncing team…"
    // banner rather than silently rendering the issue read-only).
    var permissionsPending = false
    var isSubscribed = false
    /// True when the issue's team has exactly one human member: the
    /// assignee picker row is hidden (nothing to reassign to) — EXP-50.
    var singleMemberTeam = false

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let issueImagesApi: IssueImagesApi
    private let labelsApi: LabelsApi
    private let subscriptionsApi: SubscriptionsApi
    private let auth: AuthRepository
    private let baseURL: URL?
    /// Raw instance base string for building shareable web links.
    private let instanceUrl: String?
    private var observationTask: Task<Void, Never>?
    private var autosaveTask: Task<Void, Never>?
    private var livenessTask: Task<Void, Never>?
    // Raw observed running-session rows — cached so the liveness ticker can
    // re-apply the staleness filter between sync deltas (EXP-153).
    private var observedSessions: [CodingSessionEntity] = []

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        issueImagesApi: IssueImagesApi,
        labelsApi: LabelsApi,
        subscriptionsApi: SubscriptionsApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.issueImagesApi = issueImagesApi
        self.labelsApi = labelsApi
        self.subscriptionsApi = subscriptionsApi
        self.auth = auth
        let instanceUrl = auth.accounts.first(where: { $0.id == accountId })?.instanceUrl ?? auth.instanceUrl
        self.instanceUrl = instanceUrl
        self.baseURL = instanceUrl.flatMap { URL(string: $0) }
        editor.onEdit = { [weak self] in self?.scheduleAutosave() }
        // Inline `#IDENTIFIER` refs render as tappable pills when they resolve
        // against the local issues store (render-only; see IssueRefs).
        editor.issueRefResolver = { [weak self] identifier in
            self?.resolveIssueRef(identifier)
        }
        // Typing `#` offers same-team issues; selecting one inserts the
        // plain `#IDENTIFIER` interchange token.
        editor.issueRefSearch = { [weak self] query in
            self?.searchIssueRefs(query) ?? []
        }
    }

    /// identifier (e.g. `VER-12`) → local issue id, from the synced GRDB store
    /// (same team only). Synchronous lookup; nil when unknown (token
    /// stays plain).
    func resolveIssueRef(_ identifier: String) -> String? {
        IssueRefLookup.resolve(identifier, scope: .issue(id: issueId), db: db, accountId: accountId)
    }

    /// Issues offered by the description editor's #-autocomplete
    /// (team-scoped; identifier + title substring match).
    func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issueId), db: db, accountId: accountId)
    }

    func startObserving() {
        // GRDB only re-fires on writes — a minute clock re-applies the
        // staleness filter so a phantom session's steer panel clears once its
        // liveness window elapses without any sync delta (EXP-153).
        livenessTask?.cancel()
        livenessTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(60))
                guard let self, !Task.isCancelled else { return }
                self.applySessionLiveness()
            }
        }
        observationTask = Task { [weak self] in
            guard let self else { return }
            guard let pool = try? self.db.pool(forAccountId: self.accountId) else {
                self.error = "Couldn't open local data store"
                return
            }

            let issueObs = ValueObservation.tracking { db in
                try IssueEntity.fetchOne(db, key: self.issueId)
            }
            Task {
                for try await issue in issueObs.values(in: pool) {
                    guard let issue else { continue }
                    let isFirstLoad = self.issue == nil
                    self.issue = issue
                    let remoteText = getIssueDescriptionText(issue.description)
                    if isFirstLoad {
                        self.editingTitle = issue.title
                        self.editor.load(markdown: remoteText, baseURL: self.baseURL)
                    } else {
                        // Live-apply remote edits when safe; otherwise stash for
                        // a user-driven reload (field-level last-write-wins).
                        self.editor.applyRemote(markdown: remoteText, baseURL: self.baseURL)
                    }
                    self.refreshPermissions(for: issue)
                    self.refreshBoard(issue: issue, pool: pool)
                    self.refreshDuplicateOf(issue: issue, pool: pool)
                }
            }

            // Live "coding now" sessions for this issue (14th synced shape).
            let sessionObs = ValueObservation.tracking { db in
                try CodingSessionEntity
                    .filter(Column("issue_id") == self.issueId)
                    .filter(Column("status") == DomainContract.codingSessionStatusRunning)
                    .fetchAll(db)
            }
            Task {
                for try await sessions in sessionObs.values(in: pool) {
                    self.observedSessions = sessions
                    self.applySessionLiveness()
                }
            }

            let labelObs = ValueObservation.tracking { db in try LabelEntity.fetchAll(db) }
            Task {
                for try await labels in labelObs.values(in: pool) {
                    self.labels = labels
                }
            }

            // Team boards for the "Move to board" picker (EXP-57).
            let boardObs = ValueObservation.tracking { db in try BoardEntity.fetchAll(db) }
            Task {
                for try await boards in boardObs.values(in: pool) {
                    self.boards = boards
                }
            }

            let issueLabelObs = ValueObservation.tracking { db in
                try IssueLabelEntity.filter(Column("issue_id") == self.issueId).fetchAll(db)
            }
            Task {
                for try await il in issueLabelObs.values(in: pool) {
                    self.issueLabels = il
                }
            }

            let userObs = ValueObservation.tracking { db in try UserEntity.fetchAll(db) }
            Task {
                for try await users in userObs.values(in: pool) {
                    self.users = users
                    // is_agent flips / member user rows arriving can change the
                    // human-member count, which gates the assignee row (EXP-50).
                    if let issue = self.issue {
                        self.refreshPermissions(for: issue)
                    }
                }
            }

            // Subscription state for the Bell toggle in the detail toolbar.
            let subObs = ValueObservation.tracking { db in
                try IssueSubscriberEntity.filter(Column("issue_id") == self.issueId).fetchAll(db)
            }
            Task {
                for try await subs in subObs.values(in: pool) {
                    let me = self.auth.userId
                    self.isSubscribed = me != nil && subs.contains { $0.userId == me && !$0.unsubscribed }
                }
            }

            // Recompute permissions when membership or the members-shape sync
            // state changes. The issue row observed above may not change again
            // after the members shape snapshots in, so without this the "Syncing
            // team…" banner would stick and the issue would stay read-only
            // until the view is remounted. Tracks the two regions the
            // computation reads: the team_members table and the
            // "team-members" offset row (isLive).
            let membersObs = ValueObservation.tracking { db -> (Int, Bool) in
                let count = try TeamMemberEntity.fetchCount(db)
                let live = try ElectricOffset.fetchOne(db, key: "team-members")?.isLive ?? false
                return (count, live)
            }
            Task {
                for try await _ in membersObs.values(in: pool) {
                    if let issue = self.issue {
                        self.refreshPermissions(for: issue)
                    }
                }
            }
        }
    }

    func toggleSubscribe() async {
        let wasSubscribed = isSubscribed
        do {
            if wasSubscribed {
                try await subscriptionsApi.unsubscribe(accountId: accountId, issueId: issueId)
            } else {
                try await subscriptionsApi.subscribe(accountId: accountId, issueId: issueId)
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    func stopObserving() {
        observationTask?.cancel()
        observationTask = nil
        autosaveTask?.cancel()
        autosaveTask = nil
        livenessTask?.cancel()
        livenessTask = nil
    }

    // Heartbeat-stale rows render as absent — mirroring the server sweep's
    // DELETE (EXP-153).
    private func applySessionLiveness() {
        runningSessions = observedSessions.filter { CodingSessionLiveness.isLive($0) }
    }

    var assignedLabelIds: Set<String> {
        Set(issueLabels.map(\.labelId))
    }

    func assignee() -> UserEntity? {
        guard let id = issue?.assigneeId else { return nil }
        return users.first { $0.id == id }
    }

    /// Same-team boards the issue can move to (EXP-57): the current
    /// board and archived boards are excluded; name-sorted. Empty on a
    /// single-board team — the "Move to board" action hides then.
    var moveTargetBoards: [BoardEntity] {
        guard let issue, let teamId = board?.teamId else { return [] }
        return boards
            .filter { $0.teamId == teamId && $0.id != issue.boardId && $0.archivedAt == nil }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// Move the issue to another board in the same team (EXP-57). The
    /// issue keeps its id — the detail view's by-id observation stays valid —
    /// but the server renumbers it in the target board; the new
    /// boardId/identifier arrive via Electric sync (standard mutation
    /// pattern — no local optimistic write).
    func moveToBoard(_ boardId: String) async {
        guard let issue, issue.boardId != boardId else { return }
        do {
            try await issuesApi.move(accountId: accountId, id: issue.id, boardId: boardId)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func reloadRemoteDescription() {
        editor.reloadPendingRemote(baseURL: baseURL)
    }

    // MARK: - Mutations

    func saveTitle() async {
        guard let issue, editingTitle != issue.title, !editingTitle.isEmpty else { return }
        await update(UpdateIssueInput(id: issue.id, title: editingTitle))
    }

    /// Upload any pending draft images, then persist the description — but only
    /// if every image resolved (all-or-nothing). On partial failure the failed
    /// drafts stay pending with a retry affordance and nothing is saved.
    func commitDescription() async {
        guard let issue else { return }

        let allUploaded = await editor.commitPendingImages(uploader: makeImageUploader(issueId: issue.id))
        guard allUploaded, !editor.hasUncommittedDrafts else {
            error = "Some images couldn't be uploaded. Tap an image to retry."
            return
        }
        error = nil

        let markdown = editor.currentMarkdown()
        guard markdown != editor.lastSavedMarkdown else { return }

        var input = UpdateIssueInput(id: issue.id)
        if markdown.isEmpty {
            input.explicitNulls.insert("description")
        } else {
            input.description = markdown
        }
        // Only baseline the editor when the server accepted the write: a failed
        // save must leave isDirty true so the next autosave / onDisappear commit
        // retries (the lastSavedMarkdown guard above stays open) and applyRemote
        // stashes remote content behind the reload banner instead of clobbering
        // the unsaved edit.
        guard await update(input) else { return }
        editor.markSaved(markdown)
    }

    private func makeImageUploader(issueId: String) -> @Sendable (PendingImage) async throws -> String {
        let api = issueImagesApi
        let accountId = accountId
        return { image in
            let uploaded = try await api.upload(
                accountId: accountId,
                issueId: issueId,
                data: image.data,
                filename: image.filename,
                contentType: image.contentType
            )
            return uploaded.url
        }
    }

    private func scheduleAutosave() {
        autosaveTask?.cancel()
        autosaveTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 1_200_000_000)
            guard !Task.isCancelled else { return }
            // Run the save in an INDEPENDENT task: the debounce timer above is
            // cancelled by the next keystroke, and we must not let that cancel
            // an in-flight save's network request mid-write.
            self?.saveNow()
        }
    }

    private func saveNow() {
        Task { [weak self] in await self?.commitDescription() }
    }

    func setStatus(_ status: IssueStatus) async {
        guard let issue else { return }
        await update(UpdateIssueInput(id: issue.id, status: status.rawValue))
    }

    func setPriority(_ priority: IssuePriority) async {
        guard let issue else { return }
        await update(UpdateIssueInput(id: issue.id, priority: priority.rawValue))
    }

    func setAssignee(_ userId: String?) async {
        guard let issue else { return }
        if let userId {
            await update(UpdateIssueInput(id: issue.id, assigneeId: userId))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("assigneeId")
            await update(input)
        }
    }

    func setDueDate(_ date: Date?) async {
        guard let issue else { return }
        if let date {
            await update(UpdateIssueInput(id: issue.id, dueDate: formatDate(date)))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("dueDate")
            await update(input)
        }
    }

    func setDueTime(_ time: String?) async {
        guard let issue else { return }
        if let time {
            await update(UpdateIssueInput(id: issue.id, dueTime: time))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("dueTime")
            await update(input)
        }
    }

    func setEndTime(_ time: String?) async {
        guard let issue else { return }
        if let time {
            await update(UpdateIssueInput(id: issue.id, endTime: time))
        } else {
            var input = UpdateIssueInput(id: issue.id)
            input.explicitNulls.insert("endTime")
            await update(input)
        }
    }

    /// Create a team label and assign it to this issue in one step
    /// (parity with Android's createAndAssignLabel).
    func createAndAssignLabel(name: String, color: String) async {
        guard let issue, let teamId = board?.teamId else { return }
        do {
            let labelId = try await labelsApi.create(
                accountId: accountId,
                CreateLabelInput(name: name, color: color, teamId: teamId)
            )
            try await labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func toggleLabel(_ labelId: String) async {
        guard let issue else { return }
        do {
            if assignedLabelIds.contains(labelId) {
                try await labelsApi.removeFromIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            } else {
                try await labelsApi.addToIssue(accountId: accountId, issueId: issue.id, labelId: labelId)
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    // MARK: - Duplicate-of (masterplan §5e)

    /// Mark this issue as a duplicate of `canonical`: sets `duplicateOfId` AND
    /// flips status to the terminal `duplicate` in ONE atomic update.
    func markDuplicate(of canonical: IssueEntity) async {
        guard let issue, canonical.id != issue.id else { return }
        await update(UpdateIssueInput(
            id: issue.id,
            status: IssueStatus.duplicate.rawValue,
            duplicateOfId: canonical.id
        ))
    }

    /// Clear the duplicate marking: null the FK and restore a working status.
    func unmarkDuplicate() async {
        guard let issue else { return }
        var input = UpdateIssueInput(id: issue.id, status: IssueStatus.backlog.rawValue)
        input.explicitNulls.insert("duplicateOfId")
        await update(input)
    }

    /// Candidate canonical issues for the duplicate picker: every other issue
    /// in the same team (across boards), newest first. One-shot read —
    /// the picker is transient.
    func duplicateCandidates() async -> [IssueEntity] {
        guard let issue, let pool = try? db.pool(forAccountId: accountId) else { return [] }
        let issueId = issue.id
        let boardId = issue.boardId
        let result: [IssueEntity]? = try? await pool.read { db in
            guard let board = try BoardEntity.fetchOne(db, key: boardId) else { return [] }
            let teamBoardIds = try BoardEntity
                .filter(Column("team_id") == board.teamId)
                .fetchAll(db)
                .map(\.id)
            return try IssueEntity
                .filter(teamBoardIds.contains(Column("board_id")))
                .fetchAll(db)
                .filter { $0.id != issueId && $0.archivedAt == nil }
                .sorted { $0.updatedAt > $1.updatedAt }
        }
        return result ?? []
    }

    /// Candidate issues for the unified Start-coding sheet (EXP-156): every
    /// eligible issue in the current issue's team, the current issue pinned
    /// first (pre-checked) and the rest by recency. Eligibility = the issue's
    /// board is repo-backed and not archived, the issue isn't archived, its
    /// status isn't terminal (done/cancelled/duplicate) and its PR isn't merged.
    /// The CURRENT issue is exempt from the issue-level checks (archived /
    /// terminal / merged) so it always appears — you opened the card from it.
    /// One-shot read; the sheet is transient. (Trashed boards never reach the
    /// local store, so "not deleted" is implicit.)
    func startCodingCandidates() async -> [StartCodingSheet.IssueOption] {
        guard let issue, let pool = try? db.pool(forAccountId: accountId) else { return [] }
        let currentId = issue.id
        let currentBoardId = issue.boardId
        let result: [StartCodingSheet.IssueOption]? = try? await pool.read { db in
            guard let current = try BoardEntity.fetchOne(db, key: currentBoardId) else { return [] }
            let boards = try BoardEntity
                .filter(Column("team_id") == current.teamId)
                .fetchAll(db)
            // boardId → repositoryId for repo-backed boards. `repoActive` is
            // the normal eligibility set (non-archived); `repoAny` also holds
            // archived repo-backed boards so the current issue on an archived
            // board can still be force-included (parity with the desktop
            // dialog). A repo-LESS board is in neither map.
            var repoActive: [String: String] = [:]
            var repoAny: [String: String] = [:]
            for board in boards {
                guard let repoId = board.repositoryId else { continue }
                repoAny[board.id] = repoId
                if board.archivedAt == nil {
                    repoActive[board.id] = repoId
                }
            }
            let terminal: Set<String> = [
                IssueStatus.done.rawValue,
                IssueStatus.cancelled.rawValue,
                IssueStatus.duplicate.rawValue,
            ]
            let rows = try IssueEntity
                .filter(Array(repoAny.keys).contains(Column("board_id")))
                .fetchAll(db)
                .filter { row in
                    // The current issue is force-included as long as its board
                    // is repo-backed (archived OK) — exempt from the archived /
                    // terminal / merged rules so a checked pre-seed is never a
                    // stray. A repo-LESS current issue isn't in repoAny and
                    // correctly stays out of the pool entirely.
                    if row.id == currentId {
                        return repoAny[row.boardId] != nil
                    }
                    guard repoActive[row.boardId] != nil else { return false }
                    if row.archivedAt != nil { return false }
                    if terminal.contains(row.status) { return false }
                    if row.prState == DomainContract.prStateMerged { return false }
                    return true
                }
                .sorted { a, b in
                    if a.id == currentId { return true }
                    if b.id == currentId { return false }
                    return a.updatedAt > b.updatedAt
                }
            return rows.map { row in
                StartCodingSheet.IssueOption(
                    id: row.id,
                    identifier: row.identifier,
                    title: row.title,
                    repositoryId: repoAny[row.boardId],
                    status: row.status,
                    priority: row.priority
                )
            }
        }
        return result ?? []
    }

    private func refreshBoard(issue: IssueEntity, pool: DatabasePool) {
        guard board?.id != issue.boardId else { return }
        board = (try? pool.read { db in
            try BoardEntity.fetchOne(db, key: issue.boardId)
        }) ?? nil
    }

    private func refreshDuplicateOf(issue: IssueEntity, pool: DatabasePool) {
        guard let canonicalId = issue.duplicateOfId else {
            duplicateOf = nil
            return
        }
        guard duplicateOf?.id != canonicalId else { return }
        duplicateOf = (try? pool.read { db in
            try IssueEntity.fetchOne(db, key: canonicalId)
        }) ?? nil
    }

    func deleteIssue() async -> Bool {
        guard let issue else { return false }
        do {
            try await issuesApi.delete(accountId: accountId, id: issue.id)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    /// Runs the tRPC issue update. Returns false (and surfaces the error via
    /// `self.error`) on failure so callers can keep local state dirty and retry
    /// instead of treating the write as landed.
    @discardableResult
    private func update(_ input: UpdateIssueInput) async -> Bool {
        do {
            try await issuesApi.update(accountId: accountId, input)
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }

    private func refreshPermissions(for issue: IssueEntity) {
        guard let pool = try? db.pool(forAccountId: accountId) else { return }
        let (team, membersLive): (TeamEntity?, Bool) = (try? pool.read { db -> (TeamEntity?, Bool) in
            let board = try BoardEntity.fetchOne(db, key: issue.boardId)
            let ws = try board.flatMap { try TeamEntity.fetchOne(db, key: $0.teamId) }
            let live = try ElectricOffset.fetchOne(db, key: "team-members")?.isLive ?? false
            return (ws, live)
        }) ?? (nil, false)
        self.team = team
        permissions = TeamPermissions.resolve(
            team: team,
            currentUserId: auth.userId,
            isAdmin: auth.isAdmin,
            dbPool: pool
        )
        permissionsPending = permissions.isAuthed && !permissions.isMember && !membersLive
        if let team {
            let humanIds = (try? pool.read { db in
                try humanTeamMemberIds(teamId: team.id, db: db)
            }) ?? []
            singleMemberTeam = humanIds.count == 1
        } else {
            singleMemberTeam = false
        }
    }

    // MARK: - Share

    /// The shareable web URL for this issue, once the team, board and
    /// issue identifier are all resolved locally. Nil until then (or if the
    /// instance URL is unknown).
    var shareURL: URL? {
        guard let team, let board, let identifier = issue?.identifier, !identifier.isEmpty
        else { return nil }
        return WebLinks.issue(
            instanceUrl: instanceUrl,
            teamSlug: team.slug,
            boardSlug: board.slug,
            identifier: identifier
        )
    }

    /// Human-readable share subject: `{identifier}: {title}`.
    var shareText: String {
        guard let issue else { return "" }
        let id = issue.identifier ?? ""
        return id.isEmpty ? issue.title : "\(id): \(issue.title)"
    }

    private func formatDate(_ date: Date) -> String {
        AppDateFormatters.yyyyMMdd.string(from: date)
    }
}
