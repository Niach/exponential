import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-825/EXP-1291: the ONE launcher's state — what Home's composer is
/// about to start, and the send. Every play button navigates here with an
/// `AgentComposerSeed`; the send is ONE `sessions.send` that starts a new
/// THREAD root in the ACTIVE team (phones: the current board's team):
///
/// - The SUBJECT is issue chips, OR one prompt chip (NEW-2: picked in the
///   `/` menu, sent as `promptId`), OR the agent the agent row picked (S-3,
///   composer-home.json `agentRow`: `pickAgent` / `clearAgent` = Temporary
///   session); picking issues or a prompt REPLACES the current subject
///   (exclusivity by swap, no disabled controls). No subject = the text is
///   the request.
/// - S-3 SEND TO AN AGENT: while `agentId` is set, `submit()` goes through
///   `sendToAgent` = `sessions.send({agentId, text, attachmentIds})` into the
///   viewer's conversation with that agent and publishes `landedAgentId`
///   (the page opens that agent's chat and consumes it). The pick STAYS
///   after the send (the row never resets itself); the "+" hides its runner
///   and work rows meanwhile (`temporary` false).
/// - The free text is OPTIONAL with a subject and required without one
///   (`AgentComposerPrompt`). Images and files ride it as the steer embed
///   format, uploaded BEFORE the send to the team route, their ids sent
///   beside it.
/// - The machine pool (the Devices model's devices), the per-machine option
///   seeding (`LaunchOptionsState`) and the single-repository and issue-cap
///   guards live here. An agent message needs none of them: it runs on the
///   agent's own device and login.
/// - The database is the bus: the call answers with the new root's id, so
///   the page opens the thread at once (`landedRootId`); the device picks
///   the queued row up from its shape.
///
/// The pools (issues, boards, prompts, agents, pull requests) read LIVE off GRDB;
/// only the repo registry (tRPC) and the `@` vocabulary are fetched once.
@MainActor @Observable
final class AgentComposerModel {
    let teamId: String?
    private let accountId: String
    private let deps: AppDependencies
    /// The Devices tab's model — the composer's machine pool.
    let devicesModel: AgentsViewModel

    /// The draft: a one-block editor so `@` members, `#` issues and `:`
    /// emoji complete here exactly as they do in a comment (EXP-802).
    let draftEditor = IssueEditorModel()
    /// EXP-892 — the `#` menu's server half, held so its debounce outlives the
    /// keystroke that armed it.
    @ObservationIgnored private var issueRefAugmentor: IssueRefAugmentor?
    let launch = LaunchOptionsState()

    /// Checked issue ids in pick order (the send's `issueIds` order).
    private(set) var checked: [String] = [] {
        didSet { if checked != oldValue { rebuildIssueOptions() } }
    }
    /// The picked prompt (NEW-2); nil = another subject or a free request.
    private(set) var promptId: String?
    /// The picked agent (NEW-3); nil = another subject or a free request.
    private(set) var agentId: String?
    var pendingImages: [PendingSteerImage] = []
    /// Wave D: non-image files, uploaded beside the images and sent as
    /// `[<filename>](/api/attachments/<id>)` lines after the embeds.
    var pendingFiles: [PendingSteerFile] = []
    var imageError: String?
    /// EXP-615/739: a free request's repository — the team's FIRST one (a
    /// phone never picks it, EXP-993); `""` = none (a scratch dir).
    var chatRepoId = ""
    /// The explicitly picked machine (nil = the seed's / the default one).
    private(set) var deviceId: String?
    var sending = false
    /// EXP-1271 (feedback-states.json `starting`): from the Start press until
    /// `sessions.send` answers. The card shows the mark + "Starting on
    /// <device>" + the elapsed seconds; the draft stays put meanwhile, so a
    /// refusal leaves it ready to retry.
    private(set) var starting: Starting?

    struct Starting: Equatable {
        let deviceLabel: String
        let since: Date
    }

