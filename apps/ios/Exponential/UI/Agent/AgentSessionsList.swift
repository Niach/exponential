import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs in the
/// active team ("Running", nested by `SessionTree`, EXP-818) above the
/// finished ones ("Past", EXP-746). Moved verbatim from the Devices tab,
/// which keeps machines only (web parity, EXP-818).
///
/// Session rows open the live agent session view directly when the relay
/// is configured, else fall back to the issue detail; the trailing control
/// (EXP-694) names the run's subject — the action's glyph to its editor,
/// nothing on chat/batch/issue runs (the title already prints the
/// identifier). A refused merge captions its row and, on a REAL conflict,
/// offers the builtin recovery run, which the page seeds into the composer
/// above (no sheet: the composer IS the launcher now).
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
            relativeTime: relativeDate(PastRuns.endedAt(row.session))
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

    // The primary tap target and the trailing affordances (merge-and-close,
    // edit) are siblings (not nested controls) so every hit area stays
    // reliable.
    @ViewBuilder
    private func sessionRow(_ row: AgentsViewModel.Row) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            sessionRowBody(row)
            if let failure = mergeErrors[row.id] {
                mergeErrorCaption(row, failure: failure)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 12)
        .flatRow()
        .accessibilityIdentifier("agent-session-row")
        // EXP-698: the row's tap goes to the LIVE session when steering is on;
        // the issue keeps a route: press and hold.
        .contextMenu {
            if let issue = row.issue, !(issue.identifier ?? "").isEmpty {
                // A NavigationLink inside a context menu is lowered to a
                // UIMenu outside the NavigationStack and never fires; route
                // through the deep-link bus like the steering screen does.
                Button {
                    deps.deepLinkBus.navigateToIssue(issue.id, accountId: accountId)
                } label: {
                    Label("Open issue", appIcon: AppIcons.uiIssue)
                }
            }
        }
    }

    @ViewBuilder
    private func sessionRowBody(_ row: AgentsViewModel.Row) -> some View {
        HStack(spacing: 12) {
            // Every listed row is the caller's own (EXP-312: live sessions are
            // owner-only), so with the relay configured the row jumps straight
            // into the live agent session; without it, into the issue detail.
            Group {
                if steerEnabled {
                    NavigationLink(value: AppRoute.agentSession(
                        accountId: accountId, sessionId: row.session.id
                    )) {
                        sessionRowContent(row)
                    }
                    .buttonStyle(.plain)
                } else if let issue = row.issue {
                    NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                        sessionRowContent(row)
                    }
                    .buttonStyle(.plain)
                } else {
                    sessionRowContent(row)
                }
            }

            // Merge shortcut — merging always closes the run too (EXP-498),
            // so it only shows while there IS an open PR to merge. EXP-535:
            // batch rows merge through their resolved PR's representative
            // issue — same button, same server call.
            if let target = row.mergeTarget {
                if merging.contains(row.id) {
                    ProgressView()
                        .controlSize(.mini)
                        .tint(.white)
                        .frame(width: GlassTokens.controlSize, height: GlassTokens.controlSize)
                        .accessibilityLabel("Merging")
                } else {
                    CircleIconButton(AppIcons.prMerged, accessibilityLabel: "Merge") {
                        mergeConfirm = MergeConfirmTarget(rowId: row.id, target: target)
                    }
                }
            }

            sessionTrailingControl(row)
        }
        .frame(minHeight: GlassTokens.controlSize)
    }

    /// EXP-694 (S6): the row's trailing affordance names WHAT the run is
    /// about. An action or automation run wears that action's own glyph and
    /// opens its editor; an issue run gets nothing (`SessionRowTitle` already
    /// prints the identifier, EXP-698); a chat or batch run points at nothing.
    @ViewBuilder
    private func sessionTrailingControl(_ row: AgentsViewModel.Row) -> some View {
        if !(row.issue?.identifier ?? "").isEmpty {
            EmptyView()
        } else if let action = sessionAction(row) {
            let target = editTarget(for: row, action: action)
            GhostIconButton(
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

    /// A refused merge (conflicts, branch protection, GitHub App errors)
    /// captions THIS row — and a conflict is the common case, so the builtin
    /// recovery run sits right next to the reason (EXP-486, the same shape as
    /// the Reviews rows, EXP-323).
    @ViewBuilder
    private func mergeErrorCaption(_ row: AgentsViewModel.Row, failure: MergeFailure) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(failure.message)
                .font(.caption)
                .foregroundStyle(DesignTokens.Semantic.red)
                .fixedSize(horizontal: false, vertical: true)

            // EXP-535: a batch row's refused merge recovers through the same
            // representative issue its Merge button used — the composer's PR
            // picker normalizes any linked issue id to its option.
            if failure.isConflict, let issue = row.issue ?? row.batchPrIssue, canFixConflicts(issue) {
                GlassPill("Fix conflicts", icon: AppIcons.uiBranch, mode: .action {
                    onFixConflicts(issue.id)
                })
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
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

    @ViewBuilder
    private func sessionRowContent(_ row: AgentsViewModel.Row) -> some View {
        // The parked states render a static dot/label instead of the
        // pulsing-green "Coding now": review green, done blue (once the PR
        // merges), needs-input amber while the agent waits on a picker
        // (EXP-194/EXP-214). EXP-734: a run that opened its own issue-less
        // PR carries the state on its OWN row.
        let state = CodingSessionDisplayState.of(
            session: row.session, prState: row.issue?.prState ?? row.session.prState
        )
        // EXP-550: the host machine stopped heartbeating (lid closed) — the
        // run is PAUSED, not ended, and resumes when the machine returns.
        let paused = row.device.isPaused(state)
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                // EXP-688: line 1 is shared with the steering screen's header
                // (SessionRowTitle) so the two can never drift.
                SessionRowTitle(
                    identifier: row.issue?.identifier,
                    title: sessionRowTitle(issue: row.issue, session: row.session),
                    state: state,
                    paused: paused,
                    // EXP-848: the dot pulses on the device-written turn flag,
                    // not on the row merely being `running`.
                    busy: row.session.agentBusy
                )
                // EXP-850 §8: the device-written caption (today the running
                // workflow's) is the row's SECOND line, above the byline.
                // Only a live row (web/desktop rule): a merged run never
                // shows what it "is doing", whatever the column still says.
                if state != .done, let caption = row.session.agentCaption, !caption.isEmpty {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                }
                HStack(spacing: 6) {
                    if paused {
                        Text("Paused")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                            .lineLimit(1)
                    } else if let label = sessionStateLabel(state) {
                        Text(label)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(sessionStateColor(state))
                            .lineLimit(1)
                    }
                    // EXP-804: BESIDE the state, never instead of it — a
                    // walled run still reads `running`.
                    SessionBlockedBadge(blocked: row.session.blocked)
                    Text(byline(row, paused: paused))
                        .font(.caption)
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 0)
        }
        .contentShape(Rectangle())
    }

    /// EXP-549: the machine name comes from the LIVE devices row (a rename
    /// never rewrites the session's start-time snapshot). EXP-550: a paused
    /// row says WHY instead of how long ago it started.
    private func byline(_ row: AgentsViewModel.Row, paused: Bool) -> String {
        let device = row.device.displayLabel
        if paused { return "\(device) · offline" }
        let started = relativeDate(row.session.startedAt)
        return started.isEmpty ? device : "\(device) · started \(started)"
    }

    private func relativeDate(_ s: String) -> String {
        // Electric syncs started_at as Postgres text (space separator, hour-only
        // offset), which ISO8601DateFormatter alone rejects — WireTimestamps
        // handles both wire forms (EXP-169).
        guard let date = WireTimestamps.parse(s) else { return "" }
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}
