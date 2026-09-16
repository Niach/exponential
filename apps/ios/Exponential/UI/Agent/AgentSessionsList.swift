import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs in the
/// active team ("Running", nested by `SessionTree`, EXP-818) above the
/// finished ones ("Recent", EXP-746; "Past" until EXP-886). Moved verbatim from the Devices tab,
/// which keeps machines only (web parity, EXP-818).
///
/// EXP-893: a row only OPENS the run — its Work screen, where Merge / Fix
/// conflicts live on the Changes face and the issue is one switch away. The
/// trailing Merge / Fix-conflicts and Open-issue / Open-action circles are
/// gone from every row.
struct AgentSessionsList: View {
    let vm: AgentsViewModel
    let steerEnabled: Bool

    @Environment(\.accountId) private var accountId

    /// The Recent rows' tap target — the page pushes it.
    @State private var sessionTarget: StartedRunWatcher.StartedSession?
    /// EXP-862: "Recent" is FOLDED by default on every client — finished runs
    /// are history, and an unfolded list of them buried the live ones. The
    /// header expands inline; EXP-886 dropped its count ×4.
    @State private var pastExpanded = false
    /// EXP-897: the runs folded shut in each band. A parent run's children are
    /// nested under it (`SessionTree`), and its chevron hides the whole
    /// subtree — the ×4 rule, in BOTH bands.
    @State private var collapsedRunning: Set<String> = []
    @State private var collapsedPast: Set<String> = []

    var body: some View {
        // EXP-818: the Running/Recent groups are filled BANDS over flat rows
        // (`GlassSectionBand` + `.flatRow()`) — the runs read as a table, the
        // way web's and the IDE's session lists do.
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Running")
                if vm.rows.isEmpty {
                    noAgentsRow
                } else {
                    // EXP-818: a run started by another run nests under its
                    // parent, indented (`SessionTree`, the ×4 rule). EXP-897:
                    // every parent carries a fold, 14 pt per level.
                    ForEach(runningRows, id: \.session.id) { entry in
                        sessionRow(
                            entry.session,
                            expandable: entry.hasChildren,
                            expanded: !collapsedRunning.contains(entry.session.session.id),
                            onToggle: { toggle(&collapsedRunning, entry.session.session.id) }
                        )
                        .padding(.leading, CGFloat(entry.depth) * Self.indentPerLevel)
                    }
                }
            }

            // EXP-746: the caller's finished runs. Absent entirely when
            // there are none — an empty history is not news.
            if !vm.pastRows.isEmpty {
                pastSection
            }
        }
        .navigationDestination(item: $sessionTarget) { target in
            WorkScreen(subject: .session(id: target.sessionId))
                .environment(\.accountId, accountId)
        }
    }

    // MARK: - Recent (EXP-746)

    /// The caller's finished runs: title + byline, and a tap opens that run's
    /// Work screen — where its transcript and its Resume live since EXP-773.
    /// Automation runs stay under Automations (`PastRuns.select` drops every
    /// `started_reason` row). EXP-862: collapsed until the band is tapped.
    @ViewBuilder
    private var pastSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand("Recent") {
                AppIcon(
                    pastExpanded ? AppIcons.uiChevronUp : AppIcons.uiChevronDown,
                    size: 12
                )
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
            .contentShape(Rectangle())
            .onTapGesture {
                pastExpanded.toggle()
            }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("past-runs-band")
            if pastExpanded {
                // EXP-897: Recent nests too — a child run is history under its
                // parent, not a stranger in the same list.
                ForEach(pastRows, id: \.session.id) { entry in
                    let row = entry.session
                    EndedRunRow(
                        title: PastRuns.title(
                            row.session, issue: row.issue, batchIssues: row.batchIssues
                        ),
                        // EXP-876: a batch's `EXP-874 +2`, an issue run's id.
                        identifier: PastRuns.identifier(
                            row.session, issue: row.issue, batchIssues: row.batchIssues
                        ),
                        byline: pastByline(row),
                        expandable: entry.hasChildren,
                        expanded: !collapsedPast.contains(row.session.id),
                        onToggle: { toggle(&collapsedPast, row.session.id) },
                        onOpen: { sessionTarget = .init(sessionId: row.session.id) }
                    )
                    .padding(.leading, CGFloat(entry.depth) * Self.indentPerLevel)
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

    // MARK: - Nesting (EXP-818/EXP-897)

    /// 14 pt per level, the ×4 measure (was 16 before the fold chevron took
    /// its own 14 pt of leading).
    private static let indentPerLevel: CGFloat = 14

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

    private var pastRows: [SessionTree.Row<AgentsViewModel.PastRow>] {
        SessionTree.visibleRows(
            SessionTree.nest(
                vm.pastRows,
                id: { $0.session.id },
                parent: { $0.session.parentSessionId },
                startedAt: { $0.session.startedAt }
            ),
            collapsed: collapsedPast,
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
