import ExpCore
import ExpUI
import SwiftUI

/// EXP-1284: one THREAD in a list (brief §5: lists show threads, not bare
/// runs) — THE session row (`SessionRow`, big) with the thread's title and
/// its `listCaption` (`2 issues · 1 run · macbook`). The lead is the newest
/// live run's mark — the working spinner while its agent works (`busy`),
/// amber while it waits on a person (`needsInput`), as today's run rows —
/// and the chat glyph for a thread with nothing running.
struct ThreadListRow: View {
    let view: ThreadViewModel
    /// The newest live run's agent (nil = claude / nothing live).
    var liveAgent: String? = nil
    var size: SessionRow<AnyView>.Size = .big

    var body: some View {
        SessionRow(
            size: size,
            title: view.title,
            caption: view.listCaption,
            captionTone: view.needsInput ? .amber : .muted
        ) {
            AnyView(lead)
        }
    }

    @ViewBuilder
    private var lead: some View {
        if view.live {
            AgentRunMark(
                agent: liveAgent,
                state: view.needsInput ? .needsInput : (view.busy ? .working : nil),
                badgeSize: SessionRowLead.badgeSize
            )
        } else {
            AppIcon(AppIcons.actionChat, size: AppIcon.Size.small)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }
}

/// EXP-1284: the Agent page's RUNNING threads (draft 3; review T4/T6) —
/// every member team, banded per team once the caller is in several: the
/// threads holding a live session of mine plus (T4) my live sessions no
/// thread item names yet, as plain session rows (opening the session's
/// thread once it lands). No Recent band under the composer (Danny
/// 2026-10-12): recent threads live in the toolbar's history sheet
/// (`RecentThreadsSheet`). A thread row opens the thread.
struct AgentThreadLists: View {
    let model: ThreadListModel
    let onOpen: (String) -> Void
    /// An orphan session row (T4): its own thread lookup, Work fallback.
    let onOpenSession: (String) -> Void

    @Environment(TeamState.self) private var teamState

    var body: some View {
        running
    }

    private var multiTeam: Bool { TeamGroups.isMultiTeam(teamState.teams) }

    @ViewBuilder
    private var running: some View {
        let rows = ThreadRunningRow.rows(model)
        let groups = multiTeam ? TeamGroups.group(rows, teams: teamState.teams) { $0.teamId } : []
        if groups.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Running")
                if rows.isEmpty {
                    EmptyState(.runningAgents)
                } else {
                    ForEach(rows) { row in runningRow(row) }
                }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("agent-threads-running")
        } else {
            ForEach(groups) { group in
                VStack(alignment: .leading, spacing: 0) {
                    GlassSectionBand(group.team.name) {
                        TeamAvatar(team: group.team, size: 16)
                    } trailing: {
                        EmptyView()
                    }
                    ForEach(group.items) { row in runningRow(row) }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("agent-threads-team-\(group.team.id)")
            }
        }
    }

    @ViewBuilder
    private func runningRow(_ row: ThreadRunningRow) -> some View {
        switch row {
        case let .thread(entry):
            ThreadEntryButton(entry: entry, onOpen: onOpen)
        case let .orphan(orphan):
            Button {
                onOpenSession(orphan.session.id)
            } label: {
                SessionListRow(
                    session: orphan.session,
                    identifier: sessionRowIdentifier(
                        issue: orphan.issue, session: orphan.session, batchIssues: orphan.batchIssues
                    ),
                    title: sessionRowTitle(
                        issue: orphan.issue, session: orphan.session, batchIssues: orphan.batchIssues
                    ),
                    prState: orphan.issue?.prState ?? orphan.session.prState,
                    device: orphan.device,
                    deviceIcon: orphan.deviceIcon
                )
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("agent-session-row")
        }
    }
}

/// T4: one Running row — a thread, or a live session of mine no thread
/// item names yet. Orphans first (they are the starts still landing), then
/// the threads, newest activity first.
enum ThreadRunningRow: Identifiable {
    case thread(ThreadListModel.Entry)
    case orphan(ThreadListModel.Orphan)

    var id: String {
        switch self {
        case let .thread(entry): "thread:\(entry.id)"
        case let .orphan(orphan): "session:\(orphan.id)"
        }
    }

    var teamId: String {
        switch self {
        case let .thread(entry): entry.teamId
        case let .orphan(orphan): orphan.teamId
        }
    }

    @MainActor
    static func rows(_ model: ThreadListModel) -> [ThreadRunningRow] {
        model.orphans.map { .orphan($0) } + model.running.map { .thread($0) }
    }
}

/// One thread row as a button opening the thread.
struct ThreadEntryButton: View {
    let entry: ThreadListModel.Entry
    let onOpen: (String) -> Void

