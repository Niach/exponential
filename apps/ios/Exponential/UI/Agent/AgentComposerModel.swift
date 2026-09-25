import ExpCore
import ExpUI
import Foundation
import GRDB

/// EXP-825: the ONE launcher's state — what the Agent page composer is about
/// to start, and the send. It replaced the three-tab Start-coding sheet and
/// the Create-action sheet (every play button navigates here with an
/// `AgentComposerSeed` now), so the rules those two carried live here:
///
/// - The SUBJECT is issue chips OR one action chip; picking the other kind
///   REPLACES the current subject (exclusivity by swap, no disabled
///   controls). No subject = a chat over the hidden `builtin:chat` action.
/// - The free text is OPTIONAL additional instructions once a subject is
///   picked, the chat prompt when there is none, and REQUIRED for the Chat
///   and Create action builtins (`AgentComposerPrompt`). Images ride it as
///   the steer embed format, uploaded BEFORE the start to the team route.
/// - The machine pool, the per-machine option seeding (`LaunchOptionsState`),
///   the single-repository and batch-cap guards, the resume offer (EXP-481)
///   and the typed action inputs (`ActionInputValues`) are the sheet's,
///   unchanged.
/// - A start is only a COMMAND: the shared `StartedRunWatcher` waits for the
///   desktop's synced row and the page pushes the live session (EXP-536).
///
/// The pools (issues, actions, boards, pull requests, devices, worktrees)
/// read LIVE off the page's `AgentsViewModel` observations; only the repo
/// registry (tRPC) and the `@` vocabulary are fetched once on load.
@MainActor @Observable
final class AgentComposerModel {
    let teamId: String?
    private let accountId: String
    private let deps: AppDependencies
    /// The page's sessions/devices view model — the composer's pools.
    let sessions: AgentsViewModel

    /// The draft: a one-block editor so `@` members, `#` issues and `:`
    /// emoji complete here exactly as they do in a comment (EXP-802).
    let draftEditor = IssueEditorModel()
    /// EXP-892 — the `#` menu's server half, held so its debounce outlives the
    /// keystroke that armed it.
    @ObservationIgnored private var issueRefAugmentor: IssueRefAugmentor?
    let launch = LaunchOptionsState()
    let startWatcher = StartedRunWatcher()

    /// Checked issue ids in pick order (the launch payload order).
    private(set) var checked: [String] = []
    /// The picked action (a team row or a builtin id); nil = issues or chat.
    private(set) var actionId: String?
    /// EXP-981: the DRAFT workflow a `builtin:plan-workflow` subject plans —
    /// seeded by that workflow's Plan button and sent as `workflowId`. Cleared
    /// with the action, because the builtin is meaningless without it.
    private(set) var workflowId: String?
    /// Typed input values keyed by the def's `key`; `""` = cleared. A key
    /// that was never touched resolves to its default (`value(for:)`).
    var inputValues: [String: String] = [:]
    var pendingImages: [PendingSteerImage] = []
    var imageError: String?
    /// EXP-615/739: the chat's OPTIONAL repository — `""` = none.
    var chatRepoId = ""
    /// The explicitly picked machine (nil = the seed's / the default one).
    private(set) var deviceId: String?
    var sending = false
    var error: String?
    /// EXP-897: the blocked-start prompt, up while the reader chooses between
    /// a stacked PR, an ordinary run and cancelling. EXP-980: it asks for a
    /// BATCH too (blockers outside the picked set), and never for a resume.
    var blockedPrompt: BlockedStartPrompt?

    /// The repo registry — one tRPC read; the chat picker and the `repo`
    /// inputs pick from it.
    private(set) var repos: [TeamRepo] = []
    /// A `pr` seed waiting for the pull-request pool (EXP-323): normalised
    /// by MEMBERSHIP when the options exist, never stomping a manual pick.
    private var pendingPrIssueId: String?
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
    /// EXP-897/EXP-980: every `blocks` row and the issues at either end, read
    /// live off GRDB — `IssueGraph` applies the rules. The whole table rather
    /// than one issue's rows: the prompt asks for a BATCH as well, and draws
    /// the TRANSITIVE chain, so the subject no longer bounds the read.
    private var blockerRelations: [IssueRelationEntity] = []
    private var blockerIssues: [IssueEntity] = []
    private var blockerTask: Task<Void, Never>?

