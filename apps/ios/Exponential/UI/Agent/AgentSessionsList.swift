import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs
/// ("Running"; EXP-1186: across EVERY member team, one band per team —
/// `TeamAvatar` + name — once the caller is in more than one; nested by `SessionTree`, EXP-818). Moved verbatim
/// from the Devices tab, which keeps machines only (web parity, EXP-818).
///
/// EXP-996: the list draws the session TREE, not a flat roll of strangers —
/// `SessionTree.sessionTree` (the ×4 selector): a resume succession is ONE row
/// and a child run nests under its parent.
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
    @Environment(TeamState.self) private var teamState

    /// EXP-897/EXP-996: the nodes folded shut, keyed by the NODE key
    /// (`SessionTree.nodeKey`) — a parent's chevron hides its whole subtree.
    /// The ×4 rule.
    @State private var collapsedRunning: Set<String> = []

    var body: some View {
        // EXP-818: the Running group is a filled BAND over flat rows
        // (`GlassSectionBand` + `.flatRow()`) — the runs read as a table, the
        // way web's and the IDE's session lists do.
        // EXP-1186: grouped per team only when the caller is in several; one
        // team draws exactly the single "Running" band it always did.
        let groups = TeamGroups.isMultiTeam(teamState.teams)
            ? TeamGroups.group(vm.rows, teams: teamState.teams) { $0.session.teamId }
            : []
        if groups.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Running")
                if vm.rows.isEmpty {
                    noAgentsRow
                } else {
                    tree(vm.rows)
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 12) {
                ForEach(groups) { group in
                    VStack(alignment: .leading, spacing: 0) {
                        GlassSectionBand(group.team.name) {
                            TeamAvatar(team: group.team, size: 16)
                        } trailing: {
                            EmptyView()
                        }
                        tree(group.items)
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("agent-sessions-team-\(group.team.id)")
                }
            }
        }
    }

    /// One band's rows. EXP-818: a run started by another run nests under its
    /// parent, indented (`SessionTree`, the ×4 rule). EXP-897: every parent
    /// carries a fold, 14 pt per level. EXP-965: the connector says which row
    /// hangs off which; the band's rows stack flush (spacing 0), so it
    /// bridges no gap.
    @ViewBuilder
    private func tree(_ source: [AgentsViewModel.Row]) -> some View {
        let rows = runningRows(source)
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        let byId = Dictionary(
            source.map { ($0.session.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
            treeRow(entry, rows: byId)
                .treeGuides(guides[index])
        }
    }

    // MARK: - Nesting (EXP-818/EXP-897/EXP-996)

    /// EXP-996: the list is the `sessionTree` SELECTOR drawn — a resume
    /// succession is ONE row and children nest.
    private func runningRows(_ source: [AgentsViewModel.Row]) -> [SessionTree.FlatRow] {
        SessionTree.visibleRows(
            SessionTree.sessionTree(source.map(\.session)),
            collapsed: collapsedRunning
        )
    }

    /// One drawn row. The tree is built from THESE rows, so the lookup always
    /// resolves; a row that somehow did not is simply not drawn.
    @ViewBuilder
    private func treeRow(
        _ entry: SessionTree.FlatRow, rows: [String: AgentsViewModel.Row]
    ) -> some View {
        if let row = rows[entry.node.session.id] {
            sessionRow(
                row,
                expandable: entry.hasChildren,
                expanded: !collapsedRunning.contains(entry.key),
                onToggle: { toggle(&collapsedRunning, entry.key) }
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
            title: sessionRowTitle(
                issue: row.issue, session: row.session, batchIssues: row.batchIssues
            ),
            state: state,
            device: row.device,
            open: sessionRowOpen(row),
            expandable: expandable,
            expanded: expanded,
            onToggle: onToggle,
            marks: RunningSessionRowMarks(
                needsYou: !(row.session.pendingQuestion ?? "").isEmpty
            )
        )
        .accessibilityIdentifier("agent-session-row")
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
