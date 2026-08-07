import ExpUI
import ExpCore
import Foundation
import GRDB
import UIKit
import UniformTypeIdentifiers

/// A file the user picked that hasn't landed as a synced `attachments` row yet
/// (EXP-297). It renders as a placeholder row in the Files section: a spinner
/// while the POST is in flight, or the failure plus a retry. Once the upload
/// returns, `uploadedId` is stamped and the entry is dropped the moment Electric
/// delivers the real row — so the list never flickers empty in between.
struct PendingFileUpload: Identifiable, Sendable {
    let id = UUID()
    let filename: String
    let contentType: String
    let data: Data
    var uploadedId: String?
    var failure: String?

    var sizeBytes: Int { data.count }
}

@MainActor @Observable
final class IssueDetailViewModel {
    var issue: IssueEntity?
    var labels: [LabelEntity] = []
    /// EXP-314: every synced team's `issue_statuses` rows; scoped to this
    /// issue's team by `teamStatuses`.
    var statusRows: [IssueStatusEntity] = []
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
    /// Non-inline-image attachments on this issue, oldest first (EXP-297) — the
    /// Files section's rows. Inline images (`image/png|jpeg|webp|gif|avif`) are
    /// deliberately excluded: they live in the description markdown and are
    /// rendered by the editor.
    var fileAttachments: [AttachmentEntity] = []
    /// Picked files whose upload is in flight (or failed) — see PendingFileUpload.
    var pendingFileUploads: [PendingFileUpload] = []
    /// Attachment ids with a delete request in flight; the row stays visible but
    /// inert until sync removes it.
    var deletingAttachmentIds: Set<String> = []

    // Non-agent members offered by the editor's @-mention autocomplete.
    var mentionMembers: [MentionMember] {
        users.map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }
    /// True once the bounded load clock ran out with no issue row — the view
    /// swaps its spinner for a real explanation plus a retry (EXP-264: a push
    /// tap or deep link to a row outside this account's synced scope used to
    /// spin forever).
    var loadTimedOut = false
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

    // Steer state for the bottom bar's Start-coding circle (EXP-240 — moved
    // here from AgentPrCard so the card can stay a pure status glance).
    var steerConfig: SteerConfig?
    /// The caller's online desktops; nil until presence resolves.
    var steerDevices: [SteerDevice]?
    /// True from "start sent" until the desktop's session row lands (or the
    /// 30s grace elapses) — renders the circle as a spinner. Single-issue
    /// starts only; batch runs get `batchStartNotice` instead.
    var startPending = false
    /// Transient confirmation after a BATCH start (2+ issues): the batch
    /// session row is issue-LESS (issue_id NULL), so it never syncs into this
    /// issue's `runningSessions` and the start circle can't reflect it —
    /// surface explicit "follow it in the Agents tab" feedback instead.
    var batchStartNotice: String?

    private let accountId: String
    private let issueId: String
    private let db: DatabaseManager
    private let issuesApi: IssuesApi
    private let issueImagesApi: IssueImagesApi
    private let attachmentsApi: AttachmentsApi
    private let labelsApi: LabelsApi
    private let subscriptionsApi: SubscriptionsApi
    private let steerApi: SteerApi
    /// EXP-432: the start circle's device source — `devices.list` team-scoped,
    /// not `steer.myDevices`, so a teammate's shared server counts too.
    private let devicesApi: DevicesApi
    private let auth: AuthRepository
    private let baseURL: URL?
    /// Raw instance base string for building shareable web links.
    private let instanceUrl: String?
    private var observationTask: Task<Void, Never>?
    private var autosaveTask: Task<Void, Never>?
    private var livenessTask: Task<Void, Never>?
    private var fallbackTask: Task<Void, Never>?
    // Raw observed running-session rows — cached so the liveness ticker can
    // re-apply the staleness filter between sync deltas (EXP-153).
    private var observedSessions: [CodingSessionEntity] = []