    /// A batch run is deliberately loose but not unbounded — one session on
    /// one branch; past this the prompt is unwieldy and token-expensive.
    static let maxBatchIssues = 30
    /// Above this we warn about token cost (still allowed up to the cap).
    static let costWarnThreshold = 6

    init(
        teamId: String?,
        accountId: String,
        deps: AppDependencies,
        sessions: AgentsViewModel,
        seed: AgentComposerSeed
    ) {
        self.teamId = teamId
        self.accountId = accountId
        self.deps = deps
        self.sessions = sessions
        apply(seed)
        configureDraftEditor()
    }

    // MARK: - Seed

    /// The preselection a play button handed over. `action` wins over
    /// `issues`; text lands only in an EMPTY draft; a `pr` seed waits for the
    /// pool; a device seed is a PREFERENCE (sticky until the user picks).
    func apply(_ seed: AgentComposerSeed) {
        if let actionId = seed.actionId {
            self.actionId = actionId
            // EXP-981: rides ONLY with the Plan workflow builtin — the server
            // refuses it beside any other subject.
            workflowId = actionId == DomainContract.builtinPlanWorkflowId
                ? seed.workflowId
                : nil
            checked = []
            inputValues = [:]
        } else if !seed.effectiveIssueIds.isEmpty {
            self.actionId = nil
            checked = seed.effectiveIssueIds
            inputValues = [:]
        }
        if let icon = seed.icon, !icon.isEmpty {
            inputValues["icon"] = icon
        }
        pendingPrIssueId = seed.prIssueId
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
        !touched && pendingImages.isEmpty && draftText == seededDraft
    }

    // MARK: - Load

