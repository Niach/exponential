import ExpCore
import ExpUI
import SwiftUI

/// EXP-1291: the THREADS tab (it replaced Reviews) — the team agents on top,
/// then the thread roots Needs you · Running · Recent across EVERY member
/// team (`threads-list.json`; phones band the rows per team once the caller
/// is in several). The styleguide list language: a filled group band, no
/// count, over flat hairline rows. A row opens the thread, an agent row its
/// channel (the same thread screen).
struct ThreadsView: View {
    @Environment(SessionListsModel.self) private var lists
    @Environment(TeamState.self) private var teamState
    @Environment(\.accountId) private var accountId
    @Environment(\.pushRoute) private var pushRoute

    var body: some View {
        ZStack {
            AppBackground()
            if lists.loaded {
                ThreadsListContent(
                    groups: lists.groups,
                    agents: lists.agents,
                    identifiers: lists.identifiers,
                    clis: lists.clis,
                    words: lists.words,
                    teams: teamState.teams,
                    onOpen: { rootId in
                        pushRoute(.thread(accountId: accountId, threadId: rootId))
                    }
                )
            } else {
                // EXP-1275 (D8): the first load is skeleton rows.
                SkeletonRows()
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .navigationTitle(SessionTree.ListCopy.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }
}

/// The bare list over plain values (web `ThreadsListView`).
struct ThreadsListContent: View {
    let groups: [ThreadsListGroup]
    let agents: [AgentRow]
    /// A thread's first linked issue identifier, by root id.
    let identifiers: [String: String]
    /// A root's agent CLI (the row's run mark), by root id.
    var clis: [String: String] = [:]
    /// EXP-1309: a root's quiet word, by root id.
    var words: [String: ThreadRowWord] = [:]
    let teams: [TeamEntity]
    let onOpen: (String) -> Void

    var body: some View {
        if groups.isEmpty && agents.isEmpty {
            EmptyState(.threads)
                .frame(maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if !agents.isEmpty { agentsBand }
                    ForEach(groups) { group in
                        VStack(alignment: .leading, spacing: 0) {
                            GlassSectionBand(group.title) {
                                AppIcon(Self.bandIcon(group.key), size: 13)
                                    .foregroundStyle(Self.bandTint(group.key))
                            } trailing: {
                                EmptyView()
                            }
                            TeamBandedRows(rows: group.rows, teams: teams, teamId: \.teamId) { row in
                                threadRow(row)
                            }
                        }
                        .accessibilityElement(children: .contain)
                        .accessibilityIdentifier("threads-group-\(group.key.rawValue)")
                    }
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .padding(.bottom, 16)
            }
            .scrollContentBackground(.hidden)
            .background(Color.clear)
            .tabBarBottomInset()
            .accessibilityIdentifier(SessionTree.ListCopy.listTestId)
        }
    }

    private var agentsBand: some View {
        VStack(alignment: .leading, spacing: 0) {
            GlassSectionBand(SessionTree.ListCopy.agents) {
                AppIcon(AppIcons.settingsAgents, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.secondary))
            } trailing: {
                EmptyView()
            }
            TeamBandedRows(rows: agents, teams: teams, teamId: \.teamId) { agent in
                Button {
                    onOpen(agent.sessionId)
                } label: {
                    // EXP-1309 (web `ThreadsListItem`): the working mark
                    // while it works, `needs input` while it waits on you.
                    SessionRow(
                        title: agent.name,
                        word: agent.needsYou ? ThreadRowWord.needsInput.text : nil
                    ) {
                        if agent.busy {
                            AgentWorkingMark(agent: nil)
                        } else {
                            AgentRowIcon(icon: agent.icon, color: agent.color)
                        }
                    }
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier(SessionTree.ListCopy.agentRowTestId)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("threads-group-agents")
    }

    private func threadRow(_ row: ThreadsListRow) -> some View {
        Button {
            onOpen(row.sessionId)
        } label: {
            ThreadRowLabel(
                row: row,
                identifier: identifiers[row.sessionId],
                cli: clis[row.sessionId],
                word: .some(words[row.sessionId])
            )
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(SessionTree.ListCopy.rowTestId)
    }

    static func bandIcon(_ key: ThreadsListGroup.Key) -> String {
        switch key {
        case .needsYou: AppIcons.codingNeedsInput
        case .running: AppIcons.codingRunning
        case .recent: AppIcons.codingEnded
        }
    }

    static func bandTint(_ key: ThreadsListGroup.Key) -> Color {
        key == .needsYou ? DesignTokens.Semantic.yellow : .white.opacity(TextOpacity.secondary)
    }
}

/// One thread as THE session row: the root agent's run mark (Claude's
/// spark while it works), the first linked issue's id, the title, the lock /
/// spinner / amber dot — no caption (`threads-list.json`; Home's Running
/// band). EXP-1309: on the Threads tab (`word` given, web
/// `ThreadsListView`) the row ends on ONE quiet word instead and its state
/// moves into the lead: the working mark while busy, the mark with an amber
/// badge when it needs you, the PR glyph `ready to merge` / `merged` name.
struct ThreadRowLabel: View {
    let row: ThreadsListRow
    let identifier: String?
    /// The root's agent CLI; nil/empty = claude.
    var cli: String? = nil
    /// nil = Home's row (spinner / dot); `.some` = the Threads tab's row.
    var word: ThreadRowWord?? = nil

    var body: some View {
        if let word {
            SessionRow(
                identifier: identifier,
                title: row.title,
                isPrivate: row.isPrivate,
                word: word?.text,
                wordIcon: word == .stack ? AppIcons.prStack : nil
            ) {
                threadsLead(word)
            }
        } else {
            SessionRow(
                identifier: identifier,
                title: row.title,
                isPrivate: row.isPrivate,
                busy: row.busy,
                needsYou: row.needsYou
            ) {
                if row.busy {
                    AgentWorkingMark(agent: cli)
                } else {
                    BeatingAgentMark(agent: cli, beating: false)
                }
            }
        }
    }

    /// Web `rowLead`.
    @ViewBuilder
    private func threadsLead(_ word: ThreadRowWord?) -> some View {
        if row.busy {
            AgentWorkingMark(agent: cli)
        } else if row.needsYou {
            BeatingAgentMark(agent: cli, beating: false)
                .overlay(alignment: .topTrailing) {
                    Circle()
                        .fill(DesignTokens.Semantic.yellow)
                        .frame(width: 6, height: 6)
                        .offset(x: 2, y: -2)
                        .accessibilityLabel("Needs you")
                }
        } else if word == .readyToMerge {
            AppIcon(AppIcons.prOpen, size: 13)
                .foregroundStyle(DesignTokens.Semantic.green)
        } else if word == .merged {
            AppIcon(AppIcons.prMerged, size: 13)
                .foregroundStyle(DesignTokens.Avatar.violet)
        } else {
            BeatingAgentMark(agent: cli, beating: false)
        }
    }
}

/// An agent's lead: its curated icon in its colour, like a board row
/// (`BoardTypeDisplay`; an unknown name falls back to the default `bot`).
struct AgentRowIcon: View {
    let icon: String
    let color: String

    var body: some View {
        AppIcon(
            BoardTypeDisplay.iconName(for: icon) ?? SessionTree.ListCopy.agentDefaultIcon,
            size: AppIcon.Size.small
        )
        .foregroundStyle(
            Color(hex: color) ?? Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .white
        )
    }
}

/// Rows of EVERY team, one small team header (avatar + name) before each
/// team's rows once the caller is in several (EXP-1186); in one team the
/// rows are flat. Order within a team is the given order.
struct TeamBandedRows<Item: Identifiable, Row: View>: View {
    let rows: [Item]
    let teams: [TeamEntity]
    let teamId: KeyPath<Item, String>
    @ViewBuilder let row: (Item) -> Row

    var body: some View {
        if TeamGroups.isMultiTeam(teams) {
            ForEach(TeamGroups.group(rows, teams: teams) { $0[keyPath: teamId] }) { group in
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
                .accessibilityIdentifier("threads-team-header-\(group.team.id)")
                ForEach(group.items) { item in row(item) }
            }
        } else {
            ForEach(rows) { item in row(item) }
        }
    }
}
