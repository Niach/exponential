import ExpCore
import ExpUI
import SwiftUI

/// EXP-923: the caller's finished runs — history, behind the Agent page's
/// toolbar glyph instead of a fold under the composer (the ×4 rule). The
/// composer page lists what is RUNNING; what is over is one tap away and
/// never in the way.
///
/// No fold of its own: the sheet is not a disclosure. EXP-1061: the rows are
/// the `SessionTree.sessionTree` SELECTOR drawn, exactly like the Running band
/// (and web's Recent panel): a resume succession is ONE row, a child nests
/// under its parent, every parent folds, top level newest ACTIVITY first. Each run row
/// is the `EndedRunRow` the band used to draw, each tap opening that run's
/// Work screen.
///
/// EXP-1186: history spans EVERY member team; with more than one, the rows
/// split into one band per team (`TeamAvatar` + name), each its own tree.
struct RecentRunsSheet: View {
    let vm: AgentsViewModel
    /// The picked run — the page dismisses this sheet and pushes it.
    let onOpen: (String) -> Void

    @Environment(TeamState.self) private var teamState

    /// The nodes folded shut, keyed by `SessionTree.nodeKey` (the ×4 rule).
    @State private var collapsed: Set<String> = []

    var body: some View {
        GlassSheetChrome(title: "Recent") {
            VStack(alignment: .leading, spacing: 0) {
                if vm.pastRows.isEmpty {
                    emptyNote
                } else if TeamGroups.isMultiTeam(teamState.teams) {
                    let groups = TeamGroups.group(vm.pastRows, teams: teamState.teams) {
                        $0.session.teamId
                    }
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
                        }
                    }
                } else {
                    tree(vm.pastRows)
                }
            }
            .padding(.horizontal, 10)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("recent-runs-sheet")
    }

    /// One list's rows, drawn as the session tree.
    @ViewBuilder
    private func tree(_ source: [AgentsViewModel.PastRow]) -> some View {
        let rows = pastRows(source)
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        let byId = Dictionary(
            source.map { ($0.session.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
            treeRow(entry, rows: byId)
                .treeGuides(guides[index])
        }
    }

    /// One drawn row.
    @ViewBuilder
    private func treeRow(
        _ entry: SessionTree.FlatRow, rows: [String: AgentsViewModel.PastRow]
    ) -> some View {
        if let past = rows[entry.node.session.id] {
            row(
                past,
                expandable: entry.hasChildren,
                expanded: !collapsed.contains(entry.key),
                onToggle: { toggle(entry.key) }
            )
        }
    }

    private func toggle(_ key: String) {
        if collapsed.contains(key) {
            collapsed.remove(key)
        } else {
            collapsed.insert(key)
        }
    }

    private func row(
        _ row: AgentsViewModel.PastRow,
        expandable: Bool,
        expanded: Bool,
        onToggle: @escaping () -> Void
    ) -> some View {
        EndedRunRow(
            title: PastRuns.title(
                row.session, issue: row.issue, batchIssues: row.batchIssues
            ),
            // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
            identifier: PastRuns.identifier(
                row.session, issue: row.issue, batchIssues: row.batchIssues
            ),
            byline: byline(row),
            expandable: expandable,
            expanded: expanded,
            onToggle: onToggle,
            onOpen: { onOpen(row.session.id) }
        )
        .accessibilityIdentifier("past-run-row")
    }

    /// "macbook · 5m ago" — the ×4 rule, fed the LIVE devices row's label (a
    /// rename never rewrites the session's start-time snapshot) and this
    /// client's own relative time.
    private func byline(_ row: AgentsViewModel.PastRow) -> String {
        PastRuns.byline(
            device: row.device.displayLabel,
            relativeTime: relativeWireDate(PastRuns.endedAt(row.session))
        )
    }

    /// EXP-1061: history is the same tree as the Running band — the cap
    /// (`PastRuns.cap`) applies to the ROWS first, so a child whose parent fell
    /// off it is a top-level orphan.
    private func pastRows(_ source: [AgentsViewModel.PastRow]) -> [SessionTree.FlatRow] {
        SessionTree.visibleRows(
            SessionTree.sessionTree(source.map(\.session)),
            collapsed: collapsed
        )
    }

    private var emptyNote: some View {
        Text("No recent runs")
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 16)
            .accessibilityIdentifier("recent-runs-empty")
    }
}
