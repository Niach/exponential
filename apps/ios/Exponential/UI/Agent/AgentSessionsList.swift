import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs in the
/// active team ("Running", nested by `SessionTree`, EXP-818). Moved verbatim
/// from the Devices tab, which keeps machines only (web parity, EXP-818).
///
/// EXP-996: the list draws the session TREE, not a flat roll of strangers —
/// `SessionTree.sessionTree` (the ×4 selector): a resume succession is ONE row,
/// a child run nests under its parent, the runs of a workflow sit under one
/// group row that LINKS to the workflow, and a stack sits under one group row
/// in linear order, lowest first.
///
/// EXP-923: the finished runs are NOT here. "Recent" was a fold nobody opened
/// under the composer; history now lives behind the page's toolbar glyph, in
/// its own sheet (`RecentRunsSheet`) — the ×4 rule.
///
/// EXP-893: a row only OPENS the run — its Work screen, where Merge / Fix
/// conflicts live on the Changes face and the issue is one switch away. The
/// trailing Merge / Fix-conflicts and Open-issue / Open-action circles are
/// gone from every row.
struct AgentSessionsList: View {
    let vm: AgentsViewModel
    let steerEnabled: Bool

    @Environment(\.accountId) private var accountId

    /// EXP-897/EXP-996: the nodes folded shut, keyed by the NODE key
    /// (`SessionTree.nodeKey`: a run's id, else `workflow:<id>` /
    /// `stack:<rootIssueId>`) — a group folds exactly like a parent run does,
    /// and its chevron hides the whole subtree. The ×4 rule.
    @State private var collapsedRunning: Set<String> = []