    var body: some View {
        Button {
            onOpen(entry.threadId)
        } label: {
            ThreadListRow(view: entry.view, liveAgent: entry.liveAgent)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("agent-thread-row")
    }
}

/// T6: thread rows of EVERY team, one small team header (avatar + name)
/// before each team's rows once the caller is in several; in one team the
/// rows are flat. Order within a team is the given (activity) order.
struct ThreadTeamBandedRows<Row: View>: View {
    let entries: [ThreadListModel.Entry]
    let multiTeam: Bool
    let teams: [TeamEntity]
    @ViewBuilder let row: (ThreadListModel.Entry) -> Row

    var body: some View {
        if multiTeam {
            let groups = TeamGroups.group(entries, teams: teams) { $0.teamId }
            ForEach(groups) { group in
                HStack(spacing: 6) {
                    TeamAvatar(team: group.team, size: 14)
                    Text(group.team.name)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 12)
                .padding(.top, 10)
                .padding(.bottom, 4)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("thread-team-header-\(group.team.id)")
                ForEach(group.items) { entry in row(entry) }
            }
        } else {
            ForEach(entries) { entry in row(entry) }
        }
    }
}

/// T-N1: the Agent page's history sheet lists THREADS (Android's
/// `RecentThreadsSheet`): every Recent thread of mine across teams, banded
/// per team; a row closes the sheet and opens the thread.
struct RecentThreadsSheet: View {
    let model: ThreadListModel
    let onOpen: (String) -> Void

    @Environment(TeamState.self) private var teamState

    var body: some View {
        GlassSheetChrome(title: ThreadViews.recentLabel) {
            VStack(alignment: .leading, spacing: 0) {
                if !model.recentLoaded {
                    // EXP-1288: Recent loads as the sheet opens.
                    SkeletonRows()
                        .padding(.vertical, 8)
                } else if model.recent.isEmpty {
                    EmptyState(.recentRuns)
                        .padding(.vertical, 24)
                } else {
                    ThreadTeamBandedRows(
                        entries: model.recent,
                        multiTeam: TeamGroups.isMultiTeam(teamState.teams),
                        teams: teamState.teams
                    ) { entry in
                        ThreadEntryButton(entry: entry, onOpen: onOpen)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("recent-threads-sheet")
    }
}

/// EXP-1284: the issue page's `Threads` row (brief §6, draft 3's "on an issue
/// page"): one chip per thread holding the issue — the live run mark while
/// one of its runs is live, else the chat glyph — each opening its thread.
/// The host (`IssueFaceView`) owns the observation (`IssueThreadsModel`) and
/// mounts this only while the issue sits in a thread.
struct IssueThreadsRow: View {
    let threads: [IssueThreadsModel.Item]

    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Text(ThreadViews.threadsLabel)
                .font(.footnote)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                .fixedSize()
            ScrollView(.horizontal) {
                HStack(spacing: 6) {
                    ForEach(threads) { thread in
                        ThreadChip(
                            title: thread.view.title,
                            live: thread.view.live,
                            busy: thread.view.busy,
                            needsInput: thread.view.needsInput,
                            agent: thread.rows.liveAgent
                        ) {
                            pushRoute(.thread(accountId: accountId, threadId: thread.rows.thread.id))
                        }
                    }
                }
            }
            .scrollIndicators(.hidden)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("issue-threads-row")
    }
}

/// One thread as a chip: the mark (live) or the chat glyph, then its title.
struct ThreadChip: View {
    let title: String
    let live: Bool
    var busy = false
    var needsInput = false
    var agent: String?
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 6) {
                Group {
                    if live {
                        AgentRunMark(
                            agent: agent,
                            state: needsInput ? .needsInput : (busy ? .working : nil),
                            badgeSize: 5
                        )
                    } else {
                        AppIcon(AppIcons.actionChat, size: 11)
                            .foregroundStyle(.white.opacity(TextOpacity.secondary))
                    }
                }
                .frame(width: 12, height: 12)
                Text(title)
                    .font(.footnote)
                    .foregroundStyle(.white.opacity(TextOpacity.primary))
                    .lineLimit(1)
                    .frame(maxWidth: IssueChipTokens.titleMaxWidth, alignment: .leading)
            }
            .padding(.horizontal, IssueChipTokens.horizontalPadding + 2)
            .padding(.vertical, IssueChipTokens.verticalPadding + 1)
            .background(
                RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                    .fill(Color(MarkdownStyle.chipBackground))
            )
            .overlay(
                RoundedRectangle(cornerRadius: MarkdownStyle.chipCornerRadius, style: .continuous)
                    .strokeBorder(Color(MarkdownStyle.chipBorder), lineWidth: IssueChipTokens.borderWidth)
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Thread \(title)")
        .accessibilityIdentifier("thread-chip")
    }
}
