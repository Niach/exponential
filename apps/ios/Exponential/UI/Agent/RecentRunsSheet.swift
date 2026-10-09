import ExpCore
import ExpUI
import SwiftUI

/// EXP-923: the caller's finished runs — history, behind the Agent page's
/// toolbar glyph instead of a fold under the composer (the ×4 rule).
///
/// EXP-1061: the rows are the `SessionTree.sessionTree` SELECTOR drawn,
/// exactly like the Running band (and web's Recent panel): a resume
/// succession is ONE row, a child nests under its parent, top level newest
/// ACTIVITY first. EXP-1248: each row is THE `SessionRow` (big): `Done ·
/// macbook · 10 h`, the device glyph trailing, no fold (children always
/// show); a tap opens that run's Work screen.
///
/// EXP-1186: history spans EVERY member team; with more than one, the rows
/// split into one band per team (`TeamAvatar` + name, no count).
struct RecentRunsSheet: View {
    let vm: AgentsViewModel
    /// The picked run — the page dismisses this sheet and pushes it.
    let onOpen: (String) -> Void

    @Environment(TeamState.self) private var teamState

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

    /// One list's rows, drawn as the session tree. EXP-1061: the cap
    /// (`PastRuns.cap`) applies to the ROWS first, so a child whose parent
    /// fell off it is a top-level orphan.
    @ViewBuilder
    private func tree(_ source: [AgentsViewModel.PastRow]) -> some View {
        let rows = SessionTree.visibleRows(SessionTree.sessionTree(source.map(\.session)))
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        let byId = Dictionary(
            source.map { ($0.session.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
            if let past = byId[entry.node.session.id] {
                row(past, guide: guides[index])
            }
        }
    }

    private func row(_ row: AgentsViewModel.PastRow, guide: TreeGuide) -> some View {
        Button { onOpen(row.session.id) } label: {
            SessionListRow(
                session: row.session,
                // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
                identifier: PastRuns.identifier(
                    row.session, issue: row.issue, batchIssues: row.batchIssues
                ),
                title: PastRuns.title(
                    row.session, issue: row.issue, batchIssues: row.batchIssues
                ),
                prState: row.issue?.prState ?? row.session.prState,
                device: row.device,
                deviceIcon: SessionListRow.deviceIcon(row.session, devices: vm.devices),
                guide: guide
            )
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("past-run-row")
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
