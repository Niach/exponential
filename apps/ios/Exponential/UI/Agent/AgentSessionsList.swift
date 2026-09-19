import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs in the
/// active team ("Running", nested by `SessionTree`, EXP-818). Moved verbatim
/// from the Devices tab, which keeps machines only (web parity, EXP-818).
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

    /// EXP-897: the runs folded shut. A parent run's children are nested under
    /// it (`SessionTree`), and its chevron hides the whole subtree — the ×4
    /// rule.
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
                // connector says which row hangs off which.
                let rows = runningRows
                let guides = TreeGuides.compute(depths: rows.map(\.depth))
                ForEach(Array(rows.enumerated()), id: \.element.session.session.id) { index, entry in
                    sessionRow(
                        entry.session,
                        expandable: entry.hasChildren,
                        expanded: !collapsedRunning.contains(entry.session.session.id),
                        onToggle: { toggle(&collapsedRunning, entry.session.session.id) }
                    )
                    .treeGuides(guides[index])
                }
            }
        }
    }

    // MARK: - Nesting (EXP-818/EXP-897)

    private var runningRows: [SessionTree.Row<AgentsViewModel.Row>] {
        SessionTree.visibleRows(
            SessionTree.nest(
                vm.rows,
                id: { $0.session.id },
                parent: { $0.session.parentSessionId },
                startedAt: { $0.session.startedAt }
            ),
            collapsed: collapsedRunning,
            rowId: { $0.session.id }
        )
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
            onToggle: onToggle
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
