import ExpCore
import ExpUI
import SwiftUI

/// EXP-1291: the THREADS tab (it replaced Reviews) — the team agents on top,
/// then the thread roots Needs you · Running · Recent across EVERY member
/// team (`threads-list.json`; phones band the rows per team once the caller
/// is in several). The styleguide list language: a filled group band, no
/// count, over flat gapless rows, no dividers (EXP-1320). EXP-1358: the
/// title sits in the sticky inline nav bar like every other tab root; in
/// several teams each TEAM band (avatar + name) is OUTER and the group bands
/// sit inside it. A row opens the thread, an agent row its channel (the same
/// thread screen).
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
                    clis: lists.clis,
                    words: lists.words,
                    devices: lists.devices,
                    teams: teamState.teams,
                    onOpen: { rootId in
                        pushRoute(.thread(accountId: accountId, threadId: rootId))
                    },
                    // EXP-1323: a nested child row opens that session.
                    onOpenSession: { sessionId in
                        pushRoute(.agentSession(accountId: accountId, sessionId: sessionId))
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
        // EXP-1358: the same sticky inline bar as every other iOS tab root
        // (Inbox, Devices, Actions) — the large in-content title EXP-1318
        // borrowed from Android is Android's root style, not this app's.
        .navigationTitle(SessionTree.ListCopy.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
    }
}

/// The bare list over plain values (web `ThreadsListView`).
struct ThreadsListContent: View {
    let groups: [ThreadsListGroup]
    let agents: [AgentRow]
    /// A root's agent CLI (the row's run mark), by root id.
    var clis: [String: String] = [:]
    /// EXP-1309: a root's quiet word, by root id.
    var words: [String: ThreadRowWord] = [:]
    /// EXP-1316: a root's device glyph, by root id.
    var devices: [String: ThreadRowDevice] = [:]
    let teams: [TeamEntity]
    let onOpen: (String) -> Void
    /// EXP-1323: a nested child row (depth > 0) opens its session view.
    var onOpenSession: (String) -> Void = { _ in }

