import ExpUI
import ExpCore
import GRDB
import SwiftUI

// The unified Start-coding sheet (EXP-156) — the iOS twin of the desktop IDE's
// single Start-coding dialog. A searchable multi-issue picker (PRESELECTED
// rows pinned first — the pin order is snapshotted at open and never re-sorts
// on toggle, so a tapped row visibly checks in place instead of teleporting
// into a pinned group, EXP-241) over Agent / Model / Effort pickers, an
// ultracode toggle (it IS `--effort ultracode`, so it disables the Effort
// picker), a plan-mode toggle, plus a desktop picker when more than one is
// online. 1 checked issue launches a plain single-issue session; 2+ launch
// ONE batch session on a shared `exp/batch-<id8>` branch ending in ONE
// combined PR.
//
// EXP-257: hosts that pass `teamId` + `onRunAction` get a top segmented
// Issues | Actions control. Actions mode is a searchable single-select action
// list (the "Fix merge conflicts" builtin pinned FIRST by its `builtin` flag;
// "Create action" is not offered — EXP-431 gave it `createActionMode`, a
// dedicated presentation of this sheet: "New action" title, no subject
// switch, no picker, just the create builtin's input fields) over the
// selected action's typed input fields (text / repo /
// board / pr / icon) — the SAME agent/model/effort/toggle options apply, action
// runs are no longer Claude-only. Device candidates in Actions mode need the
// `actions` capability, plus `action-inputs` when the selected action is
// builtin or declares inputs.
//
// EXP-201: the desktop runs three coding agents (claude / codex / pi). The
// agent switcher — a brand-icon pill tab strip (EXP-208), the iOS twin of the
// desktop dialog's agent_tabs — shows only the SELECTED device's agents (an
// old desktop reports none = claude-only, hiding it); model/effort lists, the
// claude-only toggles, and the skip-permissions toggle all follow the agent.
// EXP-409: an advertised agent is RUNNABLE (installed and signed in), so a
// machine reporting an explicitly EMPTY list can run nothing and drops out of
// the device pool exactly like an offline one.
//
// EXP-481: single-issue starts offer "Resume previous session" when the
// selected machine advertises the `resume` cap AND its synced worktree
// inventory (shape 18) carries a row for the checked issue whose .exp-agents
// marker allows the chosen agent. Default ON; while active the plan-mode
// toggle hides (a resume never re-enters plan mode — the machine clamps it
// too) and `resume: true` rides the single-issue start only. The machine's
// launcher degrades a stale offer to a fresh session seeded with a resume
// prompt, so a worktree row that just went away is never an error.
//
// EXP-437: the sheet remembers NOTHING locally. The run lands on the selected
// MACHINE, so that machine's advertised per-agent defaults (`launchDefaults`
// on the presence row — default agent, model, effort, toggles) are the only
// seed source: they apply on open, on every device switch, and on every agent
// switch. A desktop that advertises none (an older build) falls back to the
// static contract defaults, and every advertised value is validated against
// today's contract lists + the agent's capabilities before it can be shown or
// sent. The old 2+-issue batch override (ultracode on / plan off) is GONE
// (EXP-532): a batch run seeds from the machine's advertised defaults exactly
// like a single-issue one.

struct StartCodingSheet: View {
    /// One eligible issue offered in the picker. `repositoryId` drives the
    /// single-repository-per-run validation (all checked issues must share one).
    struct IssueOption: Identifiable, Sendable {
        let id: String
        let identifier: String?
        let title: String
        let repositoryId: String?
        // Wire status/priority strings, so the picker rows can render the same
        // status/priority glyphs as the issue list (EXP-173). No defaults: a
        // producer that forgets them must fail to compile, not silently render
        // every row as Backlog/no-priority via IssueStatus/IssuePriority.from.
        let status: String?
        let priority: String?

        /// One-shot rebuild of the Issues-tab pool from the synced store —
        /// repo-backed boards, open issues, no merged PR — scoped to [teamId].
        /// Shared so every host that presents the sheet without an agents
        /// surface of its own offers the same pool (EXP-323).
        static func loadCandidates(
            db: DatabaseManager,
            accountId: String,
            teamId: String?
        ) async -> [IssueOption] {
            guard let teamId, let pool = try? db.pool(forAccountId: accountId) else { return [] }
            let boards = (try? await pool.read { db in try BoardEntity.fetchAll(db) }) ?? []
            let issues = (try? await pool.read { db in try IssueEntity.fetchAll(db) }) ?? []
            // Repo-backed boards only — boardId → repositoryId.
            var repoByBoard: [String: String] = [:]
            for board in boards where board.teamId == teamId {
                if let repoId = board.repositoryId {
                    repoByBoard[board.id] = repoId
                }
            }
            // ANCHOR set (EXP-314): custom statuses anchor to one of these
            // enum values, so the check keeps gating them correctly.
            let terminal: Set<String> = [
                IssueStatus.done.rawValue,
                IssueStatus.cancelled.rawValue,
                IssueStatus.duplicate.rawValue,
            ]
            return issues
                .filter { row in
                    guard repoByBoard[row.boardId] != nil else { return false }
                    if terminal.contains(row.status) { return false }
                    if row.prState == DomainContract.prStateMerged { return false }
                    return true
                }
                .sorted { $0.updatedAt > $1.updatedAt }
                .map { row in
                    IssueOption(
                        id: row.id,
                        identifier: row.identifier,
                        title: row.title,
                        repositoryId: repoByBoard[row.boardId],
                        status: row.status,
                        priority: row.priority
                    )
                }
        }
    }

    /// The two launch subjects (EXP-257). Actions only exists when the host
    /// wires `teamId` + `onRunAction`.
    enum SubjectTab: Hashable {
        case issues
        case actions
    }

