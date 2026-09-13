import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs in the
/// active team ("Running", nested by `SessionTree`, EXP-818) above the
/// finished ones ("Past", EXP-746). Moved verbatim from the Devices tab,
/// which keeps machines only (web parity, EXP-818).
///
/// Session rows open the live agent session view directly when the relay
/// is configured, else fall back to the issue detail; the trailing circle
/// (EXP-694/874) names the run's subject — Open issue on an issue run, the
/// action's glyph to its editor, nothing on chat/batch runs. A refused merge
/// captions its row and, on a REAL conflict, swaps Merge for the builtin
/// recovery run, which the page seeds into the composer above (no sheet: the
/// composer IS the launcher now).
struct AgentSessionsList: View {
    let vm: AgentsViewModel
    let steerEnabled: Bool
    /// Owner-only writes (the actions/automations routers are owner-gated) —
    /// resolved ONCE per active team by the page, never per row.
    let canEditActions: Bool
    /// "Fix conflicts" on a row: the representative issue of the failing PR.
    let onFixConflicts: (String) -> Void

    @Environment(AppDependencies.self) private var deps
    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState

    // Merge (EXP-498: merging always closes the session), keyed by row id:
    // the confirm target, the in-flight rows, and the per-row failure caption
    // (inline like the Reviews rows, EXP-323 — never a modal).
    @State private var mergeConfirm: MergeConfirmTarget?
    @State private var merging: Set<String> = []
    @State private var mergeErrors: [String: MergeFailure] = [:]
    /// EXP-694 (S6): the action/automation editor a session row's trailing
    /// button opened.
    @State private var sessionEditTarget: SessionEditTarget?
    @State private var editError: String?
    /// The past rows' tap target — the page pushes it.
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// EXP-862: "Past" is FOLDED by default on every client — finished runs are
    /// history, and an unfolded list of them buried the live ones. The header
    /// carries the count and expands inline.
    @State private var pastExpanded = false

    /// The row a merge confirm is pending for. Only the ids are captured —
    /// the row itself may re-sync underneath the alert. EXP-734: the target
    /// says WHICH mutation merges it (the issue's PR, or the run's own
    /// issue-less one) and picks the alert's copy.
    private struct MergeConfirmTarget: Identifiable {
        let rowId: String
        let target: MergeTarget
        var id: String { rowId }
    }

    /// EXP-694 (S6): what a session row's trailing button opens.
    private enum SessionEditTarget: Identifiable {
        case action(ActionDto)
        case automation(AutomationDto)

        var id: String {
            switch self {
            case let .action(action): "action-\(action.id)"
            case let .automation(automation): "automation-\(automation.id)"
            }
        }

        var accessibilityLabel: String {
            switch self {
            case .action: "Edit action"
            case .automation: "Edit automation"
            }
        }
    }

    var body: some View {
        // EXP-818: the Running/Past groups are filled BANDS over flat rows
        // (`GlassSectionBand` + `.flatRow()`) — the runs read as a table, the
        // way web's and the IDE's session lists do.
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Running")
                if vm.rows.isEmpty {
                    noAgentsRow
                } else {
                    // EXP-818: a run started by another run nests under its
                    // parent, indented (`SessionTree`, the ×4 rule).
                    ForEach(
                        SessionTree.nest(
                            vm.rows,
                            id: { $0.session.id },
                            parent: { $0.session.parentSessionId },
                            startedAt: { $0.session.startedAt }
                        ),
                        id: \.session.id
                    ) { entry in
                        sessionRow(entry.session)
                            .padding(.leading, CGFloat(entry.depth) * 16)
                    }
                }
            }