    func load() async {
        observeBlockers()
        guard let teamId else { return }
        repos = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)) ?? []
        // EXP-993: a phone never picks a chat's repository — the pill is gone,
        // so the team's FIRST repository is the anchor (a team with none still
        // runs in the agent's scratch dir). It used to pre-pick only when the
        // team had exactly one, which left every other team's chat repo-less
        // with no way to say otherwise.
        if chatRepoId.isEmpty, let first = repos.first {
            chatRepoId = first.id
        }
        await loadMentionMembers(teamId: teamId)
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

    // MARK: - Pools (live off the sessions view model)

    /// Every issue the composer can put on a run. The checked ids are exempt
    /// from the issue-level checks: a seeded issue must stay on offer.
    var issues: [IssueOption] {
        sessions.startCandidates(teamId: teamId, exempt: Set(checked))
    }

    /// The action picker's pool: both LISTED builtins pinned FIRST by the
    /// `builtin` flag (EXP-257 — never by sort order), "Fix merge conflicts"
    /// ahead of "Create action" (the web order), then the team's rows. Chat
    /// is in NO pool: it is what "no subject" means.
    var actions: [ActionDto] {
        guard let teamId else { return [] }
        return [
            ActionDto.builtinFixConflictsAction(teamId: teamId),
            ActionDto.builtinCreateAction(teamId: teamId),
        ] + sessions.teamActions(teamId: teamId)
    }

    var boards: [BoardEntity] {
        sessions.teamBoards(teamId: teamId)
    }

    /// Open issue-linked pull requests of the team — the options a `pr`
    /// input picks from (EXP-259/EXP-270).
    var pullRequests: [StartPullRequestOption] {
        sessions.openPullRequests(teamId: teamId)
    }

    // MARK: - Blocked start (EXP-897)

    /// EXP-980: the OPEN issues that block the picked set from OUTSIDE it —
    /// for one issue AND for a batch (a blocker picked into the same batch is
    /// not in its way). An action subject and a chat are never blocked, so
    /// they never ask.
    var openBlockers: [IssueEntity] {
        guard actionId == nil else { return [] }
        return IssueGraph.openBlockersOfSet(
            ids: effectiveChecked, relations: blockerRelations, issues: blockerIssues
        )
    }

    /// EXP-897: the target machine reads the frame's `stack` payload. An
    /// older desktop/CLI has no `stack` field in its decoder and would run
    /// UNSTACKED while the server had already recorded a stack, so the prompt
    /// DISABLES "Stacked PR" for it (EXP-980: disabled, never hidden) and
    /// `startStacked()` refuses to send one.
    var canStackStart: Bool { device?.canStackStart == true }

    /// Why the prompt's "Stacked PR" is off, or nil when it is on — the shared
    /// rule over the picked count, the machine's capability and the graph.
    func stackDisabledReason(_ prompt: BlockedStartPrompt) -> StackStart.StackDisabledReason? {
        StackStart.stackDisabledReason(
            pickedCount: prompt.issueIds.count,
            canStack: canStackStart,
            hasCycle: prompt.graph.hasCycle
        )
    }

    /// Every `blocks` row and the issues at either end. Armed once: the rule
    /// is keyed on the picked SET, which changes without any database change,
    /// so there is nothing per-subject to re-point.
    private func observeBlockers() {
        guard blockerTask == nil, let pool = try? deps.db.pool(forAccountId: accountId)
        else { return }
        let observation = ValueObservation.tracking {
            db -> ([IssueRelationEntity], [IssueEntity]) in
            let relations = try IssueRelationEntity
                .filter(Column("type") == IssueRelationType.blocks.rawValue)
                .fetchAll(db)
            let ids = Array(Set(relations.flatMap { [$0.issueId, $0.relatedIssueId] }))
            let issues = ids.isEmpty
                ? []
                : try IssueEntity.filter(ids.contains(Column("id"))).fetchAll(db)
            return (relations, issues)
        }
        blockerTask = Task { [weak self] in
            do {
                for try await (relations, issues) in observation.values(in: pool) {
                    self?.blockerRelations = relations
                    self?.blockerIssues = issues
                }
            } catch {}
        }
    }

    /// The subject changed under an open prompt: it is about a set that is no
    /// longer picked, so it goes.
    private func refreshBlockers() {
        blockedPrompt = nil
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

    var selectedAction: ActionDto? {
        guard let actionId else { return nil }
        if let picked = actions.first(where: { $0.id == actionId }) { return picked }
        // EXP-981: the Plan workflow builtin is in NO pool — a workflow's Plan
        // button seeds it directly, so it resolves here and nowhere else.
        guard actionId == DomainContract.builtinPlanWorkflowId, let teamId else { return nil }
        return ActionDto.builtinPlanWorkflowAction(teamId: teamId)
    }

    var subject: AgentComposerPrompt.Subject {
        if actionId != nil { return .action }
        let count = effectiveChecked.count
        return count == 0 ? .none : .issues(count: count)
    }

    var submitTitle: String {
        AgentComposerPrompt.submitTitle(for: subject)
    }

    /// EXP-1038: the headline verb over the composer ("Run" · "Implement" ·
    /// "Ask the agent"), the contract's copy ×4 — the subject chips sit
    /// beside it, so what a send starts is the page's main element rather
    /// than a footnote inside the card.
    var headline: String {
        AgentComposerPrompt.headline(for: subject)
    }

    /// The field's prompt per subject (`AgentComposerPrompt.placeholder`,
    /// web parity): a chat asks for the message, a picked action shows its
    /// own composer hint (EXP-825), anything else asks for what is optional
    /// next to it.
    var placeholder: String {
        AgentComposerPrompt.placeholder(
            for: subject, actionHint: selectedAction?.promptPlaceholder
        )
    }

    func isChecked(_ id: String) -> Bool {
        checked.contains(id)
    }

    /// Check or uncheck an issue. Picking an issue REPLACES an action
    /// subject (exclusivity by swap).
    func toggleIssue(_ id: String) {
        touched = true
        if actionId != nil {
            actionId = nil
            workflowId = nil
            inputValues = [:]
        }
        if let index = checked.firstIndex(of: id) {
            checked.remove(at: index)
        } else {
            checked.append(id)
        }
        refreshBlockers()
    }

    /// Pick an action. Picking one REPLACES the issue chips; a different
    /// action's defs start clean (values are keyed per def).
    func pickAction(_ action: ActionDto) {
        guard action.id != actionId else { return }
        touched = true
        actionId = action.id
        // Only a seed can name a workflow; picking another action drops it.
        workflowId = nil
        checked = []
        inputValues = [:]
        refreshBlockers()
    }

    func clearAction() {
        touched = true
        actionId = nil
        workflowId = nil
        inputValues = [:]
        refreshBlockers()
    }

    // MARK: - Action inputs

    /// The value a def shows: what the user set, else its default — a `repo`
    /// input pre-fills the action's bound repository (EXP-349; a picker
    /// reading "None" while the run targets the bound repo anyway looked
    /// misconfigured), a `pr` input the seeded pull request (EXP-323,
    /// normalised by membership — the caller's issue is rarely the option's
    /// representative). A manual clear stores "", which is non-nil, so it is
    /// never re-stomped.
    func value(for def: ActionInputDto) -> String {
        if let value = inputValues[def.key] { return value }
        switch def.type {
        case "repo":
            return selectedAction?.repositoryId ?? ""
        case "pr":
            guard let seed = pendingPrIssueId,
                  let option = StartPullRequestOption.option(in: pullRequests, forIssueId: seed)
            else { return "" }
            return option.issueId
        default:
            return ""
        }
    }

    func setValue(_ value: String, for def: ActionInputDto) {
        touched = true
        inputValues[def.key] = value
    }

    private var selectedActionInputs: [ActionInputDto] {
        selectedAction?.inputs ?? []
    }

    /// The resolved input map (defaults applied) the gates and the wire read.
    private var resolvedInputValues: [String: String] {
        var out: [String: String] = [:]
        for def in selectedActionInputs {
            out[def.key] = value(for: def)
        }
        return out
    }

    // MARK: - Devices

    /// The machines a run can be sent to — online with a runnable agent
    /// (EXP-672: one pool for every subject, the server refuses only on the
    /// agent; EXP-409 drops machines whose every agent is signed out).
    var candidateDevices: [SteerDevice] {
        (sessions.devices ?? []).filter { $0.isOnline && $0.hasRunnableAgent }
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
        // EXP-836: a human pick retires the seed's REQUEST (web's two slots:
        // `setDeviceId` clears `requestedDeviceId`) — a one-shot preference
        // never reasserts itself over a choice, and its note goes with it.
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

    var availableAgents: [String] {
        launch.availableAgents(for: device)
    }

    func selectAgent(_ value: String) {
        touched = true
        launch.selectAgent(value, device: device)
    }

    /// EXP-872: ONE pick for agent + login — the options line has no separate
    /// agent control any more, the account carries its agent.
    func selectAccount(_ option: AccountOption) {
        touched = true
        launch.selectAccount(option, device: device)
    }

    /// EXP-773: the picked agent is outside the picked machine's reported
    /// ACP set — nothing can start there.
    var agentNotReady: Bool {
        device?.agentNotReady(launch.agent) == true
    }

    /// The "nothing to start on" hint — byte-matching the web launch page,
    /// with the signed-out-agents case (EXP-409) taking precedence because it
    /// names an actionable fix.
    var noDeviceNote: String {
        let signedOut = signedOutAgentNames.joined(separator: ", ")
        if !signedOut.isEmpty {
            return "\(signedOut) not signed in on your machines. Sign in on the machine first."
        }
        return "No desktop online. Open the Exponential desktop app to start a run."
    }

    /// EXP-836: why the machine a ▶ seed NAMED is not the one this run would
    /// land on — it went offline, every agent there is signed out, or it left
    /// the registry. Byte-matching web's `deviceRequestNote`; nil once the
    /// request settled (or there never was one), and nil while the devices
    /// shape is still hydrating, because a machine that has not arrived yet is
    /// not a missing one (the request keeps outranking the default until it
    /// does).
    var deviceRequestNote: String? {
        LaunchDeviceRequest.note(
            requested: preferredDeviceId,
            resolved: device?.deviceId,
            devices: sessions.devices
        )
    }

    /// The agents installed-but-signed-out across the caller's ONLINE
    /// machines (EXP-409), in contract order.
    private var signedOutAgentNames: [String] {
        let reported = Set((sessions.devices ?? []).filter(\.isOnline).flatMap(\.unauthedAgentIds))
        return DomainContract.codingAgentValues.filter { reported.contains($0) }
    }

    // MARK: - Resume (EXP-481)

    /// The synced worktree that makes "Resume previous session" offerable:
    /// exactly ONE checked issue, a machine picked off the devices SHAPE
    /// (rowId — relay rows never resume), and a row whose identifier +
    /// .exp-agents marker match the chosen agent. Nil hides the toggle.
    var resumeCandidate: DeviceWorktreeEntity? {
        guard actionId == nil,
              let device,
              let option = checkedOptions.first,
              checkedOptions.count == 1
        else { return nil }
        return WorktreeResume.match(
            worktrees: sessions.worktrees,
            deviceRowId: device.rowId,
            issueIdentifier: option.identifier,
            agent: launch.agent
        )
    }

    var resumeActive: Bool {
        launch.resume && resumeCandidate != nil
    }

    // MARK: - Gates

    private var checkedRepoIds: Set<String> {
        Set(checkedOptions.compactMap(\.repositoryId))
    }

    var multiRepo: Bool { checkedRepoIds.count > 1 }
    var overCap: Bool { effectiveChecked.count > Self.maxBatchIssues }
    var costWarning: Bool { effectiveChecked.count > Self.costWarnThreshold }

    /// The reason the composer cannot start right now, worded for the
    /// caption under the options — nil when it can, or when the only thing
    /// missing is the message itself (the disabled button says that).
    var blocker: String? {
        guard teamId != nil else { return "Pick a team first." }
        // The pool is still resolving (the devices shape's first snapshot):
        // no verdict yet, so no "no desktop" note either.
        guard device != nil else { return sessions.devices == nil ? nil : noDeviceNote }
        if let note = LaunchVocabulary.notReadyNote(device: device, agent: launch.agent) {
            return note
        }
        // A seeded action that never synced, or belongs to another team:
        // say so instead of leaving a dead disabled button.
        if actionId != nil, selectedAction == nil {
            return "That action is not available on this team."
        }
        if multiRepo { return "Pick issues from a single repository per run." }
        if overCap { return "At most \(Self.maxBatchIssues) issues per run. Split the batch." }
        if let action = selectedAction {
            if ActionInputValues.hasUnsupportedType(action.inputs ?? []) {
                return "This action needs a newer app version."
            }
        }
        if !AgentComposerPrompt.withinLimit(draftText) {
            return "The message is too long (\(AgentComposerPrompt.maxLength) characters at most)."
        }
        return nil
    }

    /// Whether the message itself is required and missing: a chat needs text
    /// or an image, the Create action builtin needs the request text.
    private var messageMissing: Bool {
        switch subject {
        case .none:
            return trimmedDraft.isEmpty && pendingImages.isEmpty
        case .action:
            return actionId == DomainContract.builtinCreateActionId && trimmedDraft.isEmpty
        case .issues:
            return false
        }
    }

    private var requiredInputsFilled: Bool {
        ActionInputValues.requiredFilled(selectedActionInputs, values: resolvedInputValues)
    }

    var canSubmit: Bool {
        // `blocker` stays nil while the machine pool is still resolving (no
        // "no desktop" flash) — but nothing can be sent without a machine.
        guard blocker == nil, device != nil, !sending, !messageMissing else { return false }
        switch subject {
        case .none: return true
        case .issues: return !effectiveChecked.isEmpty
        case .action: return selectedAction != nil && requiredInputsFilled
        }
    }

    // MARK: - Images (EXP-511/EXP-698, the steer composer's rules)

    var attachFull: Bool {
        pendingImages.count >= AgentComposerPrompt.maxImages
    }

    /// Queue one normalized image and drop its positional `[Image #k]`
    /// marker at the END of the draft, so a sentence can name the picture it
    /// means (EXP-698). The end, not the caret: the caret is an offset into
    /// the DECORATED text, where a resolved `#EXP-1` chip carries a title
    /// character the sent text does not.
    func queueImage(_ normalized: PendingCommentAttachment) {
        guard !attachFull else { return }
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

    /// Dropping a pending image renumbers the draft's markers: the removed
    /// one goes and every higher one slides down, so `[Image #N]` keeps
    /// naming the N-th image that will actually be sent.
    func removeImage(id: UUID) {
        guard let position = pendingImages.firstIndex(where: { $0.id == id }) else { return }
        pendingImages.remove(at: position)
        draftText = SteerImageMessage.renumberImageMarkers(
            draftText, removedIndex: position + 1
        )
    }

    // MARK: - Submit

    /// EXP-897/EXP-980: a start on BLOCKED work asks first — start anyway, or
    /// build on the blocker's pull request. One issue and a batch both ask;
    /// an action run and a chat never do, and neither does a RESUME (it
    /// re-enters a run whose blockers were answered when it started, the
    /// desktop's rule since EXP-897).
    func submit() {
        guard canSubmit, !sending else { return }
        if !resumeActive {
            let blockers = openBlockers
            if !blockers.isEmpty {
                let picked = effectiveChecked
                blockedPrompt = BlockedStartPrompt(
                    issueIds: picked,
                    identifiers: blockers.map { $0.identifier ?? "" },
                    graph: IssueGraph.blockGraph(
                        subjectIds: picked, relations: blockerRelations, issues: blockerIssues
                    ),
                    issues: blockerIssues
                )
                return
            }
        }
        send(stack: nil)
    }

    /// The reader chose an ordinary run despite the blockers — exactly what
    /// was picked, the whole batch for a batch.
    func startAnyway() {
        blockedPrompt = nil
        send(stack: nil)
    }

    /// The reader chose a stacked pull request: the branch is cut from the
    /// blocker's PR branch and the pull request is based on it. Never sent
    /// while the shared rule disables the choice (a machine without
    /// `stacked-start`, a batch, a cycle) — the button is disabled there, and
    /// this guard keeps a stale tap from downgrading to a plain run.
    func startStacked() {
        guard let prompt = blockedPrompt, stackDisabledReason(prompt) == nil else { return }
        blockedPrompt = nil
        send(stack: true)
    }

    /// Upload the pending images (sequentially, stamping `uploadedId` so a
    /// retry after a mid-batch failure never uploads the same file twice),
    /// compose the `prompt`, then dispatch chat / action / issue / batch.
    /// On success the composer clears and the watcher pushes the run once
    /// its row syncs; on failure the draft, the chips and the strip stay.
    private func send(stack: Bool?) {
        guard canSubmit, let device, let teamId, !sending else { return }
        sending = true
        error = nil
        imageError = nil
        startWatcher.sending()
        Task {
            defer { sending = false }
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
                    imageError = error.userFacingMessage
                    startWatcher.failed(error.userFacingMessage)
                    return
                }
            }
            let prompt = AgentComposerPrompt.build(
                text: draftText,
                attachmentIds: pendingImages.compactMap(\.uploadedId)
            )
            do {
                let key = try await dispatch(
                    device: device, teamId: teamId, prompt: prompt, stack: stack
                )
                startWatcher.begin(
                    key: key,
                    userId: deps.auth.userId,
                    device: device,
                    db: deps.db,
                    accountId: accountId
                )
                draftText = ""
                pendingImages = []
                checked = []
                actionId = nil
                workflowId = nil
                inputValues = [:]
                pendingPrIssueId = nil
                seededDraft = ""
                touched = true
                refreshBlockers()
            } catch {
                self.error = error.userFacingMessage
                startWatcher.failed(error.userFacingMessage)
            }
        }
    }

    /// One `steer.startSession` per subject. Returns the watch key that
    /// recognises the desktop-inserted row (`StartedRunMatch`).
    private func dispatch(
        device: SteerDevice, teamId: String, prompt: String?, stack: Bool? = nil
    ) async throws -> StartedRunKey {
        switch subject {
        case .none:
            // EXP-615: a chat is an ordinary action run of the HIDDEN
            // `builtin:chat` action, constructed locally. `wireValues` drops
            // an empty `repo`, so a "No repository" pick sends none (EXP-756).
            let action = ActionDto.builtinChatAction(teamId: teamId)
            let values = ActionInputValues.wireValues(
                action.inputs ?? [], values: ["repo": chatRepoId]
            )
            try await deps.steerApi.startSession(
                accountId: accountId,
                actionId: action.id,
                deviceId: device.deviceId,
                teamId: action.teamId,
                options: launch.buildOptions(),
                inputs: values.isEmpty ? nil : values,
                prompt: prompt
            )
            return .action(name: action.name)
        case .action:
            guard let action = selectedAction else { throw SteerStartError.rejected("Pick an action.") }
            // Values in wire form: blank optionals dropped (a required blank
            // can't get here — `canSubmit` gates it). `teamId` rides ONLY
            // with the builtins (their virtual rows have no server id).
            let values = ActionInputValues.wireValues(
                action.inputs ?? [], values: resolvedInputValues
            )
            try await deps.steerApi.startSession(
                accountId: accountId,
                actionId: action.id,
                deviceId: device.deviceId,
                teamId: action.isBuiltin ? action.teamId : nil,
                // EXP-981: the Plan workflow builtin names its DRAFT; the
                // server writes the prompt's `Workflow: <uuid>` head itself
                // and refuses the field beside any other action.
                workflowId: action.id == DomainContract.builtinPlanWorkflowId
                    ? workflowId
                    : nil,
                options: launch.buildOptions(),
                inputs: values.isEmpty ? nil : values,
                prompt: prompt
            )
            return .action(name: action.name)
        case .issues:
            let ids = effectiveChecked
            guard let key = StartedRunKey.forIssues(ids) else {
                throw SteerStartError.rejected("Pick an issue.")
            }
            if ids.count > 1 {
                try await deps.steerApi.startSession(
                    accountId: accountId,
                    issueIds: ids,
                    deviceId: device.deviceId,
                    options: launch.buildOptions(),
                    prompt: prompt
                )
            } else {
                // Single-issue only — a batch has no per-issue worktree to
                // resume (EXP-481).
                try await deps.steerApi.startSession(
                    accountId: accountId,
                    issueId: ids[0],
                    deviceId: device.deviceId,
                    options: launch.buildOptions(resume: resumeActive ? true : nil),
                    prompt: prompt,
                    // EXP-897: single-issue only — the batch form above has no
                    // such field, and the server refuses it there.
                    stack: stack
                )
            }
            return key
        }
    }
}

/// EXP-897/EXP-980: what the blocked-start prompt shows — the work it is
/// about, the identifiers of the blockers outside it, and the transitive chain
/// the sheet draws under the sentence (`IssueGraphView`). A real sheet, not an
/// OS alert: an alert cannot host a graph.
struct BlockedStartPrompt: Identifiable {
    /// The picked issues, in pick order.
    let issueIds: [String]
    /// The open blockers OUTSIDE the picked set, by identifier.
    let identifiers: [String]
    let graph: IssueGraph.Graph
    /// The synced rows the graph names its nodes from.
    let issues: [IssueEntity]

    var id: String { issueIds.joined(separator: ",") }

    /// Two or more picked issues = a batch, which reads differently and can
    /// never be stacked.
    var isBatch: Bool { issueIds.count > 1 }

    /// Byte-identical ×4 (`StackStart`).
    var title: String {
        isBatch ? StackStart.blockedBatchTitle : StackStart.blockedStartTitle
    }
}