    let devices: [SteerDevice]
    /// Eligible candidates, pre-checked ids first (the current issue on the
    /// detail card, the whole pool on the Agents tab).
    let issues: [IssueOption]
    let preselectedIds: Set<String>
    let preferredDeviceId: String?
    /// Non-nil (together with `onRunAction`) enables the Actions tab: the team
    /// whose actions/repositories/boards the sheet fetches.
    let teamId: String?
    let preselectedActionId: String?
    /// EXP-431: present the sheet as the "New action" creation flow — no
    /// subject switch, no action picker, just the (preselected) create
    /// builtin's input fields. Callers pair it with the create builtin's id.
    let createActionMode: Bool
    /// Non-nil pre-picks the selected action's `pr` input (EXP-323 — the
    /// conflict-recovery entry points hand over the issue their surface acts
    /// on; ANY issue linked to the PR resolves).
    let preselectedPrIssueId: String?
    /// EXP-481: the synced worktree inventory backing the resume offer. nil =
    /// the sheet loads a one-shot snapshot itself (hosts with a live
    /// observation pass theirs for immediacy).
    let worktrees: [DeviceWorktreeEntity]?
    let onStart: (SteerDevice, [String], SteerStartOptions) -> Void
    /// Actions-mode launch: device, action, options, resolved input values
    /// (key → text or picked repo/board uuid; blank optionals dropped).
    let onRunAction: ((SteerDevice, ActionDto, SteerStartOptions, [String: String]) -> Void)?

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(\.dismiss) private var dismiss
    @Environment(\.motion) private var motion

    /// Sentinel for the blank "CLI default" choice (omit --effort; for
    /// codex/pi also the omit-model default — claude is explicit-always).
    private static let cliDefault = "cli-default"
    /// A batch run is deliberately loose but not unbounded — one Claude session
    /// on one branch; past this the prompt is unwieldy and token-expensive.
    private static let maxBatchIssues = 30
    /// Above this we warn about token cost (still allowed up to the hard cap).
    private static let costWarnThreshold = 6

    @State private var subjectTab: SubjectTab
    @State private var checked: Set<String>
    @State private var searchText = ""
    @State private var deviceId: String?

    // Actions mode (EXP-257). `loadedActions == nil` = still fetching.
    @State private var loadedActions: [ActionDto]?
    @State private var actionsError: String?
    @State private var repos: [TeamRepo] = []
    @State private var boards: [BoardEntity] = []
    /// Open issue-linked pull requests of the team — the options a `pr` input
    /// picks from (EXP-259/EXP-270).
    @State private var pullRequests: [StartPullRequestOption] = []
    @State private var selectedActionId: String?
    @State private var actionSearchText = ""
    /// Input values keyed by the input def's `key` (text, or a picked uuid;
    /// `""` = unset). Reset on action switch.
    @State private var inputValues: [String: String] = [:]

    // Seeded from the selected machine's advertised defaults in onAppear
    // (EXP-437). Placeholder values render for one frame before seed() resolves
    // them.
    @State private var agent = "claude"
    @State private var model = ""
    @State private var effort = Self.cliDefault
    @State private var ultracode = false
    @State private var planMode = false
    @State private var skipPermissions = false
    @State private var seeded = false
    // EXP-481: resume offer state. `resume` defaults ON and only the user
    // flips it (eligibility recomputes live, the choice latches); the loaded
    // snapshot backs hosts that pass no live inventory.
    @State private var resume = true
    @State private var loadedWorktrees: [DeviceWorktreeEntity] = []
    /// The machine the options currently reflect (EXP-437). Picking a DIFFERENT
    /// one reseeds from its defaults; anything that re-resolves to the same
    /// machine (a device re-poll, a reselect) must leave the user's edits alone.
    @State private var lastSeededDeviceId: String?

    init(
        devices: [SteerDevice],
        issues: [IssueOption],
        preselectedIds: Set<String>,
        preferredDeviceId: String? = nil,
        teamId: String? = nil,
        initialTab: SubjectTab = .issues,
        preselectedActionId: String? = nil,
        createActionMode: Bool = false,
        preselectedPrIssueId: String? = nil,
        worktrees: [DeviceWorktreeEntity]? = nil,
        onStart: @escaping (SteerDevice, [String], SteerStartOptions) -> Void,
        onRunAction: ((SteerDevice, ActionDto, SteerStartOptions, [String: String]) -> Void)? = nil
    ) {
        self.worktrees = worktrees
        self.devices = devices
        self.issues = issues
        self.preselectedIds = preselectedIds
        self.preferredDeviceId = preferredDeviceId
        self.teamId = teamId
        self.preselectedActionId = preselectedActionId
        self.createActionMode = createActionMode
        self.preselectedPrIssueId = preselectedPrIssueId
        self.onStart = onStart
        self.onRunAction = onRunAction
        _checked = State(initialValue: preselectedIds)
        _subjectTab = State(initialValue: initialTab)
        _selectedActionId = State(initialValue: preselectedActionId)
    }

    /// Whether the host wired the Actions tab (EXP-257).
    private var actionsEnabled: Bool {
        teamId != nil && onRunAction != nil
    }

    /// The machines a run can actually be sent to: EXP-403 lists offline rows
    /// too, and EXP-409 makes a machine whose every installed agent is signed
    /// out just as unstartable — the My machines list carries the reason.
    /// EXP-432: teammates' shared servers arrive in `devices` already (the
    /// hosts list them team-scoped) and are startable exactly like own ones.
    private var startableDevices: [SteerDevice] {
        devices.filter { $0.isOnline && $0.hasRunnableAgent }
    }

    /// Picker caption for a machine. A teammate's shared server (EXP-432) is
    /// attributed to its owner — two people's boxes can wear the same label,
    /// and a run lands on somebody else's hardware, so the picker says whose.
    private static func deviceCaption(_ device: SteerDevice) -> String {
        let name = device.deviceLabel.isEmpty ? device.deviceId : device.deviceLabel
        guard let owner = device.owner else { return name }
        return "\(name) — \(owner.name)"
    }

    /// Mode-aware device pool: Actions mode only offers capable desktops.
    private var candidateDevices: [SteerDevice] {
        subjectTab == .actions ? actionDeviceCandidates : startableDevices
    }

