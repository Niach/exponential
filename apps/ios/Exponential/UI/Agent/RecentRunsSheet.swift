import ExpCore
import ExpUI
import SwiftUI

/// EXP-923: the caller's finished runs — history, behind the Agent page's
/// toolbar glyph instead of a fold under the composer (the ×4 rule). The
/// composer page lists what is RUNNING; what is over is one tap away and
/// never in the way.
///
/// No fold of its own anywhere: a plain list, nested by `SessionTree` the way
/// the Running band is (14 pt per level, EXP-965's connector), each row the
/// same `EndedRunRow` the band used to draw, each tap opening that run's Work
/// screen.
struct RecentRunsSheet: View {
    let vm: AgentsViewModel
    /// The picked run — the page dismisses this sheet and pushes it.
    let onOpen: (String) -> Void

    var body: some View {
        GlassSheetChrome(title: "Recent") {
            VStack(alignment: .leading, spacing: 0) {
                if vm.pastRows.isEmpty {
                    emptyNote
                } else {
                    let rows = pastRows
                    let guides = TreeGuides.compute(depths: rows.map(\.depth))
                    ForEach(Array(rows.enumerated()), id: \.element.session.session.id) { index, entry in
                        row(entry.session)
                            .treeGuides(guides[index])
                    }
                }
            }
            .padding(.horizontal, 10)
            .padding(.bottom, 16)
        }
        .accessibilityIdentifier("recent-runs-sheet")
    }

    private func row(_ row: AgentsViewModel.PastRow) -> some View {
        EndedRunRow(
            title: PastRuns.title(
                row.session, issue: row.issue, batchIssues: row.batchIssues
            ),
            // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
            identifier: PastRuns.identifier(
                row.session, issue: row.issue, batchIssues: row.batchIssues
            ),
            byline: byline(row),
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

    /// EXP-897: history nests too — a child run belongs under its parent here
    /// as much as in the Running band. Nothing folds, so every row is visible.
    private var pastRows: [SessionTree.Row<AgentsViewModel.PastRow>] {
        SessionTree.nest(
            vm.pastRows,
            id: { $0.session.id },
            parent: { $0.session.parentSessionId },
            startedAt: { $0.session.startedAt }
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