    var body: some View {
        // EXP-818: the Running group is a filled BAND over flat rows
        // (`GlassSectionBand` + `.flatRow()`) — the runs read as a table, the
        // way web's and the IDE's session lists do.
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("Running")
            if vm.rows.isEmpty {
                noAgentsRow
            } else {
                // EXP-818: a run started by another run nests under its
                // parent, indented (`SessionTree`, the ×4 rule). EXP-897:
                // every parent carries a fold, 14 pt per level. EXP-965: the
                // connector says which row hangs off which; the band's rows
                // stack flush (spacing 0), so it bridges no gap.
                let rows = runningRows
                let guides = TreeGuides.compute(depths: rows.map(\.depth))
                let byId = Dictionary(
                    vm.rows.map { ($0.session.id, $0) }, uniquingKeysWith: { a, _ in a }
                )
                ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
                    treeRow(entry, rows: byId)
                        .treeGuides(guides[index])
                }
            }
        }
    }

    // MARK: - Nesting (EXP-818/EXP-897/EXP-996)

    /// EXP-996: the list is the `sessionTree` SELECTOR drawn — a resume
    /// succession is ONE row, children nest, and the runs of a workflow or of a
    /// stack sit under one group row. `vm.sessionTreeContext` carries what the
    /// rows alone cannot say (which workflow, which stack).
    private var runningRows: [SessionTree.FlatRow] {
        SessionTree.visibleRows(
            SessionTree.sessionTree(vm.rows.map(\.session), context: vm.sessionTreeContext),
            collapsed: collapsedRunning
        )
    }

    /// One drawn row: a run, or the group row its runs hang off.
    @ViewBuilder
    private func treeRow(
        _ entry: SessionTree.FlatRow, rows: [String: AgentsViewModel.Row]
    ) -> some View {
        let expanded = !collapsedRunning.contains(entry.key)
        let onToggle = { toggle(&collapsedRunning, entry.key) }
        switch entry.node {
        case let .session(node):
            // The tree is built from THESE rows, so the lookup always resolves;
            // a row that somehow did not is simply not drawn.
            if let row = rows[node.session.id] {
                sessionRow(
                    row,
                    node: node,
                    expandable: entry.hasChildren,
                    expanded: expanded,
                    onToggle: onToggle
                )
            }
        case let .workflow(group):
            // EXP-1068: the workflow's status dot + `3 running · 5 of 8 done`.
            SessionGroupRow(
                glyph: AppIcons.navWorkflows,
                title: group.name,
                count: group.children.count,
                open: .route(.workflow(accountId: accountId, id: group.workflowId)),
                key: entry.key,
                expanded: expanded,
                onToggle: onToggle,
                status: group.status,
                caption: SessionTree.workflowGroupCaption(
                    liveRuns: group.liveRuns,
                    nodesDone: group.nodesDone,
                    nodesTotal: group.nodesTotal
                )
            )
        case let .stack(group):
            // A stack is not a place you can go — its members are its only
            // page — so the row only folds.
            SessionGroupRow(
                glyph: AppIcons.prStack,
                title: SessionTree.stackGroupLabel,
                count: group.children.count,
                open: .none,
                key: entry.key,
                expanded: expanded,
                onToggle: onToggle
            )
        }
    }

    private func toggle(_ set: inout Set<String>, _ id: String) {
        if set.contains(id) {
            set.remove(id)
        } else {
            set.insert(id)
        }
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

    // EXP-874: Android's row is the reference (`RunningSessionRow`).
    @ViewBuilder
    private func sessionRow(
        _ row: AgentsViewModel.Row,
        node: SessionTree.SessionNode? = nil,
        expandable: Bool = false,
        expanded: Bool = true,
        onToggle: (() -> Void)? = nil
    ) -> some View {
        // EXP-734: a run that opened its own issue-less PR carries the state
        // on its OWN row.
        let state = CodingSessionDisplayState.of(
            session: row.session, prState: row.issue?.prState ?? row.session.prState
        )
        RunningSessionRow(
            session: row.session,
            // An issue run's id, a batch's `EXP-874 +2` (EXP-876); an
            // action/chat run prints none.
            identifier: sessionRowIdentifier(
                issue: row.issue, session: row.session, batchIssues: row.batchIssues
            ),
            title: reviewTitle(node) ?? sessionRowTitle(
                issue: row.issue, session: row.session, batchIssues: row.batchIssues
            ),
            state: state,
            device: row.device,
            open: sessionRowOpen(row),
            expandable: expandable,
            expanded: expanded,
            onToggle: onToggle,
            marks: RunningSessionRowMarks(
                needsYou: !(row.session.pendingQuestion ?? "").isEmpty,
                duplicateLive: node?.duplicateLive == true,
                account: nonDefaultAccount(row.session)
            )
        )
        .accessibilityIdentifier("agent-session-row")
    }

    // MARK: - Workflow marks (EXP-1068)

    /// A REVIEW chain's title: `Review r2 · approved` off its node's synced
    /// `review_round` + latest `review`. Nil on every other row.
    private func reviewTitle(_ node: SessionTree.SessionNode?) -> String? {
        guard let node, node.session.workflowRole == DomainContract.wfSessionRoleReview else {
            return nil
        }
        let nodeRow = vm.sessionTreeContext.workflowNode(id: node.session.workflowNodeId)
        let latest = WorkflowNodeReview.parse(nodeRow?.review)
        let verdict = SessionTree.reviewRoundVerdict(
            round: node.reviewRound,
            nodeReviewRound: nodeRow?.reviewRound,
            latestRound: latest?.round,
            latestVerdict: latest?.verdict
        )
        return SessionTree.reviewRowCaption(
            round: node.reviewRound,
            verdict: verdict,
            live: SessionTree.sessionRowIsLive(status: node.session.status)
        )
    }

    /// The run's account label when it is NOT the host machine's default for
    /// that agent (`launch_defaults.defaultAccount` when the agent is the
    /// machine's default agent, else its ambient `system` login). A host this
    /// phone has not synced shows any non-ambient account.
    private func nonDefaultAccount(_ session: CodingSessionEntity) -> String? {
        guard let account = session.agentAccount, !account.isEmpty else { return nil }
        let device = session.deviceId.flatMap { id in
            vm.devices?.first { $0.deviceId == id }
        }
        let system = AgentAccountsRows.systemProfileId
        let defaults = device?.launchDefaults
        let fallback = defaults?.defaultAccount.flatMap { $0.isEmpty ? nil : $0 } ?? system
        let machineDefault = (session.agent != nil && defaults?.defaultAgent == session.agent)
            ? fallback : system
        guard account != machineDefault else { return nil }
        let profile = session.agent.flatMap { agent in
            device?.agentAccounts?[agent]?.profiles?.first { $0.id == account }
        }
        let label = profile?.label ?? profile?.email
        if let label, !label.isEmpty { return label }
        return account == system ? "Default" : account
    }

    /// Every listed row is the caller's own (EXP-312: live sessions are
    /// owner-only), so with the relay configured the row jumps straight into
    /// the run's Work screen; without it, into the issue's.
    private func sessionRowOpen(_ row: AgentsViewModel.Row) -> RunningSessionRowOpen {
        if steerEnabled {
            return .route(.agentSession(accountId: accountId, sessionId: row.session.id))
        }
        if let issue = row.issue {
            return .route(.issue(accountId: accountId, id: issue.id))
        }
        return .none
    }
}