    private var device: SteerDevice? {
        if let deviceId, let match = candidateDevices.first(where: { $0.deviceId == deviceId }) {
            return match
        }
        if let preferredDeviceId, let match = candidateDevices.first(where: { $0.deviceId == preferredDeviceId }) {
            return match
        }
        return candidateDevices.first
    }

    var body: some View {
        NavigationStack {
            Form {
                // Create mode is single-purpose (EXP-431) — the subject
                // switch would only lead out of the creation flow.
                if actionsEnabled, !createActionMode {
                    Section {
                        Picker("Subject", selection: $subjectTab) {
                            Text("Issues").tag(SubjectTab.issues)
                            Text("Actions").tag(SubjectTab.actions)
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                    }
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
                }

                if subjectTab == .issues {
                    issuesSection
                } else {
                    if createActionMode {
                        createActionIntroSection
                    } else {
                        actionsSection
                    }
                    if let action = selectedAction, !(action.inputs ?? []).isEmpty {
                        inputsSection(action)
                    }
                }

                if candidateDevices.isEmpty {
                    // The "no capable desktop" hint (EXP-257) — distinguishes
                    // offline from an outdated desktop app, and (EXP-409) from
                    // one whose agents are all signed out.
                    Section {
                        Text(noDeviceNote)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                } else if candidateDevices.count > 1 {
                    Section {
                        Picker("Desktop", selection: deviceBinding) {
                            ForEach(candidateDevices) { device in
                                Text(Self.deviceCaption(device))
                                    .tag(device.deviceId)
                            }
                        }
                    }
                }

                // The agent strip rides the Model section's HEADER (EXP-211):
                // a standalone clear Section pads its lone row to the 44pt list
                // minimum, which read as a bigger gap than listSectionSpacing.
                Section {
                    Picker("Model", selection: $model) {
                        ForEach(modelValues, id: \.self) { value in
                            Text(Self.modelLabel(value)).tag(value)
                        }
                    }
                    Picker(effortTitle, selection: $effort) {
                        Text("CLI default").tag(Self.cliDefault)
                        ForEach(effortValues, id: \.self) { value in
                            Text(Self.effortLabel(value)).tag(value)
                        }
                    }
                    .disabled(ultracode)
                } header: {
                    if availableAgents.count > 1 {
                        agentTabStrip
                            .textCase(nil)
                    }
                }

                // One footer-less toggle section (EXP-208 — no helper notices,
                // like the IDE). Ultracode is claude-only, plan mode is
                // claude+pi (EXP-441), skip permissions doesn't exist for pi.
                Section {
                    if resumeCandidate != nil {
                        Toggle("Resume previous session", isOn: $resume)
                    }
                    if agent == "claude" {
                        Toggle("Ultracode", isOn: $ultracode)
                    }
                    // A resume never re-enters plan mode (the machine clamps
                    // it too) — hide the toggle while one is active.
                    if Self.supportsPlanMode(agent), !resumeActive {
                        Toggle("Plan mode", isOn: $planMode)
                    }
                    if agent != "pi" {
                        Toggle("Skip permissions", isOn: $skipPermissions)
                    }
                } footer: {
                    if resumeActive, let candidate = resumeCandidate {
                        Text("A worktree for \(candidate.issueIdentifier ?? "this issue") already exists (\(candidate.branch)).")
                    }
                }
            }
            // No navigation title (EXP-211) — the confirm button already says
            // "Start coding"; the bar carries only Cancel + Start. Create mode
            // (EXP-431) is the one host that needs to say what it is.
            .navigationTitle(createActionMode ? "New action" : "")
            .navigationBarTitleDisplayMode(.inline)
            .listSectionSpacing(12)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(confirmTitle) {
                        if subjectTab == .actions {
                            submitAction()
                        } else {
                            submit()
                        }
                    }
                    .disabled(subjectTab == .actions ? !canRunAction : !canStart)
                }
            }
        }
        .presentationDetents([.large])
        // `children: .contain` keeps the form's controls queryable; a bare
        // identifier on a plain container never reaches the UI-test hierarchy.
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("start-coding-sheet")
        .onAppear { seed() }
        .task { await loadActionsData() }
        .task {
            // EXP-481: hosts without a live worktrees observation get a
            // one-shot snapshot — enough for an offer that degrades
            // gracefully on staleness anyway.
            guard worktrees == nil else { return }
            loadedWorktrees = await DeviceQueries.worktrees(db: deps.db, accountId: accountId)
        }
        // The candidate device pool changes with the tab (Actions filters to
        // capable desktops) and with the selection (inputs-carrying/builtin
        // actions additionally need `action-inputs`) — the resolved device may
        // stop offering the chosen agent, or be a different machine entirely
        // (which brings its own defaults, EXP-437).
        .onChange(of: subjectTab) { _, _ in reconcileResolvedDevice() }
        .onChange(of: selectedActionId) { _, _ in reconcileResolvedDevice() }
    }

    /// After anything that can implicitly re-resolve `device` (tab or action
    /// switch tightening the candidate pool): clamp the agent, and when the
    /// pool settled on a DIFFERENT machine, reseed from its defaults exactly
    /// like an explicit pick would (EXP-437).
    private func reconcileResolvedDevice() {
        clampAgentToDevice()
        if device?.deviceId != lastSeededDeviceId { applyDeviceDefaults() }
    }

    // MARK: - Issue picker

