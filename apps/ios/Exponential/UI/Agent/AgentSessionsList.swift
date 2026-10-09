import ExpCore
import ExpUI
import SwiftUI

/// EXP-825: the Agent page's sessions — the caller's OWN live runs
/// ("Running"; EXP-1186: across EVERY member team, one band per team —
/// `TeamAvatar` + name — once the caller is in more than one).
///
/// EXP-996: the list draws the session TREE, not a flat roll of strangers —
/// `SessionTree.sessionTree` (the ×4 selector): a resume succession is ONE row
/// and a child run nests under its parent.
///
/// EXP-923: the finished runs are NOT here; history lives behind the page's
/// toolbar glyph (`RecentRunsSheet`).
///
/// EXP-1248: every row is THE `SessionRow` (big), ×4: the run mark at
/// 12 + 14·depth, no fold chevron (children always show), the caption
/// `Building · macbook · 21 h`, the device glyph trailing. A row only OPENS
/// the run. The bands carry no count.
struct AgentSessionsList: View {
    let vm: AgentsViewModel
    let steerEnabled: Bool

    @Environment(\.accountId) private var accountId
    @Environment(TeamState.self) private var teamState

    var body: some View {
        // EXP-818: the Running group is a filled BAND over flat rows.
        // EXP-1186: grouped per team only when the caller is in several.
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

    /// One band's rows: the `sessionTree` selector drawn, gapless, every
    /// child under its parent with its connector.
    @ViewBuilder
    private func tree(_ source: [AgentsViewModel.Row]) -> some View {
        let rows = SessionTree.visibleRows(SessionTree.sessionTree(source.map(\.session)))
        let guides = TreeGuides.compute(depths: rows.map(\.depth))
        let byId = Dictionary(
            source.map { ($0.session.id, $0) }, uniquingKeysWith: { a, _ in a }
        )
        ForEach(Array(rows.enumerated()), id: \.element.key) { index, entry in
            if let row = byId[entry.node.session.id] {
                sessionRow(row, guide: guides[index])
            }
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

    @ViewBuilder
    private func sessionRow(_ row: AgentsViewModel.Row, guide: TreeGuide) -> some View {
        let content = SessionListRow(
            session: row.session,
            identifier: sessionRowIdentifier(
                issue: row.issue, session: row.session, batchIssues: row.batchIssues
            ),
            title: sessionRowTitle(
                issue: row.issue, session: row.session, batchIssues: row.batchIssues
            ),
            // EXP-734: a run that opened its own issue-less PR carries the
            // state on its OWN row.
            prState: row.issue?.prState ?? row.session.prState,
            device: row.device,
            deviceIcon: SessionListRow.deviceIcon(row.session, devices: vm.devices),
            guide: guide
        )
        // Every listed row is the caller's own (EXP-312: live sessions are
        // owner-only), so with the relay configured the row jumps straight
        // into the run's Work screen; without it, into the issue's.
        Group {
            if steerEnabled {
                NavigationLink(value: AppRoute.agentSession(accountId: accountId, sessionId: row.session.id)) {
                    content
                }
            } else if let issue = row.issue {
                NavigationLink(value: AppRoute.issue(accountId: accountId, id: issue.id)) {
                    content
                }
            } else {
                content
            }
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("agent-session-row")
    }
}

/// EXP-1248: one run as THE `SessionRow` — the Agent page's Running band,
/// the Recent sheet and an action's Runs, ×4 (web `session-tree.tsx`
/// `sessionRowFacts`). Decides the mark, the caption + tone
/// (`SessionRowCaption`, `list-item.json`) and the dimming; the host decides
/// where a tap goes.
struct SessionListRow: View {
    let session: CodingSessionEntity
    /// An issue run's identifier, a batch's `EXP-874 +2`; nil otherwise.
    let identifier: String?
    let title: String
    /// The PR state the display rule reads (the issue's, else the run's own).
    let prState: String?
    let device: SessionDevicePresentation
    let deviceIcon: String
    var guide: TreeGuide = TreeGuide()
    var size: SessionRow<AnyView>.Size = .big
    /// Pinned by previews; production reads the wall clock on every redraw.
    var now: Date = Date()

    var body: some View {
        let ended = PastRuns.hasEnded(session)
        let state = CodingSessionDisplayState.of(session: session, prState: prState)
        let paused = !ended && device.isPaused(state, status: session.status)
        let working = CodingSessionDisplayState.working(
            status: session.status, state: state, paused: paused, live: true
        )
        let caption = SessionRowCaption.sessionRowCaption(
            ended: ended,
            paused: paused,
            state: state,
            device: device.label,
            startedAt: session.startedAt,
            updatedAt: session.updatedAt,
            endedAt: session.endedAt,
            blockedLabel: ended ? nil : AgentUsagePresentation.blockedBadgeLabel(
                AgentUsagePresentation.parseBlocked(session.blocked), now: now
            ),
            now: now
        )
        SessionRow(
            size: size,
            identifier: identifier,
            title: title,
            caption: caption.text,
            captionTone: caption.tone,
            guide: guide,
            deviceIcon: deviceIcon,
            deviceName: device.label,
            dimmed: paused
        ) {
            AnyView(mark(ended: ended, state: state, paused: paused, working: working))
        }
    }

    @ViewBuilder
    private func mark(
        ended: Bool, state: CodingSessionDisplayState, paused: Bool, working: Bool
    ) -> some View {
        if ended {
            AgentRunMark(agent: session.agent, state: nil, ended: true)
        } else {
            AgentRunMark(
                agent: session.agent,
                state: runningRowMarkState(state, paused: paused, working: working),
                badgeSize: SessionRowLead.badgeSize
            )
        }
    }

    /// The host's glyph: the matched device's pick (`DeviceIconDisplay`), the
    /// generic device glyph when no row matched.
    static func deviceIcon(_ session: CodingSessionEntity, devices: [SteerDevice]?) -> String {
        if let id = session.deviceId, let device = devices?.first(where: { $0.deviceId == id }) {
            return DeviceIconDisplay.iconName(for: device)
        }
        return DeviceIconDisplay.iconName(icon: nil, kind: nil)
    }
}
