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

/// EXP-1284: the Agent page's thread bands (draft 3): `Running` = the
/// threads holding a live run of mine (every member team, one band per team
/// once the caller is in several, like the run list it replaces), `Recent` =
/// the active team's other threads, newest activity first, capped at
/// `HomeGreeting.recentCount` (no band when empty). A row opens the thread.
struct AgentThreadLists: View {
    let model: ThreadListModel
    let onOpen: (String) -> Void

    @Environment(TeamState.self) private var teamState

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            running
            recent
        }
    }

    @ViewBuilder
    private var running: some View {
        let groups = TeamGroups.isMultiTeam(teamState.teams)
            ? TeamGroups.group(model.running, teams: teamState.teams) { $0.teamId }
            : []
        if groups.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand("Running")
                if model.running.isEmpty {
                    EmptyState(.runningAgents)
                } else {
                    ForEach(model.running) { entry in row(entry) }
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
                    ForEach(group.items) { entry in row(entry) }
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("agent-threads-team-\(group.team.id)")
            }
        }
    }

    @ViewBuilder
    private var recent: some View {
        let rows = Array(model.recent.prefix(HomeGreeting.recentCount))
        if !rows.isEmpty {
            VStack(alignment: .leading, spacing: 0) {
                GlassSectionBand(HomeGreeting.recentLabel)
                ForEach(rows) { entry in row(entry) }
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(HomeGreeting.TestId.recent)
        }
    }

    private func row(_ entry: ThreadListModel.Entry) -> some View {
        Button {
            onOpen(entry.threadId)
        } label: {
            ThreadListRow(view: entry.view, liveAgent: entry.rows.liveAgent)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("agent-thread-row")
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
