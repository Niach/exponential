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
    let launch = LaunchOptionsState()
    let startWatcher = StartedRunWatcher()

    /// Checked issue ids in pick order (the launch payload order).
    private(set) var checked: [String] = []
    /// The picked action (a team row or a builtin id); nil = issues or chat.
    private(set) var actionId: String?
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

    /// The repo registry — one tRPC read; the chat picker and the `repo`
    /// inputs pick from it.
    private(set) var repos: [TeamRepo] = []
    /// A `pr` seed waiting for the pull-request pool (EXP-323): normalised
    /// by MEMBERSHIP when the options exist, never stomping a manual pick.
    private var pendingPrIssueId: String?
    private var preferredDeviceId: String?

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
        }
    }

    // MARK: - Load

    func load() async {
        guard let teamId else { return }
        repos = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)) ?? []
        // EXP-615: one repository pre-picks for a chat (web parity); the
        // picker still offers "No repository".
        if chatRepoId.isEmpty, repos.count == 1 {
            chatRepoId = repos[0].id
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
        draftEditor.issueRefSearch = { query in
            IssueRefLookup.search(query, scope: scope, db: database, accountId: account)
        }
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
        return actions.first { $0.id == actionId }
    }

    var subject: AgentComposerPrompt.Subject {
        if actionId != nil { return .action }
        let count = effectiveChecked.count
        return count == 0 ? .none : .issues(count: count)
    }

    var submitTitle: String {
        AgentComposerPrompt.submitTitle(for: subject)
    }

    /// The field's prompt per subject — a chat asks for the message, a
    /// subject asks for what is optional next to it.
    var placeholder: String {
        switch subject {
        case .none: "What should the agent do?"
        case .issues, .action: "Additional instructions (optional)"
        }
    }

    func isChecked(_ id: String) -> Bool {
        checked.contains(id)
    }

    /// Check or uncheck an issue. Picking an issue REPLACES an action
    /// subject (exclusivity by swap).
    func toggleIssue(_ id: String) {
        if actionId != nil {
            actionId = nil
            inputValues = [:]
        }
        if let index = checked.firstIndex(of: id) {
            checked.remove(at: index)
        } else {
            checked.append(id)
        }
    }

    /// Pick an action. Picking one REPLACES the issue chips; a different
    /// action's defs start clean (values are keyed per def).
    func pickAction(_ action: ActionDto) {
        guard action.id != actionId else { return }
        actionId = action.id
        checked = []
        inputValues = [:]
    }

    func clearAction() {
        actionId = nil
        inputValues = [:]
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
        deviceId = id
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
        launch.selectAgent(value, device: device)
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
        guard blocker == nil, !sending, !messageMissing else { return false }
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

    /// Upload the pending images (sequentially, stamping `uploadedId` so a
    /// retry after a mid-batch failure never uploads the same file twice),
    /// compose the `prompt`, then dispatch chat / action / issue / batch.
    /// On success the composer clears and the watcher pushes the run once
    /// its row syncs; on failure the draft, the chips and the strip stay.
    func submit() {
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
                let key = try await dispatch(device: device, teamId: teamId, prompt: prompt)
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
                inputValues = [:]
                pendingPrIssueId = nil
            } catch {
                self.error = error.userFacingMessage
                startWatcher.failed(error.userFacingMessage)
            }
        }
    }

    /// One `steer.startSession` per subject. Returns the watch key that
    /// recognises the desktop-inserted row (`StartedRunMatch`).
    private func dispatch(
        device: SteerDevice, teamId: String, prompt: String?
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
                    prompt: prompt
                )
            }
            return key
        }
    }
}