    private var issuesSection: some View {
        Section {
            searchField
            if pinnedRows.isEmpty, otherRows.isEmpty {
                Text(issues.isEmpty ? "No eligible issues to code." : "No matching issues.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if pinnedRows.count + otherRows.count <= 6 {
                ForEach(pinnedRows) { issueRow($0) }
                ForEach(otherRows) { issueRow($0) }
            } else {
                // Many candidates: scroll them inside a bounded box (one
                // section row) so the Model / Effort / toggle sections
                // stay near the top instead of being pushed off-screen.
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(pinnedRows) { issueRow($0) }
                        ForEach(otherRows) { issueRow($0) }
                    }
                }
                .frame(maxHeight: 280)
            }
        } header: {
            Text("Issues")
        } footer: {
            // Only attach a footer when there's a message — an empty
            // footer view still reserves space, inflating the gap to
            // the next card past listSectionSpacing (EXP-211).
            if multiRepo || overCap || effectiveChecked.count > Self.costWarnThreshold {
                issuesFooter
            }
        }
    }

    private var searchField: some View {
        // Inline search field. NOT system .searchable — same rationale as
        // DuplicatePickerSheet (iOS 26 renders it as a bottom-edge glass bar).
        HStack(spacing: 8) {
            AppIcon(AppIcons.navSearch, size: AppIcon.Size.small)
                .foregroundStyle(.secondary)
            TextField("Search issues", text: $searchText)
                .textFieldStyle(.plain)
                .submitLabel(.search)
            if !searchText.isEmpty {
                Button {
                    searchText = ""
                } label: {
                    AppIcon(AppIcons.uiClear, size: AppIcon.Size.small)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func issueRow(_ option: IssueOption) -> some View {
        let isChecked = checked.contains(option.id)
        return Button {
            toggle(option.id)
        } label: {
            HStack(spacing: 10) {
                // Selection state must be unmissable (EXP-241): body-size
                // glyph swap plus a tinted row background below — the old
                // caption-size circle alone read as decoration, not a control.
                AppIcon(isChecked ? AppIcons.uiSelected : AppIcons.uiUnselected, size: AppIcon.Size.medium)
                    .foregroundStyle(isChecked ? Accent.indigo : .secondary)

                // Issue-list row anatomy (EXP-173): priority icon, mono
                // identifier, status icon, title.
                AppIcon(IssuePriority.from(option.priority).iconName, size: AppIcon.Size.small)
                    .foregroundStyle(IssuePriority.from(option.priority).color)
                    .frame(width: 16)

                Text(option.identifier ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .frame(minWidth: 60, alignment: .leading)

                AppIcon(IssueStatus.from(option.status).iconName, size: AppIcon.Size.small)
                    .foregroundStyle(IssueStatus.from(option.status).color)
                    .frame(width: 16)

                Text(option.title)
                    .font(.subheadline)
                    .lineLimit(1)

                Spacer(minLength: 0)
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 6)
            .background(
                isChecked ? Accent.indigo.opacity(0.12) : Color.clear,
                in: RoundedRectangle(cornerRadius: 8)
            )
            // Cancel the highlight inset so text stays aligned with the
            // search field and the neighboring form rows.
            .padding(.horizontal, -6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var issuesFooter: some View {
        VStack(alignment: .leading, spacing: 4) {
            if multiRepo {
                Text("Pick issues from a single repository per run.")
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
            if overCap {
                Text("A batch run is capped at \(Self.maxBatchIssues) issues.")
                    .foregroundStyle(DesignTokens.Semantic.red)
            } else if effectiveChecked.count > Self.costWarnThreshold {
                Text("Large batches are token-expensive.")
            }
        }
    }

    private func matchesSearch(_ option: IssueOption) -> Bool {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        return option.title.localizedCaseInsensitiveContains(trimmed)
            || (option.identifier ?? "").localizedCaseInsensitiveContains(trimmed)
    }

    /// Every id currently in the candidate pool.
    private var poolIds: Set<String> {
        Set(issues.map(\.id))
    }

    /// Checked ids that are actually in the pool. A preselected id whose issue
    /// isn't eligible (e.g. a repo-less current issue that startCodingCandidates
    /// deliberately left out) is a stray — it must never be shown, counted,
    /// validated, or submitted, so every derived value works off THIS masked set,
    /// not `checked`. This also self-heals if the pool arrives after seeding.
    private var effectiveChecked: Set<String> {
        checked.intersection(poolIds)
    }

    /// Preselected rows, pinned to the top. The pin set is the OPEN-time
    /// `preselectedIds` snapshot, deliberately NOT the live `checked` set:
    /// re-sorting on every toggle teleported the tapped row out from under
    /// the finger (often out of the bounded scroll box), which read as
    /// "issues are not selectable" (EXP-241). Rows now stay put; only the
    /// check indicator changes.
    private var pinnedRows: [IssueOption] {
        issues.filter { preselectedIds.contains($0.id) && matchesSearch($0) }
    }

    private var otherRows: [IssueOption] {
        Array(issues.filter { !preselectedIds.contains($0.id) && matchesSearch($0) }.prefix(50))
    }

    /// Checked ids in the candidate pool's order (pre-checked / recency) — the
    /// launch payload. Filtering `issues` inherently drops strays.
    private var orderedCheckedIds: [String] {
        issues.filter { effectiveChecked.contains($0.id) }.map(\.id)
    }

    private var checkedRepoIds: Set<String> {
        Set(issues.filter { effectiveChecked.contains($0.id) }.compactMap(\.repositoryId))
    }

    private var multiRepo: Bool { checkedRepoIds.count > 1 }
    private var overCap: Bool { effectiveChecked.count > Self.maxBatchIssues }

    private var canStart: Bool {
        device != nil && !effectiveChecked.isEmpty && !multiRepo && !overCap
    }

    private var startTitle: String {
        effectiveChecked.count > 1 ? "Start coding (\(effectiveChecked.count) issues)" : "Start coding"
    }

    private func toggle(_ id: String) {
        withAnimation(motion.standard) {
            // Both results discarded explicitly: a lone `if` is if-expression
            // eligible, and its branches here have mismatched non-Void types
            // (String? vs the insert tuple), which is exactly the shape that
            // makes WMO mis-infer withAnimation's generic Result in Release
            // builds only (EXP-240).
            if checked.contains(id) {
                _ = checked.remove(id)
            } else {
                _ = checked.insert(id)
            }
        }
    }

    // MARK: - Actions mode (EXP-257)

    /// The confirm button's label: the create builtin's run IS creation
    /// (desktop footer parity, EXP-431); other actions just run.
    private var confirmTitle: String {
        guard subjectTab == .actions else { return startTitle }
        return selectedAction?.id == DomainContract.builtinCreateActionId ? "Create" : "Run action"
    }

    /// EXP-431 create mode: no picker — the create builtin's input fields
    /// below ARE the form (Description leads with the locked placeholder).
    private var createActionIntroSection: some View {
        Section {
            Text("Describe it and your agent will author it for the team.")
                .font(.caption)
                .foregroundStyle(.secondary)
            if loadedActions == nil, actionsError == nil {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Loading actions…")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } else if let actionsError {
                Text(actionsError)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            }
        }
    }

    private var actionsSection: some View {
        Section {
            actionSearchField
            if loadedActions == nil, actionsError == nil {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Loading actions…")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } else if let actionsError {
                Text(actionsError)
                    .font(.caption)
                    .foregroundStyle(DesignTokens.Semantic.red)
            } else if actionRows.isEmpty {
                Text((loadedActions ?? []).isEmpty ? "No actions yet." : "No matching actions.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else if actionRows.count <= 6 {
                ForEach(actionRows) { actionRow($0) }
            } else {
                // Bounded like the issue picker: keep the input fields and the
                // Model / Effort sections reachable.
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(actionRows) { actionRow($0) }
                    }
                }
                .frame(maxHeight: 280)
            }
        } header: {
            Text("Actions")
        }
    }

    private var actionSearchField: some View {
        HStack(spacing: 8) {
            AppIcon(AppIcons.navSearch, size: AppIcon.Size.small)
                .foregroundStyle(.secondary)
            TextField("Search actions", text: $actionSearchText)
                .textFieldStyle(.plain)
                .submitLabel(.search)
            if !actionSearchText.isEmpty {
                Button {
                    actionSearchText = ""
                } label: {
                    AppIcon(AppIcons.uiClear, size: AppIcon.Size.small)
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func actionRow(_ action: ActionDto) -> some View {
        let isSelected = action.id == selectedActionId
        return Button {
            selectAction(action)
        } label: {
            HStack(spacing: 10) {
                AppIcon(isSelected ? AppIcons.uiSelected : AppIcons.uiUnselected, size: AppIcon.Size.medium)
                    .foregroundStyle(isSelected ? Accent.indigo : .secondary)

                // The builtin "Create action" wears the create affordance;
                // real actions keep the bolt (the Actions surface glyph).
                AppIcon(action.isBuiltin ? AppIcons.actionCreate : AppIcons.actionDefault, size: AppIcon.Size.small)
                    .foregroundStyle(.secondary)
                    .frame(width: 16)

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 6) {
                        Text(action.name)
                            .font(.subheadline)
                            .lineLimit(1)
                        if action.repositoryId != nil {
                            // Small repo indicator: this action clones its repo.
                            AppIcon(AppIcons.actionRepository, size: 11)
                                .foregroundStyle(.secondary)
                                .accessibilityLabel("Runs in a repository")
                        }
                    }
                    if let description = action.description, !description.isEmpty {
                        Text(description)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 0)
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 6)
            .background(
                isSelected ? Accent.indigo.opacity(0.12) : Color.clear,
                in: RoundedRectangle(cornerRadius: 8)
            )
            .padding(.horizontal, -6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func inputsSection(_ action: ActionDto) -> some View {
        Section {
            ForEach(action.inputs ?? [], id: \.key) { def in
                inputField(def)
            }
        } header: {
            Text("Inputs")
        }
    }

    @ViewBuilder
    private func inputField(_ def: ActionInputDto) -> some View {
        switch def.type {
        case "text":
            VStack(alignment: .leading, spacing: 4) {
                Text(inputLabel(def))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextField(def.placeholder ?? "", text: inputBinding(def.key), axis: .vertical)
                    .lineLimit(1...4)
            }
        case "repo":
            Picker(inputLabel(def), selection: inputBinding(def.key)) {
                Text(def.isRequired ? "Select a repository" : "None").tag("")
                ForEach(repos) { repo in
                    Text(repo.fullName).tag(repo.id)
                }
            }
        case "board":
            Picker(inputLabel(def), selection: inputBinding(def.key)) {
                Text(def.isRequired ? "Select a board" : "None").tag("")
                ForEach(boards) { board in
                    Text(board.name).tag(board.id)
                }
            }
        case "icon":
            // EXP-273: the value is a curated registry NAME (e.g. `rocket`) —
            // the same string a board stores — picked from the same swatch grid
            // as the create-board form. Optional inputs start at none and can
            // be cleared again (the desktop's popover behaves identically).
            VStack(alignment: .leading, spacing: 6) {
                Text(inputLabel(def))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                IconSwatchGrid(selection: inputBinding(def.key), allowsNone: !def.isRequired)
            }
        case "pr":
            // EXP-259: the value is the REPRESENTATIVE issue id of an open
            // issue-linked PR (batch PRs dedupe by prUrl, so one row can list
            // several identifiers).
            if pullRequests.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text(inputLabel(def))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text("No open pull requests.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } else {
                Picker(inputLabel(def), selection: inputBinding(def.key)) {
                    Text(def.isRequired ? "Select a pull request" : "None").tag("")
                    ForEach(pullRequests) { pull in
                        Text(pull.label).tag(pull.issueId)
                    }
                }
            }
        default:
            // Unknown future input type — block the run instead of silently
            // degrading to text (the desktop mirrors this posture).
            Text("\"\(def.label)\" needs a newer app version.")
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
        }
    }

    private func inputLabel(_ def: ActionInputDto) -> String {
        def.isRequired ? def.label : "\(def.label) (optional)"
    }

    private func inputBinding(_ key: String) -> Binding<String> {
        Binding(
            get: { inputValues[key] ?? "" },
            set: { inputValues[key] = $0 }
        )
    }

    private func selectAction(_ action: ActionDto) {
        guard action.id != selectedActionId else { return }
        withAnimation(motion.standard) {
            selectedActionId = action.id
        }
        // Values are keyed per-def — a different action's defs must start clean.
        inputValues = [:]
        seedRepoInputs(for: action)
    }

    /// Pre-fill `repo` inputs with the action's bound repository (EXP-349) —
    /// a picker reading "None" while the run targets the bound repo anyway
    /// looked misconfigured. The already-set guard mirrors `seedPreselectedPr`:
    /// a manual clear stores "", which is non-nil, so it's never re-stomped.
    private func seedRepoInputs(for action: ActionDto) {
        guard let repositoryId = action.repositoryId else { return }
        for def in action.inputs ?? [] where def.type == "repo" && inputValues[def.key] == nil {
            inputValues[def.key] = repositoryId
        }
    }

    private func matchesActionSearch(_ action: ActionDto) -> Bool {
        let trimmed = actionSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return true }
        return action.name.localizedCaseInsensitiveContains(trimmed)
            || (action.description ?? "").localizedCaseInsensitiveContains(trimmed)
    }

    /// Search-filtered rows, builtins pinned FIRST by the `builtin` flag (the
    /// contract — never by sort order). The create builtin stays in the POOL
    /// (`createActionMode` preselects it) but never renders as a picker row
    /// (EXP-431).
    private var actionRows: [ActionDto] {
        let filtered = (loadedActions ?? [])
            .filter { $0.id != DomainContract.builtinCreateActionId }
            .filter { matchesActionSearch($0) }
        return filtered.filter(\.isBuiltin) + filtered.filter { !$0.isBuiltin }
    }

    private var selectedAction: ActionDto? {
        guard let selectedActionId else { return nil }
        return loadedActions?.first { $0.id == selectedActionId }
    }

    private var selectedActionInputs: [ActionInputDto] {
        selectedAction?.inputs ?? []
    }

    /// Builtin or inputs-carrying runs additionally need the `action-inputs`
    /// capability (EXP-257) — older desktops can't render/inject them.
    private var selectedActionNeedsInputsCap: Bool {
        guard let action = selectedAction else { return false }
        return action.isBuiltin || !(action.inputs ?? []).isEmpty
    }

    /// The "Fix merge conflicts" builtin needs its own capability on top
    /// (EXP-259) — the server rejects it otherwise, so filter here (EXP-323).
    private var selectedActionNeedsFixConflictsCap: Bool {
        selectedAction?.id == DomainContract.builtinFixConflictsId
    }

    private var actionDeviceCandidates: [SteerDevice] {
        startableDevices.filter { device in
            guard device.canRunActions else { return false }
            if selectedActionNeedsInputsCap, !device.canRunActionInputs { return false }
            if selectedActionNeedsFixConflictsCap, !device.canFixConflicts { return false }
            return true
        }
    }

    /// The "nothing to start on" hint (EXP-257) — distinguishes offline from
    /// an outdated desktop app, calls out the fix-conflicts cap by name so the
    /// fix is obvious (EXP-323), and names the signed-out agents when a
    /// machine is online but can run nothing (EXP-409).
    private var noDeviceNote: String {
        if startableDevices.isEmpty {
            let signedOut = signedOutAgentNames.joined(separator: ", ")
            if !signedOut.isEmpty {
                return "\(signedOut) not signed in on your machines. Sign in on the machine first."
            }
            return subjectTab == .actions
                ? "No desktop online. Open the Exponential desktop app to run here."
                : "No desktop online. Open the Exponential desktop app to start coding."
        }
        if selectedActionNeedsFixConflictsCap {
            return "No desktop can fix merge conflicts yet. Update the Exponential desktop app."
        }
        return "No compatible desktop online. Update the Exponential desktop app to run this action."
    }

    /// The agents installed-but-signed-out across the caller's ONLINE machines
    /// (EXP-409), in contract order.
    private var signedOutAgentNames: [String] {
        let reported = Set(devices.filter(\.isOnline).flatMap(\.unauthedAgentIds))
        return DomainContract.codingAgentValues.filter { reported.contains($0) }
    }

    // The three run gates and the wire mapping live in ExpCore
    // (`ActionInputValues`) so they are unit-testable and stay in step with the
    // web helper.
    private var hasUnknownInputType: Bool {
        ActionInputValues.hasUnsupportedType(selectedActionInputs)
    }

    private var requiredInputsFilled: Bool {
        ActionInputValues.requiredFilled(selectedActionInputs, values: inputValues)
    }

    private var textInputsWithinLimit: Bool {
        ActionInputValues.textsWithinLimit(selectedActionInputs, values: inputValues)
    }

    private var canRunAction: Bool {
        device != nil
            && selectedAction != nil
            && !hasUnknownInputType
            && requiredInputsFilled
            && textInputsWithinLimit
    }

    /// One-shot fetch of the Actions-tab data: the team's actions + boards
    /// from the synced GRDB store (EXP-268 — actions are the 15th shape, so
    /// no tRPC round trip; `body` isn't synced and nothing here needs it),
    /// the repo registry over tRPC. The builtin "Create action" is PREPENDED
    /// locally — synced rows can't carry the virtual entry.
    @MainActor
    private func loadActionsData() async {
        guard actionsEnabled, let teamId else { return }
        if let pool = try? deps.db.pool(forAccountId: accountId) {
            let rows = (try? await pool.read { db in
                try ActionEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }) ?? []
            let dtos = rows
                .sorted { ($0.sortOrder ?? 0, $0.name) < ($1.sortOrder ?? 0, $1.name) }
                .map { ActionDto(entity: $0) }
            loadedActions = ActionDto.builtinActions(teamId: teamId) + dtos
            actionsError = nil
        } else {
            actionsError = "The local database is unavailable."
        }
        repos = (try? await deps.repositoriesApi.list(accountId: accountId, teamId: teamId)) ?? []
        if let pool = try? deps.db.pool(forAccountId: accountId) {
            let rows = (try? await pool.read { db in
                try BoardEntity.filter(Column("team_id") == teamId).fetchAll(db)
            }) ?? []
            boards = rows.sorted { lhs, rhs in
                (lhs.sortOrder ?? 0, lhs.name) < (rhs.sortOrder ?? 0, rhs.name)
            }
            // Issues don't sync team_id, so the team scope comes from the
            // synced boards — same derivation the web picker uses.
            let boardIds = Set(boards.map(\.id))
            let openPrIssues = (try? await pool.read { db in
                try IssueEntity.filter(Column("pr_state") == DomainContract.prStateOpen)
                    .fetchAll(db)
            }) ?? []
            pullRequests = StartPullRequestOption.build(
                from: openPrIssues,
                teamBoardIds: boardIds
            )
        }
        seedPreselectedPr()
        // A preselected action never goes through `selectAction` (the id is
        // seeded via State(initialValue:)) — seed its repo inputs here, once
        // the action rows exist (EXP-349).
        if let action = selectedAction {
            seedRepoInputs(for: action)
        }
    }

    /// Pre-pick the target PR once both the action list and the options exist
    /// (EXP-323). The seed is normalised by MEMBERSHIP — the caller's issue is
    /// rarely the option's representative, and only the representative id
    /// matches a `Picker` tag. The already-set guard keeps a manual re-pick
    /// from being stomped if this ever runs twice.
    @MainActor
    private func seedPreselectedPr() {
        guard let seed = preselectedPrIssueId,
            let key = selectedActionInputs.first(where: { $0.type == "pr" })?.key,
            inputValues[key] == nil,
            let option = StartPullRequestOption.option(in: pullRequests, forIssueId: seed)
        else { return }
        inputValues[key] = option.issueId
    }

    // MARK: - Agent tab strip (EXP-208)

    /// Horizontal centered pill tab strip with brand icons — the iOS twin of
    /// the desktop dialog's `agent_tabs`. Selection goes through `selectAgent`
    /// so a switch reseeds the options from the machine's per-agent defaults.
    private var agentTabStrip: some View {
        HStack(spacing: 8) {
            ForEach(availableAgents, id: \.self) { value in
                agentTab(value)
            }
        }
        .frame(maxWidth: .infinity)
    }

    private func agentTab(_ value: String) -> some View {
        let selected = value == agent
        return Button {
            // Same no-op-on-reselect semantics as the old segmented Picker
            // (`selectAgent` guards it too — a reselect keeps the edits).
            guard value != agent else { return }
            selectAgent(value)
        } label: {
            HStack(spacing: 6) {
                Image("agent-\(value)")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
                Text(Self.agentLabel(value))
                    .font(.subheadline.weight(.medium))
            }
            .foregroundStyle(selected ? .white : .white.opacity(TextOpacity.secondary))
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .background(selected ? Accent.indigoStrong : Color.white.opacity(0.06), in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
    }

    // MARK: - Bindings

    private var deviceBinding: Binding<String> {
        Binding(
            get: { device?.deviceId ?? "" },
            set: {
                let switched = $0 != lastSeededDeviceId
                deviceId = $0
                // The newly selected desktop may not run the chosen agent.
                clampAgentToDevice()
                // A DIFFERENT machine brings its own coding defaults (EXP-437);
                // reselecting the current one keeps the user's edits.
                if switched { applyDeviceDefaults() }
            }
        )
    }

    // MARK: - Agent-dependent option lists (EXP-201)

    /// The selected device's RUNNABLE agents, in contract order. An ABSENT
    /// advertisement is an old claude-only desktop, but an explicitly empty
    /// one means the machine can run nothing (EXP-409) — such devices never
    /// reach `candidateDevices`, so the picker keeps its claude fallback only
    /// for the no-device case (it renders nothing when there is one entry).
    private var availableAgents: [String] {
        let supported = device?.agentIds ?? ["claude"]
        let ordered = DomainContract.codingAgentValues.filter { supported.contains($0) }
        return ordered.isEmpty ? ["claude"] : ordered
    }

    private var modelValues: [String] { Self.modelValues(for: agent) }

    private var effortValues: [String] { Self.effortValues(for: agent) }

    /// Claude's model is explicit-always; codex/pi offer a "CLI default" blank.
    /// Parameterized because the seed validates an advertised value against the
    /// agent it belongs to, which isn't always the selected one yet (EXP-437).
    private static func modelValues(for agent: String) -> [String] {
        switch agent {
        case "codex": [cliDefault] + DomainContract.codexModelValues
        case "pi": [cliDefault] + DomainContract.piModelValues
        default: DomainContract.codingModelValues
        }
    }

    private static func effortValues(for agent: String) -> [String] {
        switch agent {
        case "codex": DomainContract.codexEffortValues
        case "pi": DomainContract.piThinkingValues
        default: DomainContract.codingEffortValues
        }
    }

    private var effortTitle: String {
        switch agent {
        case "codex": "Reasoning"
        case "pi": "Thinking"
        default: "Effort"
        }
    }

    private static func defaultModel(for agent: String) -> String {
        agent == "claude" ? (DomainContract.codingModelValues.first ?? "") : cliDefault
    }

    /// Switch agent: model/effort/toggles reseed from the machine's defaults
    /// for the NEW agent (EXP-437 — they are per-agent), falling back to the
    /// agent's static defaults, then clamp to what it supports.
    private func selectAgent(_ value: String) {
        guard value != agent else { return }
        agent = value
        applyAgentDefaults(for: value)
    }

    /// Plan mode is claude (native) + pi (via the launcher-injected
    /// extension, EXP-441); codex has no launch-into-plan mode.
    static func supportsPlanMode(_ agent: String) -> Bool {
        agent == "claude" || agent == "pi"
    }

    private func clampToggles() {
        if agent != "claude" {
            ultracode = false
        }
        if !Self.supportsPlanMode(agent) {
            planMode = false
        }
        if agent == "pi" {
            skipPermissions = false
        }
    }

    private func clampAgentToDevice() {
        if !availableAgents.contains(agent) {
            selectAgent(availableAgents.first ?? "claude")
        }
    }

    // MARK: - Seed / submit

    private func seed() {
        guard !seeded else { return }
        seeded = true
        applyDeviceDefaults()
    }

    /// Reseed agent + every option from the resolved machine's advertised
    /// defaults (EXP-437). The machine is the only seed source — nothing is
    /// remembered locally — so this runs on open and on every device switch.
    private func applyDeviceDefaults() {
        if let advertised = device?.defaultLaunchAgent {
            agent = advertised
        } else if !availableAgents.contains(agent) {
            // Nothing advertised: keep the current agent when the machine can
            // run it, else fall back exactly like `clampAgentToDevice`.
            agent = availableAgents.first ?? "claude"
        }
        applyAgentDefaults(for: agent)
        lastSeededDeviceId = device?.deviceId
    }

    /// The per-agent half: reset to [value]'s static defaults, then overlay what
    /// the machine advertises FOR THAT AGENT. An advertised value outside
    /// today's contract lists is dropped rather than shown or sent (a desktop of
    /// another vintage must never push a value the server rejects), and the
    /// toggles clamp to the agent's capabilities.
    private func applyAgentDefaults(for value: String) {
        let advertised = device?.agentDefaults(for: value)
        model = Self.seedModel(advertised?.model, for: value)
        effort = Self.seedEffort(advertised?.effort, for: value)
        ultracode = advertised?.ultracode ?? false
        planMode = advertised?.planMode ?? false
        skipPermissions = advertised?.skipPermissions ?? false
        clampToggles()
    }

    /// An advertised model, validated against the agent's contract list. Blank
    /// is the desktop's "CLI default", which for codex/pi IS the static default
    /// and for claude (explicit-always) means falling back to its first model.
    private static func seedModel(_ value: String?, for agent: String) -> String {
        guard let value, !value.isEmpty, modelValues(for: agent).contains(value) else {
            return defaultModel(for: agent)
        }
        return value
    }

    /// An advertised effort/reasoning/thinking value; blank or unknown = the
    /// "CLI default" row (omit the flag).
    private static func seedEffort(_ value: String?, for agent: String) -> String {
        guard let value, effortValues(for: agent).contains(value) else { return cliDefault }
        return value
    }

    // MARK: - Resume (EXP-481)

    /// The worktree inventory backing the resume offer — the host's live rows
    /// when passed, else the sheet's own one-shot snapshot.
    private var worktreePool: [DeviceWorktreeEntity] {
        worktrees ?? loadedWorktrees
    }

    /// The synced worktree that makes "Resume previous session" offerable:
    /// Issues tab, exactly ONE checked issue, a resume-capable machine picked
    /// off the devices SHAPE (rowId — tRPC/relay rows never resume), and a
    /// row whose identifier + .exp-agents marker match. Recomputed live;
    /// nil hides the toggle.
    private var resumeCandidate: DeviceWorktreeEntity? {
        guard subjectTab == .issues,
              effectiveChecked.count == 1,
              let device, device.canResume,
              let issueId = effectiveChecked.first,
              let identifier = issues.first(where: { $0.id == issueId })?.identifier
        else { return nil }
        return WorktreeResume.match(
            worktrees: worktreePool,
            deviceRowId: device.rowId,
            issueIdentifier: identifier,
            agent: agent
        )
    }

    private var resumeActive: Bool {
        resume && resumeCandidate != nil
    }

    /// The chosen options in wire form — shared by both launch subjects.
    /// `resume` is set by `submit()` alone: single-issue starts only.
    private func buildOptions(resume: Bool? = nil) -> SteerStartOptions {
        let isClaude = agent == "claude"
        return SteerStartOptions(
            agent: agent,
            model: model == Self.cliDefault ? "" : model,
            effort: effort == Self.cliDefault ? "" : effort,
            // The toggles only exist for the agents that support them — never
            // send a stale value the launcher would reject or misread.
            ultracode: isClaude ? ultracode : nil,
            // A resume never re-enters plan mode (mirrors the desktop clamp).
            planMode: Self.supportsPlanMode(agent) ? (resume == true ? false : planMode) : nil,
            skipPermissions: agent == "pi" ? nil : skipPermissions,
            resume: resume
        )
    }

    private func submit() {
        guard let device, !orderedCheckedIds.isEmpty else { return }
        // Snapshot before dismissing — the payload must not depend on what the
        // teardown does to the sheet's state.
        let ids = orderedCheckedIds
        // Single-issue only — a batch has no per-issue worktree to resume.
        let options = buildOptions(
            resume: ids.count == 1 && resumeActive ? true : nil
        )
        dismiss()
        onStart(device, ids, options)
    }

    private func submitAction() {
        guard let device, let action = selectedAction, let onRunAction, canRunAction else { return }
        let options = buildOptions()
        // Values in wire form: text trimmed, blank optionals dropped (a
        // required blank can't get here — `canRunAction` gates it).
        let values = ActionInputValues.wireValues(selectedActionInputs, values: inputValues)
        dismiss()
        onRunAction(device, action, options, values)
    }

    private static func agentLabel(_ value: String) -> String {
        switch value {
        case "claude": "Claude Code"
        case "codex": "Codex"
        case "pi": "pi"
        default: value
        }
    }

    private static func modelLabel(_ value: String) -> String {
        switch value {
        case cliDefault: "CLI default"
        case "gpt-5.6-sol": "GPT-5.6 Sol"
        case "gpt-5.6-terra": "GPT-5.6 Terra"
        case "gpt-5.6-luna": "GPT-5.6 Luna"
        case "grok-4.5": "Grok 4.5"
        default: value.prefix(1).uppercased() + value.dropFirst()
        }
    }

    private static func effortLabel(_ value: String) -> String {
        value == "xhigh" ? "XHigh" : value.prefix(1).uppercased() + value.dropFirst()
    }
}
