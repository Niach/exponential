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

    // EXP-874: Android's row is the reference (`RunningSessionRow`).
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
            open: sessionRowOpen(row)
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