    var body: some View {
        if groups.isEmpty && agents.isEmpty {
            EmptyState(.threads)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                // EXP-1358: ONE lazy item per row. A group used to be one
                // `VStack` child of the lazy stack, so a band scrolling into
                // view built and laid out every row it holds in one frame
                // (Recent has no cap); the 12pt group gap is the band's top
                // padding now.
                LazyVStack(alignment: .leading, spacing: 0) {
                    if !agents.isEmpty { agentsBand }
                    if TeamGroups.isMultiTeam(teams) {
                        ForEach(teamBands) { band in
                            GlassSectionBand(band.team.name) {
                                TeamAvatar(team: band.team, size: 16)
                            } trailing: {
                                EmptyView()
                            }
                            .padding(.top, 12)
                            .accessibilityIdentifier("threads-team-header-\(band.team.id)")
                            ForEach(band.items) { group in
                                groupRows(group, teamId: band.team.id, gap: 8)
                            }
                        }
                    } else {
                        ForEach(groups) { group in groupRows(group, teamId: nil, gap: 12) }
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

    /// Every team (THE team order) holding a thread, its groups narrowed to
    /// that team's rows (empty groups dropped).
    private var teamBands: [TeamGroups.Group<ThreadsListGroup>] {
        TeamGroups.ordered(teams).compactMap { team in
            let own = groups.compactMap { group -> ThreadsListGroup? in
                let rows = group.rows.filter { $0.teamId == team.id }
                return rows.isEmpty ? nil : ThreadsListGroup(key: group.key, title: group.title, rows: rows)
            }
            return own.isEmpty ? nil : TeamGroups.Group(team: team, items: own)
        }
    }

    /// A group band (its glyph, no count) over its flat rows — emitted as
    /// SIBLINGS of the lazy stack, `gap` above the band.
    @ViewBuilder
    private func groupRows(_ group: ThreadsListGroup, teamId: String?, gap: CGFloat) -> some View {
        GlassSectionBand(group.title) {
            Self.bandGlyph(group.key)
        } trailing: {
            EmptyView()
        }
        .padding(.top, gap)
        .accessibilityIdentifier(
            teamId.map { "threads-group-\(group.key.rawValue)-\($0)" } ?? "threads-group-\(group.key.rawValue)"
        )
        // EXP-1323: a live tree nests its children under the root —
        // SessionRow depth + THE tree guides.
        let guides = TreeGuides.compute(depths: group.rows.map(\.depth))
        ForEach(Array(group.rows.enumerated()), id: \.element.id) { index, row in
            threadRow(row, guide: guides[index])
        }
    }

    @ViewBuilder
    private var agentsBand: some View {
        GlassSectionBand(SessionTree.ListCopy.agents) {
            AppIcon(AppIcons.settingsAgents, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        } trailing: {
            EmptyView()
        }
        .accessibilityIdentifier("threads-group-agents")
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

    private func threadRow(_ row: ThreadsListRow, guide: TreeGuide) -> some View {
        Button {
            if row.depth > 0 {
                onOpenSession(row.sessionId)
            } else {
                onOpen(row.sessionId)
            }
        } label: {
            ThreadRowLabel(
                row: row,
                cli: clis[row.sessionId],
                word: .some(words[row.sessionId]),
                device: devices[row.sessionId],
                guide: guide
            )
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier(SessionTree.ListCopy.rowTestId)
    }

    /// EXP-1318: the group band glyphs ×4 — Needs you amber, Running the
    /// working mark, Recent a check.
    @ViewBuilder
    static func bandGlyph(_ key: ThreadsListGroup.Key) -> some View {
        switch key {
        case .needsYou:
            AppIcon(AppIcons.codingNeedsInput, size: 13)
                .foregroundStyle(DesignTokens.Semantic.yellow)
        case .running:
            AgentWorkingMark(agent: nil)
                .frame(width: 13, height: 13)
        case .recent:
            AppIcon(AppIcons.uiCheck, size: 13)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
        }
    }
}

/// One thread as THE session row: the root agent's run mark (Claude's spark
/// while it works, EXP-1312: amber-badged when it needs you), the title (no
/// identifier slot, EXP-1318: the title names it), the spinner /
/// amber dot — no caption
/// (`threads-list.json`; Home's Running band). EXP-1309: on the Threads tab
/// (`word` given, web `ThreadsListView`) the row ends on ONE quiet word
/// instead (EXP-1339: no lock) and its state moves into the lead: the working mark while busy,
/// the mark with an amber badge when it needs you, the PR glyph `ready to
/// merge` names, the violet pr-merged glyph for `merged` (no word).
struct ThreadRowLabel: View {
    let row: ThreadsListRow
    /// The root's agent CLI; nil/empty = claude.
    var cli: String? = nil
    /// nil = Home's row (spinner / dot); `.some` = the Threads tab's row.
    var word: ThreadRowWord?? = nil
    /// EXP-1316: the root's device glyph, last on the row (after the
    /// word); nil = none.
    var device: ThreadRowDevice? = nil
    /// EXP-1323: the row's tree connector (a nested child's depth).
    var guide = TreeGuide()

    var body: some View {
        if let word {
            SessionRow(
                title: row.title,
                guide: guide,
                unread: row.unread,
                caption: row.caption,
                // EXP-1339: `merged` has no text, only the violet lead.
                word: word?.text,
                wordIcon: word == .stack ? AppIcons.prStack : nil,
                deviceIcon: device?.icon,
                deviceName: device?.name
            ) {
                threadsLead(word)
            }
        } else {
            SessionRow(
                title: row.title,
                guide: guide,
                busy: row.busy,
                needsYou: row.needsYou,
                unread: row.unread,
                deviceIcon: device?.icon,
                deviceName: device?.name
            ) {
                // EXP-1312: the ×4 rule (`threadRowMarkState`, web
                // `ThreadRunMark`, Android `AgentRunMark(state = …)`).
                switch SessionTree.threadRowMarkState(needsYou: row.needsYou, busy: row.busy) {
                case .needsYou: needsYouMark
                case .working: AgentWorkingMark(agent: cli)
                default: BeatingAgentMark(agent: cli, beating: false)
                }
            }
        }
    }

    /// The brand mark with the amber needs-you badge on its top-trailing corner.
    private var needsYouMark: some View {
        BeatingAgentMark(agent: cli, beating: false)
            .overlay(alignment: .topTrailing) {
                Circle()
                    .fill(DesignTokens.Semantic.yellow)
                    .frame(width: 6, height: 6)
                    .offset(x: 2, y: -2)
                    .accessibilityLabel("Needs you")
            }
    }

    /// Web `rowLead`. EXP-1344: `threadRowMarkState` FIRST at every depth
    /// (needs-you's amber badge beats busy: a parked plan/question keeps
    /// the device busy), then the PR glyphs.
    @ViewBuilder
    private func threadsLead(_ word: ThreadRowWord?) -> some View {
        let mark = SessionTree.threadRowMarkState(needsYou: row.needsYou, busy: row.busy)
        if mark == .needsYou {
            needsYouMark
        } else if mark == .working {
            AgentWorkingMark(agent: cli)
        } else if word == .readyToMerge {
            AppIcon(AppIcons.prOpen, size: 13)
                .foregroundStyle(DesignTokens.Semantic.green)
        } else if word == .merged {
            AppIcon(AppIcons.prMerged, size: 13)
                .foregroundStyle(DesignTokens.Avatar.violet)
        } else {
            // No word = a finished thread: the mark dimmed (web `ended`,
            // EXP-1208's 0.5 ×4).
            BeatingAgentMark(agent: cli, beating: false)
                .opacity(word == nil ? 0.5 : 1)
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