    /// The new thread root a send started — the page opens it while visible
    /// and consumes it.
    var landedRootId: String?
    /// NEW-3: the agent whose DM a send went into — the page opens it while
    /// visible and consumes it.
    var landedAgentId: String?
    /// The error toast still up, dismissed by the next send.
    private var errorToastId: UUID?

    /// The repo registry — one tRPC read; a free request and the `repo`
    /// inputs pick from it.
    private(set) var repos: [TeamRepo] = []
    /// EXP-836: the machine a ▶ seed REQUESTED — a one-shot preference that
    /// outranks the default machine while it is a candidate, explains itself
    /// when it is not (`deviceRequestNote`) and is retired by a human pick.
    private var preferredDeviceId: String?
    /// Whether the user has acted on this composer (picked, typed, attached,
    /// sent): the page rebuilds a PRISTINE composer with its original seed
    /// when the team resolves late, a touched one with an empty seed.
    private var touched = false
    /// The text a seed dropped into the draft — a draft that still equals
    /// it has not been typed in.
    private var seededDraft = ""

    /// The live pools, read off GRDB.
    private var issueRows: [IssueEntity] = []
    private var boardRows: [BoardEntity] = []
    private var promptRows: [PromptEntity] = []
    private var agentRows: [AgentDefinitionEntity] = []
    private var pullRequestRows: [StartPullRequestOption.Candidate] = []
    @ObservationIgnored private var poolTask: Task<Void, Never>?

    /// At most this many issues per thread (contract `sessionsSend.maxIssues`).
    static let maxIssues = DomainContract.sessionsSendMaxIssues

    init(
        teamId: String?,
        accountId: String,
        deps: AppDependencies,
        devicesModel: AgentsViewModel,
        seed: AgentComposerSeed
    ) {
        self.teamId = teamId
        self.accountId = accountId
        self.deps = deps
        self.devicesModel = devicesModel
        apply(seed)
        configureDraftEditor()
    }

    // MARK: - Seed

    /// The preselection a play button handed over. A prompt wins over
    /// `issues`; text lands only in an EMPTY draft; a device seed is a
    /// PREFERENCE (sticky until the user picks).
    func apply(_ seed: AgentComposerSeed) {
        if let promptId = seed.promptId {
            self.promptId = promptId
            agentId = nil
            checked = []
        } else if !seed.effectiveIssueIds.isEmpty {
            promptId = nil
            agentId = nil
            checked = seed.effectiveIssueIds
        }
        if let deviceId = seed.deviceId {
            preferredDeviceId = deviceId
        }
        if let text = seed.text, !text.isEmpty, draftText.isEmpty {
            draftText = text
            seededDraft = text
        }
    }

    /// Nothing picked, typed, attached or sent since the seed was applied.
    var isPristine: Bool {
        !touched && pendingImages.isEmpty && pendingFiles.isEmpty && draftText == seededDraft
    }

    /// A send is on the wire (the uploads, the request): the page keeps this
    /// composer through a team change ONLY for that window and rebinds once
    /// it ends.
    var isStartInFlight: Bool {
        sending
    }

    /// The page is letting this composer go for another team's.
    func release() {
        poolTask?.cancel()
        poolTask = nil
        dismissErrorToast()
    }

    /// The text and the attachments of the composer this one replaces (a
    /// team change under a draft in progress): what is not bound to a team.
    /// The subject, the inputs and the repo pick were the old team's and
    /// stay behind; the uploads are redone under the new team at the send.
    func carryDraft(from previous: AgentComposerModel) {
        draftText = previous.draftText
        pendingImages = previous.pendingImages.map { image in
            var copy = image
            copy.uploadedId = nil
            return copy
        }
        pendingFiles = previous.pendingFiles.map { file in
            var copy = file
            copy.uploadedId = nil
            copy.uploadedName = nil
            return copy
        }
        landedRootId = previous.landedRootId
        landedAgentId = previous.landedAgentId
        touched = true
    }

    // MARK: - Load