            if let editError {
                Text(editError)
                    .font(.caption2)
                    .foregroundStyle(DesignTokens.Semantic.red)
                    .padding(.horizontal, 4)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            // EXP-746: the caller's finished runs. Absent entirely when
            // there are none — an empty history is not news.
            if !vm.pastRows.isEmpty {
                pastSection
            }
        }
        // Its own zero-size node for the editor sheet — the alert below hangs
        // off the list, and one presentation per node is the rule.
        .background(
            Color.clear
                .frame(width: 0, height: 0)
                .allowsHitTesting(false)
                .sheet(item: $sessionEditTarget) { target in
                    switch target {
                    case let .action(action):
                        EditActionSheet(action: action, canEdit: canEditActions)
                            .environment(\.accountId, accountId)
                    case let .automation(automation):
                        AutomationFormSheet(
                            teamId: automation.teamId,
                            // The VM observes actions ACCOUNT-wide (a session
                            // names its own team), so the picker scopes itself.
                            actions: vm.actions.filter { $0.teamId == automation.teamId },
                            devices: (vm.devices ?? []).filter(\.canRunAutomations),
                            editing: automation,
                            onSubmit: { actionId, deviceId, trigger, launch in
                                saveAutomation(
                                    automation,
                                    actionId: actionId,
                                    deviceId: deviceId,
                                    trigger: trigger,
                                    launch: launch
                                )
                            }
                        )
                        .environment(\.accountId, accountId)
                    }
                }
        )
        .alert(
            "Merge pull request?",
            isPresented: Binding(
                get: { mergeConfirm != nil },
                set: { if !$0 { mergeConfirm = nil } }
            ),
            presenting: mergeConfirm
        ) { confirm in
            Button("Merge", role: .destructive) { merge(confirm) }
            Button("Cancel", role: .cancel) { mergeConfirm = nil }
        } message: { confirm in
            // EXP-734: a run's OWN pull request links no issue, so promising
            // completed issues would be a lie.
            switch confirm.target {
            case .issue:
                Text("Merges the pull request, completes every linked issue, and closes the coding session.")
            case .session:
                Text("Merges this run's pull request and closes the coding session.")
            }
        }
        .navigationDestination(item: $sessionTarget) { target in
            AgentSessionRouteView(sessionId: target.sessionId)
                .environment(\.accountId, accountId)
        }
    }

    // MARK: - Past (EXP-746)

    /// The caller's finished runs: title + byline, and a tap opens that run's
    /// session view — where its transcript and its Resume live since EXP-773.
    /// Automation runs stay under Automations (`PastRuns.select` drops every
    /// `started_reason` row). EXP-862: collapsed until the band is tapped.
    @ViewBuilder
    private var pastSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("Past") {
                HStack(spacing: 6) {
                    Text("\(vm.pastRows.count)")
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    AppIcon(
                        pastExpanded ? AppIcons.uiChevronUp : AppIcons.uiChevronDown,
                        size: 12
                    )
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                }
            }
            .contentShape(Rectangle())
            .onTapGesture {
                pastExpanded.toggle()
            }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("past-runs-band")
            if pastExpanded {
                ForEach(vm.pastRows) { row in
                    EndedRunRow(
                        title: PastRuns.title(row.session, issue: row.issue),
                        identifier: row.issue?.identifier,
                        byline: pastByline(row),
                        onOpen: { sessionTarget = .init(sessionId: row.session.id) }
                    )
                    .accessibilityIdentifier("past-run-row")
                }
            }
        }
    }

    /// "macbook · 5m ago" — the ×4 rule, fed the LIVE devices row's label (a
    /// rename never rewrites the session's start-time snapshot) and this
    /// client's own relative time.
    private func pastByline(_ row: AgentsViewModel.PastRow) -> String {
        PastRuns.byline(
            device: row.device.displayLabel,
            relativeTime: relativeWireDate(PastRuns.endedAt(row.session))
        )
    }

    private var noAgentsRow: some View {
        HStack(spacing: 8) {
            Text("No agents running right now.")
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
    }

    // MARK: - Session rows

    // EXP-874: Android's row is the reference (`RunningSessionRow`). The
    // primary tap target and the trailing circles (Merge / Fix conflicts,
    // then Open issue OR the action glyph) are siblings, not nested controls,
    // so every hit area stays reliable.
    @ViewBuilder
    private func sessionRow(_ row: AgentsViewModel.Row) -> some View {
        // EXP-734: a run that opened its own issue-less PR carries the state
        // on its OWN row.
        let state = CodingSessionDisplayState.of(
            session: row.session, prState: row.issue?.prState ?? row.session.prState
        )
        RunningSessionRow(
            session: row.session,
            // Issue runs only: an action/chat/batch run prints no identifier.
            identifier: row.issue?.identifier,
            title: sessionRowTitle(issue: row.issue, session: row.session),
            state: state,
            device: row.device,
            open: sessionRowOpen(row),
            trailing: { sessionRowTrailing(row) },
            footer: {
                // A refused merge captions THIS row (EXP-323) — the message
                // only; a conflict's recovery run took the merge slot.
                if let failure = mergeErrors[row.id] {
                    Text(failure.message)
                        .font(.caption)
                        .foregroundStyle(DesignTokens.Semantic.red)
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        )
        .accessibilityIdentifier("agent-session-row")
    }

    /// Every listed row is the caller's own (EXP-312: live sessions are
    /// owner-only), so with the relay configured the row jumps straight into
    /// the live agent session; without it, into the issue detail.
    private func sessionRowOpen(_ row: AgentsViewModel.Row) -> RunningSessionRowOpen {
        if steerEnabled {
            return .route(.agentSession(accountId: accountId, sessionId: row.session.id))
        }
        if let issue = row.issue {
            return .route(.issue(accountId: accountId, id: issue.id))
        }
        return .none
    }

    @ViewBuilder
    private func sessionRowTrailing(_ row: AgentsViewModel.Row) -> some View {
        // Merge shortcut — merging always closes the run too (EXP-498), so it
        // only shows while there IS an open PR to merge. EXP-535: batch rows
        // merge through their resolved PR's representative issue. EXP-706: a
        // conflict-refused merge REPLACES it with the recovery run.
        if let target = row.mergeTarget {
            if merging.contains(row.id) {
                ProgressView()
                    .controlSize(.mini)
                    .tint(.white)
                    .frame(width: GlassTokens.controlSize, height: GlassTokens.controlSize)
                    .accessibilityLabel("Merging")
            } else if let issue = fixConflictsIssue(row) {
                CircleIconButton(AppIcons.uiBranch, accessibilityLabel: "Fix conflicts") {
                    onFixConflicts(issue.id)
                }
            } else {
                CircleIconButton(AppIcons.prMerged, accessibilityLabel: "Merge") {
                    mergeConfirm = MergeConfirmTarget(rowId: row.id, target: target)
                }
            }
        }

        sessionTrailingControl(row)
    }

    /// EXP-694 (S6): the row's trailing circle names WHAT the run is about.
    /// An issue run opens its issue (the deep-link bus, like the steering
    /// screen); an action or automation run wears that action's own glyph and
    /// opens its editor; a chat or batch run points at nothing.
    @ViewBuilder
    private func sessionTrailingControl(_ row: AgentsViewModel.Row) -> some View {
        if let issue = row.issue, let identifier = issue.identifier, !identifier.isEmpty {
            CircleIconButton(AppIcons.uiIssue, accessibilityLabel: "Open \(identifier)") {
                deps.deepLinkBus.navigateToIssue(issue.id, accountId: accountId)
            }
        } else if let action = sessionAction(row) {
            let target = editTarget(for: row, action: action)
            CircleIconButton(
                action.icon ?? AppIcons.actionDefault,
                accessibilityLabel: target.accessibilityLabel
            ) {
                sessionEditTarget = target
            }
        }
    }

    /// The action a run came from, off the synced store (`action_id` nulls
    /// when the action is deleted — nothing left to edit, so no button).
    private func sessionAction(_ row: AgentsViewModel.Row) -> ActionDto? {
        guard let actionId = row.session.actionId else { return nil }
        return vm.actions.first { $0.id == actionId }
    }

    /// The automation that fired the run (EXP-583: `automation_id`).
    private func sessionAutomation(_ row: AgentsViewModel.Row) -> AutomationDto? {
        guard let automationId = row.session.automationId else { return nil }
        return vm.automations.first { $0.id == automationId }
    }

    /// An automation-started run edits the AUTOMATION when we can resolve it
    /// and the caller owns the team (the form is a write surface with no
    /// read-only mode); everything else lands in the action editor, which is
    /// read-only for non-owners by itself.
    private func editTarget(for row: AgentsViewModel.Row, action: ActionDto) -> SessionEditTarget {
        if let automation = sessionAutomation(row), canEditActions {
            return .automation(automation)
        }
        return .action(action)
    }

    /// Saves an edited automation (EXP-583's owner-gated `automations.update`).
    /// The synced row echoes the change back, so there is no local write.
    private func saveAutomation(
        _ automation: AutomationDto,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        launch: AutomationLaunchPatch
    ) {
        Task {
            do {
                try await deps.automationsApi.update(
                    accountId: accountId,
                    id: automation.id,
                    actionId: actionId,
                    deviceId: deviceId,
                    trigger: trigger,
                    launch: launch
                )
            } catch {
                editError = error.userFacingMessage
            }
        }
    }

    /// A refused merge on a REAL conflict offers the builtin recovery run in
    /// the merge slot (EXP-486/706). EXP-535: a batch row recovers through the
    /// same representative issue its Merge button used.
    private func fixConflictsIssue(_ row: AgentsViewModel.Row) -> IssueEntity? {
        guard let failure = mergeErrors[row.id], failure.isConflict,
              let issue = row.issue ?? row.batchPrIssue,
              canFixConflicts(issue) else { return nil }
        return issue
    }

    /// The recovery run rebases the PR's branch, so it needs one recorded —
    /// the same gate the Reviews rows apply.
    private func canFixConflicts(_ issue: IssueEntity) -> Bool {
        steerEnabled && !(issue.branch ?? "").isEmpty
    }

    /// Merge the row's PR — the server always ends its session too (EXP-498).
    /// No local list surgery: the server flips the row to `ended`, which
    /// drops it out of the live query through sync. A refusal captions THIS
    /// row.
    private func merge(_ confirm: MergeConfirmTarget) {
        mergeConfirm = nil
        mergeErrors[confirm.rowId] = nil
        merging.insert(confirm.rowId)
        Task {
            do {
                // EXP-734: an action or chat run's PR links no issue — it
                // merges through the session row the server stamped it on.
                switch confirm.target {
                case let .issue(issueId):
                    try await deps.issuesApi.mergePr(
                        accountId: accountId,
                        issueId: issueId
                    )
                case let .session(sessionId):
                    try await deps.codingSessionsApi.mergePr(
                        accountId: accountId,
                        sessionId: sessionId
                    )
                }
            } catch {
                mergeErrors[confirm.rowId] = MergeFailure(error: error)
            }
            merging.remove(confirm.rowId)
        }
    }
}
