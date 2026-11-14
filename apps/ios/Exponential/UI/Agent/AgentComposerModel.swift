import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-825/EXP-1291: the ONE launcher's state — what Home's composer is
/// about to start, and the send. Every play button navigates here with an
/// `AgentComposerSeed`; the send is ONE `sessions.send` that starts a new
/// CHAT root in the ACTIVE team (phones: the current board's team):
///
/// - The SUBJECT is issue chips, OR one prompt chip (NEW-2: picked in the
///   `/` menu, sent as `promptId`); picking issues or a prompt REPLACES the
///   current subject and the agent row's recipient (exclusivity by swap, no
///   disabled controls). No subject = the text is the request. The agent row
///   (S-3, composer-home.json `agentRow`: `pickAgent` / `clearAgent` =
///   Temporary session) picks the RECIPIENT: EXP-1450 a pick keeps the issue
///   chips and drops only a prompt (`pickCases`).
/// - S-3 SEND TO AN AGENT: while `agentId` is set, `submit()` goes through
///   `sendToAgent` = `sessions.send({agentId, text, attachmentIds, issueIds})` into the
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
///   the page opens the chat at once (`landedRootId`); the device picks
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

    /// EXP-1450: the subject (issue chips or one prompt) and the agent
    /// row's recipient, apart from the launch options: no control re-inits
    /// it, an agent pick keeps the issue chips (`AgentComposerSubjectState`).
    private(set) var subjectState = AgentComposerSubjectState() {
        didSet { if subjectState.checked != oldValue.checked { rebuildIssueOptions() } }
    }
    /// Checked issue ids in pick order (the send's `issueIds` order).
    var checked: [String] { subjectState.checked }
    /// The picked prompt (NEW-2); nil = issues or a free request.
    var promptId: String? { subjectState.promptId }
    /// The agent row's picked agent (S-3); nil = Temporary session.
    var agentId: String? { subjectState.agentId }
    var pendingImages: [PendingSteerImage] = []
    /// Wave D: non-image files, uploaded beside the images and sent as
    /// `[<filename>](/api/attachments/<id>)` lines after the embeds.
    var pendingFiles: [PendingSteerFile] = []
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

    /// The new chat root a send started — the page opens it while visible
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
    /// #EXP-1714: the synced teams' ids (`agentsInTeam`).
    private var syncedTeamIds: [String] = []
    /// #EXP-1715: the viewer's synced teams (the Teams pick's rows) and the
    /// pick itself (nil = every team).
    private var teamRows: [TeamEntity] = []
    var teamsPicked: [String]?
    private var pullRequestRows: [StartPullRequestOption.Candidate] = []
    @ObservationIgnored private var poolTask: Task<Void, Never>?

    /// At most this many issues per chat (contract `sessionsSend.maxIssues`).
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
        subjectState.seed(issueIds: seed.effectiveIssueIds, promptId: seed.promptId)
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
    /// EXP-1450: a composer built before ANY team resolved had no team to
    /// leave behind: its subject (a play button's issues) comes along.
    func carryDraft(from previous: AgentComposerModel) {
        if previous.teamId == nil { subjectState = previous.subjectState }
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
            let teams = try TeamEntity.fetchAll(db)
            let pulls = try PullRequestEntity
                .filter(Column("state") == DomainContract.prStateOpen)
                .fetchAll(db)
            let links = try PullRequestIssueEntity.fetchAll(db)
            return (issues, boards, prompts, (agents, teams), pulls, links)
        }
        poolTask = Task { [weak self] in
            do {
                for try await (issues, boards, prompts, agents, pulls, links) in observation.values(in: pool) {
                    guard let self else { return }
                    self.issueRows = issues
                    self.boardRows = boards
                    self.rebuildIssueOptions()
                    self.promptRows = prompts
                    self.agentRows = agents.0
                    self.syncedTeamIds = agents.1.map(\.id)
                    self.teamRows = agents.1
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

    /// Every issue the composer can put on a chat. The checked ids are
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
    /// then name) — what `agentId` must name. #EXP-1714: plus the viewer's
    /// global Assistant (`agentsInTeam`).
    var agents: [AgentDefinitionEntity] {
        guard let teamId else { return [] }
        return AgentSettings.agentsInTeam(
            agentRows.sorted { ($0.sortOrder, $0.name) < ($1.sortOrder, $1.name) },
            teamId: teamId, teamIds: syncedTeamIds, me: deps.auth.userId
        )
    }

    /// #EXP-1715 (composer-menu.json `teamsPick`): the Teams pick while the
    /// agent row picks the viewer's OWN Assistant; nil for anyone else (no
    /// `scopeTeamIds` on the wire then).
    var teamsPick: ComposerTeamsBinding? {
        guard let agentId, let agent = agentRows.first(where: { $0.id == agentId }),
              AgentSettings.isMyAssistant(agent, me: deps.auth.userId)
        else { return nil }
        return ComposerTeamsBinding(
            teams: ComposerTeamsBinding.teams(teamRows),
            picked: teamsPicked,
            onChange: { [weak self] next in self?.teamsPicked = next }
        )
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

    /// EXP-1450: issue chips stay the subject (headline + chips) with an
    /// agent picked; `.agent` = an agent picked over no subject.
    var subject: AgentComposerPrompt.Subject {
        if promptId != nil { return .prompt }
        let count = effectiveChecked.count
        if count > 0 { return .issues(count: count) }
        return agentId != nil ? .agent : .none
    }

    var submitTitle: String {
        AgentComposerPrompt.submitTitle(for: agentId != nil ? .agent : subject)
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
        if agentId != nil || subject == AgentComposerPrompt.Subject.none {
            return HomeGreeting.placeholder
        }
        return AgentComposerPrompt.placeholder(for: subject)
    }

    func isChecked(_ id: String) -> Bool {
        checked.contains(id)
    }

    /// Check or uncheck an issue. Picking an issue REPLACES a prompt or the
    /// agent row's recipient (exclusivity by swap).
    func toggleIssue(_ id: String) {
        touched = true
        subjectState.toggleIssue(id)
    }

    /// NEW-2: the `/` menu's pick — the prompt becomes the subject, REPLACING
    /// the issue chips or an agent.
    func pickPrompt(_ id: String) {
        touched = true
        subjectState.pickPrompt(id)
    }

    func clearPrompt() {
        touched = true
        subjectState.clearPrompt()
    }

    /// S-3: the agent row's pick — the send goes to that agent
    /// (`sendToAgent`). EXP-1450 (`agentRow.pickCases`): the issue chips
    /// STAY (forwarded as `issueIds`); only a prompt goes.
    func pickAgent(_ id: String) {
        touched = true
        subjectState.pickAgent(id)
    }

    /// S-3: the agent row's Temporary session — today's launch on the same
    /// subject.
    func clearAgent() {
        touched = true
        subjectState.clearAgent()
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
            if multiRepo { return "Pick issues from a single repository per session." }
            if overCap { return "At most \(Self.maxIssues) issues per session." }
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
    /// issue or prompt subject never does (EXP-1450: issue chips forwarded
    /// to a picked agent included).
    private var messageMissing: Bool {
        let bare = trimmedDraft.isEmpty && pendingImages.isEmpty && pendingFiles.isEmpty
        switch subject {
        case .none, .agent: return bare
        case .issues, .prompt: return false
        }
    }

    var canSubmit: Bool {
        guard blocker == nil, !sending, !messageMissing else { return false }
        if agentId != nil { return selectedAgent != nil }
        switch subject {
        case .agent: return selectedAgent != nil
        case .none: return device != nil
        case .issues: return device != nil && !effectiveChecked.isEmpty
        case .prompt: return device != nil && selectedPrompt != nil
        }
    }

    // MARK: - Submit

    /// Upload the pending attachments (sequentially, stamping `uploadedId`
    /// so a retry after a mid-batch failure never uploads the same file
    /// twice), then ONE `sessions.send` for a new chat root. On success
    /// the page opens the chat and `ComposerSend` decides what the field
    /// keeps (EXP-1537: text typed while the send was out stays, only the
    /// sent picks leave); on failure the draft, the chips and the strip stay
    /// as they are now and a toast offers Retry. The send runs on a Task the
    /// model holds (it outlives the page), so backing out never cancels it.
    func submit() {
        guard canSubmit, let teamId else { return }
        let messagedAgentId = agentId
        guard messagedAgentId != nil || device != nil else { return }
        sending = true
        touched = true
        dismissErrorToast()
        // A DM never starts on one of the caller's machines: no starting row.
        if messagedAgentId == nil, let device {
            starting = Starting(deviceLabel: LaunchVocabulary.deviceName(device), since: Date())
        }
        // EXP-1537: what Send saw (the UNTRIMMED text, the picks); the
        // field stays editable while the send is out.
        let sentText = draftText
        // #EXP-1714: a message to an agent uploads through the AGENT's team
        // (the global Assistant's personal one); the pools stay this team's.
        let uploadTeamId = messagedAgentId.flatMap { id in agentRows.first { $0.id == id }?.teamId } ?? teamId
        var images = pendingImages
        var files = pendingFiles
        Task {
            defer {
                sending = false
                starting = nil
            }
            for index in images.indices where images[index].uploadedId == nil {
                do {
                    let uploaded = try await deps.attachmentsApi.uploadTeamSessionImage(
                        accountId: accountId,
                        teamId: uploadTeamId,
                        data: images[index].data,
                        filename: images[index].filename,
                        contentType: images[index].contentType
                    )
                    images[index].uploadedId = uploaded.id
                } catch {
                    restamp(images: images, files: files)
                    failed(verb: "upload", thing: "the attachment", error)
                    return
                }
            }
            // Wave D: then the files, through the same route (any type),
            // idempotent exactly like the images.
            for index in files.indices where files[index].uploadedId == nil {
                do {
                    let uploaded = try await deps.attachmentsApi.uploadTeamSessionImage(
                        accountId: accountId,
                        teamId: uploadTeamId,
                        data: files[index].data,
                        filename: files[index].filename,
                        contentType: files[index].contentType
                    )
                    files[index].stamp(uploaded)
                } catch {
                    restamp(images: images, files: files)
                    failed(verb: "upload", thing: "the attachment", error)
                    return
                }
            }
            restamp(images: images, files: files)
            let sent = ComposerSend.Content(
                text: sentText,
                attachments: images.map(\.id.uuidString) + files.map(\.id.uuidString)
            )
            let imageIds = images.compactMap(\.uploadedId)
            let attachmentIds = imageIds + files.compactMap(\.uploadedId)
            let text = AgentComposerPrompt.build(
                text: sentText,
                attachmentIds: imageIds,
                files: files.compactMap(\.wireFile)
            )
            if let messagedAgentId {
                await sendToAgent(
                    messagedAgentId, text: text, attachmentIds: attachmentIds,
                    issueIds: effectiveChecked,
                    identifiers: checkedOptions.compactMap(\.identifier), sent: sent
                )
                return
            }
            guard let device = self.device else { return }
            do {
                let result = try await deps.sessionsApi.send(
                    accountId: accountId,
                    sendInput(teamId: teamId, deviceId: device.deviceId, text: text, attachmentIds: attachmentIds)
                )
                settleDraft(sent: sent)
                landedRootId = result.sessionId
            } catch {
                failed(verb: "start", thing: "the session", error)
            }
        }
    }

    /// S-3 (composer-home.json `agentRow`): the message goes into the
    /// viewer's conversation with the picked agent —
    /// `sessions.send({agentId, text, attachmentIds, issueIds})`, opened on
    /// first use server-side (EXP-1450: the issue chips forward onto it) —
    /// and `landedAgentId` tells the page to open that chat. The draft
    /// settles (`settleDraft`); the agent pick stays. EXP-1686: the refs lead
    /// the text (`agentForwardText`), so the sealer seals them with it.
    private func sendToAgent(
        _ agentId: String, text: String?, attachmentIds: [String], issueIds: [String], identifiers: [String],
        sent: ComposerSend.Content
    ) async {
        do {
            _ = try await deps.sessionsApi.send(
                accountId: accountId,
                SessionBrief.agentForwardInput(
                    agentId: agentId,
                    text: text,
                    attachmentIds: attachmentIds,
                    issueIds: issueIds,
                    identifiers: identifiers,
                    // #EXP-1715: the Assistant's Teams pick, every send.
                    scope: teamsPick?.scope
                )
            )
            settleDraft(sent: sent)
            landedAgentId = agentId
        } catch {
            failed(verb: "send", thing: "the message", error)
        }
    }

    /// The new chat root this draft starts.
    private func sendInput(
        teamId: String, deviceId: String, text: String?, attachmentIds: [String]
    ) -> SessionsSendInput {
        let attachments = attachmentIds.isEmpty ? nil : attachmentIds
        switch subject {
        case .none:
            return .newChat(
                teamId: teamId,
                deviceId: deviceId,
                options: launch.buildOptions(),
                text: text,
                attachmentIds: attachments,
                repositoryId: chatRepoId.isEmpty ? nil : chatRepoId
            )
        case .issues:
            return .newChat(
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
            return .newChat(
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

    /// Carry the upload stamps onto the picks STILL in the strip (one removed
    /// meanwhile stays gone, one added meanwhile stays unstamped), so a
    /// retry uploads only the rest.
    private func restamp(images: [PendingSteerImage], files: [PendingSteerFile]) {
        let imageStamps = Dictionary(uniqueKeysWithValues: images.map { ($0.id, $0) })
        let fileStamps = Dictionary(uniqueKeysWithValues: files.map { ($0.id, $0) })
        pendingImages = pendingImages.map { imageStamps[$0.id] ?? $0 }
        pendingFiles = pendingFiles.map { fileStamps[$0.id] ?? $0 }
    }

    /// EXP-1537 (`composer-send.json`): the send is out — the CURRENT field
    /// keeps what `ComposerSend` leaves (text typed meanwhile, picks added
    /// meanwhile); the subject clears.
    private func settleDraft(sent: ComposerSend.Content) {
        let after = ComposerSend.afterSend(
            outcome: .sent,
            sent: sent,
            current: ComposerSend.Content(
                text: draftText,
                attachments: pendingImages.map(\.id.uuidString) + pendingFiles.map(\.id.uuidString)
            )
        )
        if after.text != draftText { draftText = after.text }
        let kept = Set(after.attachments)
        pendingImages.removeAll { !kept.contains($0.id.uuidString) }
        pendingFiles.removeAll { !kept.contains($0.id.uuidString) }
        subjectState.clearSubject()
        // S-3: the agent row's pick is no draft: it stays.
        seededDraft = ""
    }
}

// #EXP-1841: THE composer strip (`ComposerAttachmentHost`), the same staging,
// caps, toasts and `[Image #N]` placeholders as every chat's composer.
extension AgentComposerModel: ComposerAttachmentHost {
    func attachmentsStaged() {
        touched = true
    }
}