    func load() async {
        observePools()
        guard let teamId else { return }
        repos = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)) ?? []
        // EXP-993: a phone never picks a free request's repository — the
        // team's FIRST repository is the anchor (a team with none runs in
        // the agent's scratch dir).
        if chatRepoId.isEmpty, let first = repos.first {
            chatRepoId = first.id
        }
        await loadMentionMembers(teamId: teamId)
    }

    /// Issues, boards, prompts, agents and the open pull requests (with
    /// their linked issues' identifiers) — one observation, every pool live.
    private func observePools() {
        guard poolTask == nil, let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let observation = ValueObservation.tracking { db in
            let issues = try IssueEntity.fetchAll(db)
            let boards = try BoardEntity.fetchAll(db)
            // NEW-2: the saved prompts and the agent definitions.
            let prompts = try PromptEntity.fetchAll(db)
            let agents = try AgentDefinitionEntity.fetchAll(db)
            let pulls = try PullRequestEntity
                .filter(Column("state") == DomainContract.prStateOpen)
                .fetchAll(db)
            let links = try PullRequestIssueEntity.fetchAll(db)
            return (issues, boards, prompts, agents, pulls, links)
        }
        poolTask = Task { [weak self] in
            do {
                for try await (issues, boards, prompts, agents, pulls, links) in observation.values(in: pool) {
                    guard let self else { return }
                    self.issueRows = issues
                    self.boardRows = boards
                    self.rebuildIssueOptions()
                    self.promptRows = prompts
                    self.agentRows = agents
                    let identifierById = Dictionary(
                        issues.map { ($0.id, $0.identifier ?? "") }, uniquingKeysWith: { a, _ in a }
                    )
                    let linked = Dictionary(grouping: links, by: \.pullRequestId)
                    self.pullRequestRows = pulls.map { pull in
                        StartPullRequestOption.Candidate(
                            id: pull.id,
                            teamId: pull.teamId,
                            number: pull.number,
                            state: pull.state,
                            identifiers: (linked[pull.id] ?? []).compactMap { identifierById[$0.issueId] }
                        )
                    }
                }
            } catch {}
        }
    }

    // MARK: - Draft

    var draftText: String {
        get { draftEditor.plainText }
        set { draftEditor.setPlainText(newValue) }
    }

    var trimmedDraft: String {
        draftText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Point the draft editor's typeaheads at this team: `#` refs resolve
    /// through the `.team` scope (there is no issue or board to derive one
    /// from), `:` emoji need nothing scoped — the same wiring the steer
    /// composer does (EXP-802).
    private func configureDraftEditor() {
        guard let teamId else { return }
        let scope = IssueRefLookup.Scope.team(id: teamId)
        let database = deps.db
        let account = accountId
        draftEditor.issueRefResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: scope, db: database, accountId: account)?
                .issueId
        }
        draftEditor.issueRefTitleResolver = { identifier in
            IssueRefChipCache.chip(identifier, scope: scope, db: database, accountId: account)?
                .title
        }
        draftEditor.issueRefStatusResolver = { identifier in
            IssueRefChipCache.statusInfo(
                identifier, scope: scope, db: database, accountId: account)
        }
        // EXP-892: the locally ranked rows render instantly; a debounced
        // `issues.search` splices the server's full-text hits in behind them.
        let augmentor = IssueRefAugmentor(
            scope: scope,
            db: database,
            accountId: account,
            issuesApi: deps.issuesApi
        )
        issueRefAugmentor = augmentor
        augmentor.attach(to: draftEditor)
        EmojiCatalog.shared.preload()
        draftEditor.emojiSearch = { query in
            EmojiCatalog.shared.search(query, limit: EmojiCatalog.typeaheadLimit)
        }
        draftEditor.onEmojiInserted = { record in
            EmojiPreferences().recordRecent(record.unicode)
        }
    }

    /// The `@` vocabulary is membership (EXP-487): one read of the two synced
    /// tables — both whole and small.
    private func loadMentionMembers(teamId: String) async {
        guard let pool = try? deps.db.pool(forAccountId: accountId) else { return }
        let rows = try? await pool.read { db -> ([UserEntity], [TeamMemberEntity]) in
            (try UserEntity.fetchAll(db), try TeamMemberEntity.fetchAll(db))
        }
        guard let (users, members) = rows else { return }
        let memberIds = Set(members.filter { $0.teamId == teamId }.map(\.userId))
        draftEditor.mentionMembers = users
            .filter { memberIds.contains($0.id) }
            .map { MentionMember(name: $0.name ?? $0.email, email: $0.email) }
    }

    // MARK: - Pools

    /// Every issue the composer can put on a thread. The checked ids are
    /// exempt from the issue-level checks: a seeded issue must stay on offer.
    /// EXP-1358: built once per pool emission or `checked` change — it was a
    /// computed property every keystroke's readers (`canSubmit`, `subject`,
    /// `placeholder`, …) rebuilt from every synced issue, about ten times.
    private(set) var issues: [IssueOption] = []

    private func rebuildIssueOptions() {
        issues = IssueOption.build(issues: issueRows, boards: boardRows, teamId: teamId, exempt: Set(checked))
    }

    /// The `/` menu's pool: the team's saved prompts, sortOrder-then-name
    /// like the server list.
    var prompts: [PromptEntity] {
        guard let teamId else { return [] }
        return promptRows
            .filter { $0.teamId == teamId }
            .sorted { ($0.sortOrder, $0.name) < ($1.sortOrder, $1.name) }
    }

    /// The team's agents in their order (`SessionQueries.agents`: sortOrder,
    /// then name) — what `agentId` must name.
    var agents: [AgentDefinitionEntity] {
        guard let teamId else { return [] }
        return agentRows
            .filter { $0.teamId == teamId }
            .sorted { ($0.sortOrder, $0.name) < ($1.sortOrder, $1.name) }
    }

    /// The `/` menu over the draft: the team's prompts alone (no session
    /// yet, so no builtins).
    var slashPrompts: [SlashPrompt] {
        SlashCommands.promptMatches(
            draft: draftText,
            prompts: prompts.map { SlashPrompt(id: $0.id, name: $0.name, icon: $0.icon) }
        )
    }

    /// The team's boards, sortOrder-then-name — the `board` inputs pick from
    /// them.
    var boards: [BoardEntity] {
        guard let teamId else { return [] }
        return boardRows
            .filter { $0.teamId == teamId }
            .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
    }

    /// The team's OPEN pull requests — the options a `pr` input picks from.
    var pullRequests: [StartPullRequestOption] {
        guard let teamId else { return [] }
        return StartPullRequestOption.build(from: pullRequestRows, teamId: teamId)
    }

    // MARK: - Subject

    /// Checked ids that are actually in the pool. A stray (an id whose row
    /// has not synced, or is not eligible) is never shown, counted,
    /// validated or submitted.
    var effectiveChecked: [String] {
        let pool = Set(issues.map(\.id))
        return checked.filter { pool.contains($0) }
    }

    var checkedOptions: [IssueOption] {
        let byId = Dictionary(issues.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return effectiveChecked.compactMap { byId[$0] }
    }

    var selectedPrompt: PromptEntity? {
        guard let promptId else { return nil }
        return prompts.first(where: { $0.id == promptId })
    }

    var selectedAgent: AgentDefinitionEntity? {
        guard let agentId else { return nil }
        return agents.first(where: { $0.id == agentId })
    }

    var subject: AgentComposerPrompt.Subject {
        if agentId != nil { return .agent }
        if promptId != nil { return .prompt }
        let count = effectiveChecked.count
        return count == 0 ? .none : .issues(count: count)
    }

    var submitTitle: String {
        AgentComposerPrompt.submitTitle(for: subject)
    }

    /// EXP-1038: the headline verb over the composer ("Implement"; a prompt's
    /// or an agent's chip is the headline itself), the contract's copy ×4.
    var headline: String {
        AgentComposerPrompt.headline(for: subject)
    }

    /// The field's prompt per subject: Home asks what to build
    /// (composer-home.json `placeholder`); any subject asks for what is
    /// optional next to it.
    var placeholder: String {
        // S-3: an agent picked on the agent row is no subject chip: the card
        // keeps the home placeholder (composer-home.json `placeholder`).
        if subject == AgentComposerPrompt.Subject.none || subject == AgentComposerPrompt.Subject.agent {
            return HomeGreeting.placeholder
        }
        return AgentComposerPrompt.placeholder(for: subject)
    }

    func isChecked(_ id: String) -> Bool {
        checked.contains(id)
    }

    /// Check or uncheck an issue. Picking an issue REPLACES a prompt or an
    /// agent subject (exclusivity by swap).
    func toggleIssue(_ id: String) {
        touched = true
        promptId = nil
        agentId = nil
        if let index = checked.firstIndex(of: id) {
            checked.remove(at: index)
        } else {
            checked.append(id)
        }
    }

    /// NEW-2: the `/` menu's pick — the prompt becomes the subject, REPLACING
    /// the issue chips or an agent.
    func pickPrompt(_ id: String) {
        touched = true
        promptId = id
        agentId = nil
        checked = []
    }

    func clearPrompt() {
        touched = true
        promptId = nil
    }

    /// S-3: the agent row's pick — the send goes to that agent
    /// (`sendToAgent`), REPLACING the issue chips or a prompt.
    func pickAgent(_ id: String) {
        touched = true
        agentId = id
        promptId = nil
        checked = []
    }

    /// S-3: the agent row's Temporary session — today's launch.
    func clearAgent() {
        touched = true
        agentId = nil
    }

    // MARK: - Devices

    /// The machines a session can be sent to — online with a runnable agent
    /// (EXP-672; EXP-409 drops machines whose every agent is signed out).
    var candidateDevices: [SteerDevice] {
        (devicesModel.devices ?? []).filter { $0.isOnline && $0.hasRunnableAgent }
    }

    var device: SteerDevice? {
        if let deviceId, let match = candidateDevices.first(where: { $0.deviceId == deviceId }) {
            return match
        }
        if let preferredDeviceId,
           let match = candidateDevices.first(where: { $0.deviceId == preferredDeviceId }) {
            return match
        }
        // EXP-622: the caller's default machine, when it is still a candidate.
        if let match = candidateDevices.first(where: \.isDefaultDevice) {
            return match
        }
        return candidateDevices.first
    }

    func selectDevice(_ id: String) {
        let switched = id != launch.lastSeededDeviceId
        touched = true
        deviceId = id
        // EXP-836: a human pick retires the seed's REQUEST — a one-shot
        // preference never reasserts itself over a choice.
        preferredDeviceId = nil
        // The newly selected desktop may not run the chosen agent; a
        // DIFFERENT machine brings its own coding defaults (EXP-437).
        launch.clampAgent(to: device)
        if switched { launch.seed(from: device) }
    }

    /// After anything that can implicitly re-resolve `device` (a heartbeat
    /// dropping the picked machine, the pool arriving): the view calls this
    /// on every change of the resolved id.
    func reconcileDevice() {
        launch.reconcile(device: device)
    }

    /// EXP-872: ONE pick for agent + login — the account carries its agent.
    func selectAccount(_ option: AccountOption) {
        touched = true
        launch.selectAccount(option, device: device)
    }

    /// EXP-773: the picked agent is outside the picked machine's reported
    /// ACP set — nothing can start there.
    var agentNotReady: Bool {
        device?.agentNotReady(launch.agent) == true
    }

    /// EXP-1196/1219: when the picked agent cannot start on the picked
    /// machine, that machine's failing doctor ROW — rendered under the
    /// composer with its action. Nil when the machine reports no doctor or
    /// the report names no failure. EXP-1375: a machine that CAN start but
    /// is low on disk shows its disk row instead (a warning, never a block).
    var notReadyRow: DeviceReadiness.Row? {
        guard let device, let doctor = device.doctor else { return nil }
        guard agentNotReady else { return DeviceReadiness.warningRow(doctor, remote: true) }
        return DeviceReadiness.failingRow(
            doctor, agent: launch.agent, remote: true
        )
    }

    /// The "nothing to start on" hint, the signed-out-agents case (EXP-409)
    /// first because it names an actionable fix.
    var noDeviceNote: String {
        let signedOut = signedOutAgentNames.joined(separator: ", ")
        if !signedOut.isEmpty {
            return "\(signedOut) not signed in on your machines. Sign in on the machine first."
        }
        return "No desktop online. Open the Exponential desktop app to start a session."
    }

    /// EXP-836: why the machine a ▶ seed NAMED is not the one this session
    /// would land on; nil once the request settled (or there never was one)
    /// and while the devices shape is still hydrating.
    var deviceRequestNote: String? {
        LaunchDeviceRequest.note(
            requested: preferredDeviceId,
            resolved: device?.deviceId,
            devices: devicesModel.devices
        )
    }

    /// The agents installed-but-signed-out across the caller's ONLINE
    /// machines (EXP-409), in contract order.
    private var signedOutAgentNames: [String] {
        let reported = Set((devicesModel.devices ?? []).filter(\.isOnline).flatMap(\.unauthedAgentIds))
        return DomainContract.codingAgentValues.filter { reported.contains($0) }
    }

    // MARK: - Gates

    private var checkedRepoIds: Set<String> {
        Set(checkedOptions.compactMap(\.repositoryId))
    }

    var multiRepo: Bool { checkedRepoIds.count > 1 }
    var overCap: Bool { effectiveChecked.count > Self.maxIssues }

    /// The reason the composer cannot start right now, worded for the
    /// caption under the options — nil when it can, or when the only thing
    /// missing is the message itself (the disabled button says that).
    var blocker: String? {
        guard teamId != nil else { return "Pick a team first." }
        // NEW-3: a DM runs on the agent's own device and login — no machine
        // of the caller's is involved.
        if agentId != nil {
            if selectedAgent == nil { return "That agent is not available on this team." }
            if !AgentComposerPrompt.withinLimit(draftText) {
                return "The message is too long (\(AgentComposerPrompt.maxLength) characters at most)."
            }
            return nil
        }
        // The pool is still resolving (the devices shape's first snapshot):
        // no verdict yet, so no "no desktop" note either.
        guard device != nil else { return devicesModel.devices == nil ? nil : noDeviceNote }
        if let note = LaunchVocabulary.notReadyNote(device: device, agent: launch.agent) {
            return note
        }
        // A seeded prompt that never synced, or belongs to another team.
        if promptId != nil, selectedPrompt == nil {
            return "That prompt is not available on this team."
        }
        if multiRepo { return "Pick issues from a single repository per session." }
        if overCap { return "At most \(Self.maxIssues) issues per session." }
        if !AgentComposerPrompt.withinLimit(draftText) {
            return "The message is too long (\(AgentComposerPrompt.maxLength) characters at most)."
        }
        return nil
    }

    /// A free request and an agent message need text or an attachment; an
    /// issue or prompt subject never does.
    private var messageMissing: Bool {
        let bare = trimmedDraft.isEmpty && pendingImages.isEmpty && pendingFiles.isEmpty
        switch subject {
        case .none, .agent: return bare
        case .issues, .prompt: return false
        }
    }

    var canSubmit: Bool {
        guard blocker == nil, !sending, !messageMissing else { return false }
        switch subject {
        case .agent: return selectedAgent != nil
        case .none: return device != nil
        case .issues: return device != nil && !effectiveChecked.isEmpty
        case .prompt: return device != nil && selectedPrompt != nil
        }
    }

    // MARK: - Attachments (EXP-511/EXP-698, the steer composer's rules)

    /// Both lists full: nothing more can be attached (the "+" row dims).
    var attachFull: Bool {
        imagesFull && filesFull
    }

    var imagesFull: Bool {
        pendingImages.count >= AgentComposerPrompt.maxImages
    }

    var filesFull: Bool {
        pendingFiles.count >= SteerImageMessage.maxFiles
    }

    /// Wave D: a picked file. An inline image joins the images (marker and
    /// all); anything else queues as a file tile.
    func queuePicked(_ picked: PendingCommentAttachment) {
        if AttachmentFiles.isInlineImage(contentType: picked.contentType) {
            guard !imagesFull else { return }
            queueImage(picked)
            return
        }
        guard !filesFull else {
            imageError = SteerImageMessage.filesFullMessage
            return
        }
        touched = true
        pendingFiles.append(PendingSteerFile(
            data: picked.data,
            filename: picked.filename,
            contentType: picked.contentType
        ))
    }

    /// The strip's × — an image (renumbering the markers) or a file.
    func removeAttachment(id: UUID) {
        if pendingImages.contains(where: { $0.id == id }) {
            removeImage(id: id)
        } else {
            pendingFiles.removeAll { $0.id == id }
        }
    }

    /// Queue one normalized image and drop its positional `[Image #k]`
    /// marker at the END of the draft, so a sentence can name the picture it
    /// means (EXP-698).
    func queueImage(_ normalized: PendingCommentAttachment) {
        guard !imagesFull else { return }
        touched = true
        pendingImages.append(PendingSteerImage(
            data: normalized.data,
            filename: normalized.filename,
            contentType: normalized.contentType,
            uploadedId: nil
        ))
        let inserted = SteerImageMessage.insertImageMarker(
            text: draftText,
            caret: (draftText as NSString).length,
            index: pendingImages.count
        )
        draftText = inserted.text
    }

    /// Dropping a pending image renumbers the draft's markers, so
    /// `[Image #N]` keeps naming the N-th image that will actually be sent.
    func removeImage(id: UUID) {
        guard let position = pendingImages.firstIndex(where: { $0.id == id }) else { return }
        pendingImages.remove(at: position)
        draftText = SteerImageMessage.renumberImageMarkers(
            draftText, removedIndex: position + 1
        )
    }

    // MARK: - Submit

    /// Upload the pending attachments (sequentially, stamping `uploadedId`
    /// so a retry after a mid-batch failure never uploads the same file
    /// twice), then ONE `sessions.send` for a new thread root. On success
    /// the composer clears and the page opens the thread; on failure the
    /// draft, the chips and the strip stay and a toast offers Retry.
    func submit() {
        guard canSubmit, let teamId else { return }
        let messagedAgentId = subject == .agent ? agentId : nil
        guard messagedAgentId != nil || device != nil else { return }
        sending = true
        touched = true
        imageError = nil
        dismissErrorToast()
        // A DM never starts on one of the caller's machines: no starting row.
        if messagedAgentId == nil, let device {
            starting = Starting(deviceLabel: LaunchVocabulary.deviceName(device), since: Date())
        }
        Task {
            defer {
                sending = false
                starting = nil
            }
            for index in pendingImages.indices where pendingImages[index].uploadedId == nil {
                do {
                    let uploaded = try await deps.attachmentsApi.uploadTeamSessionImage(
                        accountId: accountId,
                        teamId: teamId,
                        data: pendingImages[index].data,
                        filename: pendingImages[index].filename,
                        contentType: pendingImages[index].contentType
                    )
                    pendingImages[index].uploadedId = uploaded.id
                } catch {
                    failed(verb: "upload", thing: "the attachment", error)
                    return
                }
            }
            // Wave D: then the files, through the same route (any type),
            // idempotent exactly like the images.
            for index in pendingFiles.indices where pendingFiles[index].uploadedId == nil {
                do {
                    let uploaded = try await deps.attachmentsApi.uploadTeamSessionImage(
                        accountId: accountId,
                        teamId: teamId,
                        data: pendingFiles[index].data,
                        filename: pendingFiles[index].filename,
                        contentType: pendingFiles[index].contentType
                    )
                    pendingFiles[index].stamp(uploaded)
                } catch {
                    failed(verb: "upload", thing: "the attachment", error)
                    return
                }
            }
            let imageIds = pendingImages.compactMap(\.uploadedId)
            let attachmentIds = imageIds + pendingFiles.compactMap(\.uploadedId)
            let text = AgentComposerPrompt.build(
                text: draftText,
                attachmentIds: imageIds,
                files: pendingFiles.compactMap(\.wireFile)
            )
            if let messagedAgentId {
                await sendToAgent(messagedAgentId, text: text, attachmentIds: attachmentIds)
                return
            }
            guard let device = self.device else { return }
            do {
                let result = try await deps.sessionsApi.send(
                    accountId: accountId,
                    sendInput(teamId: teamId, deviceId: device.deviceId, text: text, attachmentIds: attachmentIds)
                )
                clearDraft()
                landedRootId = result.sessionId
            } catch {
                failed(verb: "start", thing: "the session", error)
            }
        }
    }

    /// S-3 (composer-home.json `agentRow`): the message goes into the
    /// viewer's conversation with the picked agent —
    /// `sessions.send({agentId, text, attachmentIds})`, opened on first use
    /// server-side — and `landedAgentId` tells the page to open that chat.
    /// The draft clears; the agent pick stays.
    private func sendToAgent(_ agentId: String, text: String?, attachmentIds: [String]) async {
        do {
            _ = try await deps.sessionsApi.send(
                accountId: accountId,
                .agent(
                    agentId: agentId,
                    text: text,
                    attachmentIds: attachmentIds.isEmpty ? nil : attachmentIds
                )
            )
            clearDraft()
            landedAgentId = agentId
        } catch {
            failed(verb: "send", thing: "the message", error)
        }
    }

    /// The new thread root this draft starts.
    private func sendInput(
        teamId: String, deviceId: String, text: String?, attachmentIds: [String]
    ) -> SessionsSendInput {
        let attachments = attachmentIds.isEmpty ? nil : attachmentIds
        switch subject {
        case .none:
            return .newThread(
                teamId: teamId,
                deviceId: deviceId,
                options: launch.buildOptions(),
                text: text,
                attachmentIds: attachments,
                repositoryId: chatRepoId.isEmpty ? nil : chatRepoId
            )
        case .issues:
            return .newThread(
                teamId: teamId,
                deviceId: deviceId,
                options: launch.buildOptions(),
                text: text,
                attachmentIds: attachments,
                issueIds: effectiveChecked
            )
        case .prompt, .agent:
            // NEW-2: the picked saved PROMPT: its body plus `text` = the first
            // message, in the free request's repository. (An agent never gets
            // here: `submit` sends it into the DM.)
            return .newThread(
                teamId: teamId,
                deviceId: deviceId,
                options: launch.buildOptions(),
                text: text,
                attachmentIds: attachments,
                promptId: promptId,
                repositoryId: chatRepoId.isEmpty ? nil : chatRepoId
            )
        }
    }

    /// FEED-79 (EXP-1259 D10): a failed upload or send is a toast with
    /// Retry (the uploads are idempotent); the draft stays.
    private func failed(verb: String, thing: String, _ error: Error) {
        let toast = ToastItem(
            kind: .error,
            title: FeedbackStates.Errors.title(verb: verb, thing: thing),
            description: error.userFacingMessage,
            action: ToastAction(label: FeedbackStates.Starting.timeoutAction) { [weak self] in
                Task { @MainActor in self?.submit() }
            }
        )
        errorToastId = toast.id
        Toaster.shared.show(toast)
    }

    private func dismissErrorToast() {
        guard let id = errorToastId else { return }
        errorToastId = nil
        Toaster.shared.dismiss(id)
    }

    private func clearDraft() {
        draftText = ""
        pendingImages = []
        pendingFiles = []
        checked = []
        promptId = nil
        // S-3: the agent row's pick is no draft: it stays.
        seededDraft = ""
    }
}