    init(
        accountId: String,
        issueId: String,
        db: DatabaseManager,
        issuesApi: IssuesApi,
        issueImagesApi: IssueImagesApi,
        attachmentsApi: AttachmentsApi,
        labelsApi: LabelsApi,
        subscriptionsApi: SubscriptionsApi,
        steerApi: SteerApi,
        devicesApi: DevicesApi,
        auth: AuthRepository
    ) {
        self.accountId = accountId
        self.issueId = issueId
        self.db = db
        self.issuesApi = issuesApi
        self.issueImagesApi = issueImagesApi
        self.attachmentsApi = attachmentsApi
        self.labelsApi = labelsApi
        self.subscriptionsApi = subscriptionsApi
        self.steerApi = steerApi
        self.devicesApi = devicesApi
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
        // The chip shows `#ID <title>` while editing, like the web editor
        // (EXP-322) — the title rides a serialization-invisible attachment, so
        // the saved markdown keeps the bare token.
        editor.issueRefTitleResolver = { [weak self] identifier in
            self?.resolveIssueRefTitle(identifier)
        }
        // ...and its status glyph over the token's `#` (EXP-423).
        editor.issueRefStatusResolver = { [weak self] identifier in
            self?.resolveIssueRefStatus(identifier)
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
        IssueRefChipCache.chip(identifier, scope: .issue(id: issueId), db: db, accountId: accountId)?
            .issueId
    }

    /// identifier → the issue's title, for the chip's display-only suffix.
    func resolveIssueRefTitle(_ identifier: String) -> String? {
        IssueRefChipCache.chip(identifier, scope: .issue(id: issueId), db: db, accountId: accountId)?
            .title
    }

    /// identifier → the issue's resolved status, for the glyph the chip paints
    /// over its `#` (EXP-423). Team scoping is inherent: the lookup resolves
    /// statuses against the same team it resolved the issue in.
    func resolveIssueRefStatus(_ identifier: String) -> IssueRefStatusInfo? {
        IssueRefChipCache.statusInfo(identifier, scope: .issue(id: issueId), db: db, accountId: accountId)
    }

    /// Issues offered by the description editor's #-autocomplete
    /// (team-scoped; identifier + title substring match).
    func searchIssueRefs(_ query: String) -> [IssueRefCandidate] {
        IssueRefLookup.search(query, scope: .issue(id: issueId), db: db, accountId: accountId)
    }

    func startObserving() {
        // The by-id observation below only ever fires for a row that IS local,
        // so an issue outside the synced scope needs its own bounded clock.
        startFallbackClock()
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

            // Live sessions for this issue (14th synced shape) — running AND
            // in_review (the terminal stays alive after the PR opens, EXP-194)
            // AND merged (the merge parks the session, EXP-358).
            let sessionObs = ValueObservation.tracking { db in
                try CodingSessionEntity
                    .filter(Column("issue_id") == self.issueId)
                    .filter([
                        DomainContract.codingSessionStatusRunning,
                        DomainContract.codingSessionStatusInReview,
                        DomainContract.codingSessionStatusMerged,
                    ].contains(Column("status")))
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

            // Team statuses (EXP-314) — same pattern as labels.
            let statusObs = ValueObservation.tracking { db in try IssueStatusEntity.fetchAll(db) }
            Task {
                for try await rows in statusObs.values(in: pool) {
                    self.statusRows = rows
                }
            }

            // Team boards for the "Move to board" picker (EXP-57).
            let boardObs = ValueObservation.tracking { db in try BoardEntity.fetchAll(db) }
            Task {
                for try await boards in boardObs.values(in: pool) {
                    self.boards = boards
                }
            }

            // Attachments on this issue (EXP-297). Filtered in Swift rather
            // than SQL: the inline-image rule is a shared contract constant, and
            // the row count per issue is tiny.
            let attachmentObs = ValueObservation.tracking { db in
                try AttachmentEntity.filter(Column("issue_id") == self.issueId).fetchAll(db)
            }
            Task {
                for try await rows in attachmentObs.values(in: pool) {
                    self.applyAttachments(rows)
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
                    // Member user rows arriving can change the member count,
                    // which gates the assignee row (EXP-50).
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
        fallbackTask?.cancel()
        fallbackTask = nil
    }

    // MARK: - Bounded loading (EXP-264)

    /// Two-stage clock for an issue that isn't in the local store: after 2s
    /// ask the server for the row directly, and 6s after that stop pretending
    /// and show the unavailable state. The first wait is deliberate — a push
    /// tap activates the scene, which restarts every pipeline, so the row very
    /// often arrives on its own without a request.
    private func startFallbackClock() {
        fallbackTask?.cancel()
        fallbackTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            guard let self, !Task.isCancelled else { return }
            if self.issue == nil {
                // Fired, not awaited: the fetch rides the shared URLSession's
                // 30s request budget, and awaiting it inline stretched this
                // clock's promised 8s bound to ~38s on a black-holed
                // connection. A fetch that completes after the timeout state
                // showed still writes the row, and the observation swaps the
                // screen to content.
                Task { [weak self] in
                    await self?.fetchIssueFallback()
                }
            }
            try? await Task.sleep(for: .seconds(6))
            guard !Task.isCancelled else { return }
            if self.issue == nil {
                self.loadTimedOut = true
            }
        }
    }

    /// "Try again" from the timed-out state: back to the spinner and run the
    /// whole clock again, fresh server read included.
    func retryLoad() {
        loadTimedOut = false
        startFallbackClock()
    }

    /// One-shot server read for an issue sync hasn't delivered (a push tap on
    /// a brand-new issue, a deep link into a board still syncing). The row is
    /// written into GRDB so the existing observation picks it up exactly like
    /// a synced one; comments, timeline events and every later edit still
    /// arrive through Electric as usual.
    ///
    /// Every failure is swallowed: an older self-hosted server has no
    /// `issues.get` at all, and a genuine NOT_FOUND/FORBIDDEN is the same
    /// answer as silence — the timeout state below explains it.
    private func fetchIssueFallback() async {
        guard let result = try? await issuesApi.get(accountId: accountId, id: issueId),
              let pool = try? db.pool(forAccountId: accountId) else { return }
        let entity = result.issue.entity()
        let labelRows = result.labelIds.map {
            IssueLabelEntity(issueId: entity.id, labelId: $0, teamId: result.teamId)
        }
        try? await pool.write { gdb in
            // Never clobber a synced row: sync is the source of truth and may
            // have landed while this request was in flight.
            guard try IssueEntity.fetchOne(gdb, key: entity.id) == nil else { return }
            try entity.save(gdb)
            for row in labelRows {
                try row.save(gdb)
            }
        }
    }

    // Heartbeat-stale rows render as absent — mirroring the server sweep's
    // DELETE (EXP-153).
    private func applySessionLiveness() {
        runningSessions = observedSessions.filter { CodingSessionLiveness.isLive($0) }
    }

    var assignedLabelIds: Set<String> {
        Set(issueLabels.map(\.labelId))
    }

    /// The issue's team's labels, name-sorted — the pool holds every synced
    /// team's labels, so the chip box / label sheet must scope to this one.
    var teamLabels: [LabelEntity] {
        guard let teamId = board?.teamId else { return [] }
        return labels
            .filter { $0.teamId == teamId }
            .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    /// Labels assigned to this issue, name-sorted (the chip box's label chips).
    var assignedLabels: [LabelEntity] {
        let assigned = assignedLabelIds
        return teamLabels.filter { assigned.contains($0.id) }
    }

    func assignee() -> UserEntity? {
        guard let id = issue?.assigneeId else { return nil }
        return users.first { $0.id == id }
    }

    // MARK: - Steer (bottom-bar Start-coding circle, EXP-240)

    /// Load the relay config + device presence for the start circle. Presence
    /// only matters while startable (member, relay on, no live session) — the
    /// same gating the AgentPrCard start area used.
    func refreshSteer() async {
        steerConfig = await SteerConfigCache.load(accountId: accountId, api: steerApi)
        guard steerConfig?.enabled == true, permissions.isMember, runningSessions.isEmpty else { return }
        // Scoped to the issue's team (EXP-432): teammates' shared servers are
        // start targets too. `devices.list` also returns OFFLINE machines,
        // which the circle must never offer — `onlineStartTargets` drops them.
        steerDevices = (try? await devicesApi.onlineStartTargets(
            accountId: accountId, teamId: board?.teamId
        )) ?? []
    }

    /// Remote-start on the chosen desktop (ported from AgentPrCard.start):
    /// 1 issue → single session, 2+ → batch. The desktop inserts the
    /// coding_sessions row when the launcher spins up, which flips the circle
    /// to the session dot via sync; re-enable after a 30s grace window in
    /// case it never does.
    func startCoding(on device: SteerDevice, issueIds: [String], options: SteerStartOptions) {
        guard !issueIds.isEmpty else { return }
        let isBatch = issueIds.count > 1
        // A batch run's session row is issue-less and never lands in this
        // issue's runningSessions — a spinner would just time out silently,
        // so only single-issue starts show the pending state.
        startPending = !isBatch
        Task {
            do {
                if isBatch {
                    try await steerApi.startSession(
                        accountId: accountId,
                        issueIds: issueIds,
                        deviceId: device.deviceId,
                        options: options
                    )
                    let label = device.deviceLabel.isEmpty ? "your desktop" : device.deviceLabel
                    batchStartNotice = "Batch start sent to \(label). Follow it in the Agents tab."
                } else {
                    try await steerApi.startSession(
                        accountId: accountId,
                        issueId: issueIds[0],
                        deviceId: device.deviceId,
                        options: options
                    )
                    Task {
                        try? await Task.sleep(for: .seconds(30))
                        startPending = false
                    }
                }
            } catch {
                startPending = false
                self.error = error.localizedDescription
            }
        }
    }

    /// Same-team boards the issue can move to (EXP-57): the current board is
    /// excluded; name-sorted. Empty on a single-board team — the "Move to
    /// board" action hides then.
    var moveTargetBoards: [BoardEntity] {
        guard let issue, let teamId = board?.teamId else { return [] }
        return boards
            .filter { $0.teamId == teamId && $0.id != issue.boardId }
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

    // MARK: - File attachments (EXP-297)

    /// True when this viewer may attach or delete files — the same membership
    /// gate every other issue mutation on this screen uses.
    var canManageFiles: Bool { permissions.isModerator }

    private func applyAttachments(_ rows: [AttachmentEntity]) {
        fileAttachments = rows
            .filter { !AttachmentFiles.isInlineImage(contentType: $0.contentType) }
            .sorted { ($0.createdAt, $0.id) < ($1.createdAt, $1.id) }
        // A pending entry whose real row has arrived is now a duplicate — the
        // synced row takes over.
        let syncedIds = Set(rows.map(\.id))
        pendingFileUploads.removeAll { pending in
            guard let uploadedId = pending.uploadedId else { return false }
            return syncedIds.contains(uploadedId)
        }
    }

    /// Attach a file picked through `.fileImporter`. The security-scoped read
    /// runs inside a detached task (the scope is started and stopped off-main
    /// around the read) so a 50 MB — possibly cloud-provider-backed — read can
    /// never freeze the main thread; every `pendingFileUploads` mutation hops
    /// back to the MainActor.
    func uploadFile(from url: URL) {
        let filename = AttachmentFiles.sanitizedFilename(url.lastPathComponent)
        // Canonical (lowercased, parameter-free) so the stored row always
        // exact-matches the inline-image classification on every client.
        let contentType = AttachmentFiles.canonicalContentType(
            UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
        )

        // An inline-image type uploaded through /files would be invisible
        // everywhere (filtered out of every client's Files section, referenced
        // by no markdown) and eventually deleted by the owner's
        // unreferenced-image sweep. EXP-327: rather than dead-ending the pick
        // with an error telling the user to go press the other button, put it
        // where it belongs — appended to the description. (The editor's attach
        // menu classifies picks up front, so this only catches a URL whose type
        // resolves differently here, or an oversize image the editor handed
        // over precisely because this path can report the failure.)
        guard !AttachmentFiles.isInlineImage(contentType: contentType) else {
            appendImageToDescription(from: url, filename: filename, contentType: contentType)
            return
        }

        Task.detached { [weak self] in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }

            // Size check before buffering — never read bytes the cap rejects.
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize
            if let size, size > AttachmentFiles.maxFileUploadBytes {
                await self?.appendFailedUpload(
                    filename: filename,
                    contentType: contentType,
                    failure: "Files must be 50 MB or smaller."
                )
                return
            }
            guard let data = try? Data(contentsOf: url) else {
                await self?.appendFailedUpload(
                    filename: filename,
                    contentType: contentType,
                    failure: "Couldn't read this file."
                )
                return
            }
            guard data.count <= AttachmentFiles.maxFileUploadBytes else {
                await self?.appendFailedUpload(
                    filename: filename,
                    contentType: contentType,
                    failure: "Files must be 50 MB or smaller."
                )
                return
            }
            await self?.startUpload(filename: filename, contentType: contentType, data: data)
        }
    }

    /// Read a picked image off-main (inside its security scope) and append it to
    /// the end of the description, then save — the seamless half of EXP-327.
    private func appendImageToDescription(from url: URL, filename: String, contentType: String) {
        let editor = editor
        Task.detached { [weak self] in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }

            // Size check before buffering, exactly like the non-image branch
            // above — an oversize pick must never be read into memory, and an
            // appended draft that can't upload blocks every later description
            // save.
            let size = (try? url.resourceValues(forKeys: [.fileSizeKey]))?.fileSize
            if let size, size > AttachmentFiles.maxFileUploadBytes {
                await self?.appendFailedUpload(
                    filename: filename,
                    contentType: contentType,
                    failure: "Files must be 50 MB or smaller."
                )
                return
            }
            guard let data = try? Data(contentsOf: url) else {
                await self?.appendFailedUpload(
                    filename: filename,
                    contentType: contentType,
                    failure: "Couldn't read this file."
                )
                return
            }
            guard data.count <= AttachmentFiles.maxFileUploadBytes else {
                await self?.appendFailedUpload(
                    filename: filename,
                    contentType: contentType,
                    failure: "Files must be 50 MB or smaller."
                )
                return
            }
            let decoded = UIImage(data: data)
            let width = decoded.map { Int($0.size.width * $0.scale) }
            let height = decoded.map { Int($0.size.height * $0.scale) }
            await editor.appendImage(
                data: data,
                filename: filename,
                contentType: contentType,
                width: (width ?? 0) > 0 ? width : nil,
                height: (height ?? 0) > 0 ? height : nil
            )
            await self?.commitDescriptionNow()
        }
    }

    /// Commit the description immediately, superseding the debounced autosave
    /// that `appendImage`'s edit hook just scheduled. `commitPendingImages` has
    /// no in-flight guard, so letting the 1.2s timer fire during a slower
    /// upload would upload the same draft twice and orphan an attachment row.
    private func commitDescriptionNow() async {
        autosaveTask?.cancel()
        autosaveTask = nil
        await commitDescription()
    }

    private func appendFailedUpload(filename: String, contentType: String, failure: String) {
        pendingFileUploads.append(PendingFileUpload(
            filename: filename,
            contentType: contentType,
            data: Data(),
            failure: failure
        ))
    }

    private func startUpload(filename: String, contentType: String, data: Data) {
        let pending = PendingFileUpload(filename: filename, contentType: contentType, data: data)
        pendingFileUploads.append(pending)
        Task { await performUpload(pendingId: pending.id) }
    }

    /// Retry a failed pending upload (the row's Retry button).
    func retryUpload(pendingId: UUID) {
        guard let index = pendingFileUploads.firstIndex(where: { $0.id == pendingId }),
              !pendingFileUploads[index].data.isEmpty else {
            // Nothing to resend (an unreadable/oversize pick) — just clear it.
            pendingFileUploads.removeAll { $0.id == pendingId }
            return
        }
        pendingFileUploads[index].failure = nil
        Task { await performUpload(pendingId: pendingId) }
    }

    func cancelPendingUpload(pendingId: UUID) {
        pendingFileUploads.removeAll { $0.id == pendingId }
    }

    private func performUpload(pendingId: UUID) async {
        guard let issueId = issue?.id,
              let pending = pendingFileUploads.first(where: { $0.id == pendingId })
        else { return }
        do {
            let uploaded = try await attachmentsApi.upload(
                accountId: accountId,
                issueId: issueId,
                data: pending.data,
                filename: pending.filename,
                contentType: pending.contentType
            )
            guard let index = pendingFileUploads.firstIndex(where: { $0.id == pendingId }) else { return }
            // Hold the placeholder until the synced row lands (applyAttachments
            // drops it), but release the bytes now.
            pendingFileUploads[index].uploadedId = uploaded.id
            if fileAttachments.contains(where: { $0.id == uploaded.id }) {
                pendingFileUploads.remove(at: index)
            }
        } catch {
            guard let index = pendingFileUploads.firstIndex(where: { $0.id == pendingId }) else { return }
            pendingFileUploads[index].failure = error.localizedDescription
        }
    }

    /// Delete an attachment. The server also rewrites any markdown reference to
    /// it into a plain-text placeholder; the row's removal reaches this list
    /// through Electric sync.
    func deleteAttachment(id: String) async {
        deletingAttachmentIds.insert(id)
        defer { deletingAttachmentIds.remove(id) }
        do {
            try await attachmentsApi.delete(accountId: accountId, attachmentId: id)
        } catch {
            self.error = error.localizedDescription
        }
    }

    /// Download an attachment's bytes into a per-attachment temp folder and
    /// return the local file URL for Quick Look. Cached: a second tap on the
    /// same row reuses the file instead of re-fetching.
    func downloadForPreview(_ attachment: AttachmentEntity) async -> URL? {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("attachments", isDirectory: true)
            .appendingPathComponent(attachment.id, isDirectory: true)
        let destination = directory
            .appendingPathComponent(AttachmentFiles.sanitizedFilename(attachment.filename))

        if FileManager.default.fileExists(atPath: destination.path) { return destination }
        do {
            let data = try await attachmentsApi.download(
                accountId: accountId,
                relativeUrl: attachment.url
            )
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
            try data.write(to: destination, options: .atomic)
            return destination
        } catch {
            self.error = error.localizedDescription
            return nil
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

    /// This issue's team's statuses in render order (EXP-314), degrading to
    /// the constructed builtin defaults while the shape is still syncing.
    var teamStatuses: [ResolvedIssueStatus] {
        guard let teamId = board?.teamId else { return IssueStatusResolver.builtinFallbackTeam }
        return IssueStatusResolver.teamStatusesOrFallback(statusRows.filter { $0.teamId == teamId })
    }

    /// The issue's status, resolved against the team's rows. Never fails.
    var resolvedStatus: ResolvedIssueStatus {
        guard let issue else { return IssueStatusResolver.builtinDefault(for: .backlog) }
        return IssueStatusResolver.resolve(issue, team: teamStatuses)
    }

    /// Enum-only write, kept for the anchor-driven paths (duplicate marking).
    func setStatus(_ status: IssueStatus) async {
        guard let issue else { return }
        await update(UpdateIssueInput(id: issue.id, status: status.rawValue))
    }

    /// Status-picker write (EXP-314): the row id, or the anchor enum when the
    /// pick is a CONSTRUCTED default with no row behind it.
    func setStatus(_ resolved: ResolvedIssueStatus) async {
        guard let issue else { return }
        if let rowId = resolved.rowId {
            await update(UpdateIssueInput(id: issue.id, statusId: rowId))
        } else {
            await update(UpdateIssueInput(id: issue.id, status: resolved.anchor.rawValue))
        }
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
                .filter { $0.id != issueId }
                .sorted { $0.updatedAt > $1.updatedAt }
        }
        return result ?? []
    }

    /// Candidate issues for the unified Start-coding sheet (EXP-156): every
    /// eligible issue in the current issue's team, the current issue pinned
    /// first (pre-checked) and the rest by recency. Eligibility = the issue's
    /// board is repo-backed, its status isn't terminal (done/cancelled/
    /// duplicate) and its PR isn't merged. The CURRENT issue is exempt from
    /// the issue-level checks (terminal / merged) so it always appears — you
    /// opened the card from it. One-shot read; the sheet is transient.
    /// (Trashed boards never reach the local store, so "not deleted" is
    /// implicit.)
    func startCodingCandidates() async -> [StartCodingSheet.IssueOption] {
        guard let issue, let pool = try? db.pool(forAccountId: accountId) else { return [] }
        let currentId = issue.id
        let currentBoardId = issue.boardId
        let result: [StartCodingSheet.IssueOption]? = try? await pool.read { db in
            guard let current = try BoardEntity.fetchOne(db, key: currentBoardId) else { return [] }
            let boards = try BoardEntity
                .filter(Column("team_id") == current.teamId)
                .fetchAll(db)
            // boardId → repositoryId for repo-backed boards. A repo-LESS
            // board isn't in the map, so neither it nor its issues are
            // eligible.
            var repoByBoard: [String: String] = [:]
            for board in boards {
                guard let repoId = board.repositoryId else { continue }
                repoByBoard[board.id] = repoId
            }
            // ANCHOR set (EXP-314): custom statuses anchor to one of these
            // enum values, so the check keeps gating them correctly.
            let terminal: Set<String> = [
                IssueStatus.done.rawValue,
                IssueStatus.cancelled.rawValue,
                IssueStatus.duplicate.rawValue,
            ]
            let rows = try IssueEntity
                .filter(Array(repoByBoard.keys).contains(Column("board_id")))
                .fetchAll(db)
                .filter { row in
                    // The current issue is force-included as long as its board
                    // is repo-backed — exempt from the terminal / merged rules
                    // so a checked pre-seed is never a stray. A repo-LESS
                    // current issue isn't in repoByBoard and correctly stays
                    // out of the pool entirely.
                    if row.id == currentId {
                        return repoByBoard[row.boardId] != nil
                    }
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
                    repositoryId: repoByBoard[row.boardId],
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
