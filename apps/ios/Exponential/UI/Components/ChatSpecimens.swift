import ExpCore
import ExpUI
import SwiftUI

// Session 58cb32df (chats.json): the iOS styleguide's chat entries (web
// `apps/styleguide` entries `chat-list`, `phone-tab-bar`, `chat-transcript`,
// `chat-menus`, `chat-selection`, the updated `schedule-picker`): the REAL views over specimen
// values. Capture only: `-uiTestingStyleguide <id>` (`ChatCardsScreen`).

/// `chat-list`: THE phone Chats screen over the bottom bar — the large
/// "Chats" headline with the search + inbox circles, the Agents and (#EXP-1733)
/// Apps sections, then
/// the rows under their section headers (#FEED-191; the web entry's and
/// Android's `ChatListSpecimenScreen` composition and data: the draft's list).
struct ChatListSpecimens: View {
    private struct Row: Identifiable {
        let id: String
        let title: String
        let group: String
        let lead: Chats.ChatLead
        let previewKind: Chats.ChatPreviewKind
        let preview: String
        let time: String
        var unread = 0
        var needsYou = false
        var word: ChatRowWord?
        /// S-3: a DONE chat stays, dimmed (`listCases[0]` `dim`).
        var dim = false
    }

    // chats.json `listCases[0].expected` (the model every platform's
    // specimen paints; `ChatsTests` locks the Swift `chatList` to it).
    private static let rows = [
        Row(id: "b", title: "Running one", group: "running", lead: .working,
            previewKind: .working, preview: Chats.Copy.Preview.working, time: "09:25"),
        Row(id: "a", title: "Needs you one", group: "needs_you", lead: .needsYou,
            previewKind: .question, preview: "Question: Snapshot?", time: "09:10", needsYou: true),
        Row(id: "c", title: "Open PR", group: "recent", lead: .open,
            previewKind: .agent, preview: "Waiting for review", time: "08:30",
            unread: 1, word: .readyToMerge),
        Row(id: "d", title: "Merged", group: "recent", lead: .merged,
            previewKind: .you, preview: "You: ship it", time: Chats.Copy.Time.yesterday, dim: true),
    ]

    /// S-3 chats.json `agentRowCases[0].expected`: the pinned agent rows,
    /// the Assistant first (the lead disc always holds the avatar; busy reads
    /// only in the preview).
    private static let agentRows = [
        Chats.AgentChatRow(
            agentId: "as", sessionId: "c-as", name: "Assistant", icon: "bot", color: "#6366f1",
            assistant: true, preview: Chats.ChatPreview(kind: .agent, text: "All chats are on track."),
            time: "09:00", activityAt: "2026-10-23T09:00:00.000Z", unreadCount: 1, needsYou: false, busy: false
        ),
        Chats.AgentChatRow(
            agentId: "ag1", sessionId: "c-ag1", name: "Triage", icon: "bug", color: "#a855f7",
            assistant: false, preview: Chats.ChatPreview(kind: .working, text: Chats.Copy.Preview.working),
            time: "08:00", activityAt: "2026-10-23T08:00:00.000Z", unreadCount: 0, needsYou: false, busy: true
        ),
        Chats.AgentChatRow(
            agentId: "ag2", sessionId: "c-ag2", name: "Designer", icon: "bot", color: "#6366f1",
            assistant: false, preview: Chats.ChatPreview(kind: .question, text: "Question: Which palette?"),
            time: "09:30", activityAt: "2026-10-23T09:30:00.000Z", unreadCount: 1, needsYou: true, busy: false
        ),
    ]

    /// #EXP-1733 chats.json `appRowCases[0].expected`: the Apps section,
    /// by name (Budget has no session yet).
    private static let appRows = [
        Chats.AppChatRow(
            appId: "ap1", slug: "budget", sessionId: nil, name: "Budget", icon: nil, color: nil,
            preview: Chats.ChatPreview(kind: .empty, text: Chats.Copy.Preview.empty),
            time: "", activityAt: nil, unreadCount: 0, needsYou: false, busy: false
        ),
        Chats.AppChatRow(
            appId: "ap3", slug: "triage-board", sessionId: "e-ap3", name: "Triage board", icon: "bug",
            color: "#a855f7", preview: Chats.ChatPreview(kind: .question, text: "Question: Which columns?"),
            time: "09:40", activityAt: "2026-10-23T09:40:00.000Z", unreadCount: 1, needsYou: true, busy: false
        ),
        Chats.AppChatRow(
            appId: "ap2", slug: "working-hours", sessionId: "e-ap2", name: "working hours", icon: "clock",
            color: "#22c55e", preview: Chats.ChatPreview(kind: .working, text: Chats.Copy.Preview.working),
            time: "09:30", activityAt: "2026-10-23T09:30:00.000Z", unreadCount: 1, needsYou: false, busy: true
        ),
    ]

    private static let team = TeamEntity(
        id: "t1", name: "Acme", slug: "acme", iconUrl: nil, createdAt: "", updatedAt: ""
    )

    /// #EXP-1388 `chat-selection`: the selected ids (nil = not selecting):
    /// the Archived entry first, a selected row tinted full width with the
    /// check badge on its disc (nothing shifts) and
    /// THE BAR stands in the tab bar's slot.
    var selection: [String]?

    /// The specimen's archived count (the Archived entry's trailing slot).
    private static let archived = 2

    /// The selected rows as the bar reads them: `b` live (working),
    /// `c` ended with one unread, both the viewer's.
    private static func selectionRow(_ id: String) -> Chats.ChatSelectionRow {
        let row = rows.first { $0.id == id }
        return Chats.ChatSelectionRow(
            sessionId: id,
            rootStatus: row?.group == "recent" ? "ended" : "running",
            unreadCount: row?.unread ?? 0,
            canSteer: true,
            canDelete: true
        )
    }

    /// #FEED-191 `chatSections` over `listCases[0]`, in order.
    private static let sections: [(key: Chats.ChatSectionKey, rows: [Row])] = {
        func key(_ row: Row) -> Chats.ChatSectionKey {
            if row.group == "needs_you" { return .needsYou }
            if row.group == "running" { return .working }
            return row.unread > 0 ? .unread : .chats
        }
        return Chats.ChatSectionKey.allCases.compactMap { section in
            let own = rows.filter { key($0) == section }
            return own.isEmpty ? nil : (section, own)
        }
    }()

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        // EXP-1553: the Agents section STAYS while selecting
                        // (agents are never selectable), as in `ChatsView`.
                        ChatSectionHeader(Chats.Copy.Sections.agents)
                        ForEach(Self.agentRows) { row in
                            ChatRowView(
                                title: row.name,
                                time: row.time,
                                preview: row.preview.text,
                                previewLabel: row.preview.kind == .question ? "Question:" : nil,
                                previewItalic: row.preview.kind == .working,
                                needsYou: row.needsYou,
                                unreadCount: row.unreadCount
                            ) { AgentChatLead(row: row) }
                        }
                        ChatSectionHeader(Chats.Copy.Sections.apps)
                        ForEach(Self.appRows) { row in
                            ChatRowView(
                                title: row.name,
                                time: row.time,
                                preview: row.preview.text,
                                previewLabel: row.preview.kind == .question ? "Question:" : nil,
                                previewItalic: row.preview.kind == .working,
                                needsYou: row.needsYou,
                                unreadCount: row.unreadCount
                            ) { AgentChatLead(app: row) }
                        }
                        ForEach(Self.sections, id: \.key) { section in
                            ChatSectionHeader(Chats.Copy.Sections.title(section.key))
                            if section.key == .chats, selection != nil {
                                ChatArchivedRow(count: Self.archived)
                            }
                            ForEach(section.rows) { row in
                                ChatRowView(
                                    title: row.title,
                                    time: row.time,
                                    preview: row.preview,
                                    previewLabel: row.previewKind == .question ? "Question:" : nil,
                                    previewItalic: row.previewKind == .working,
                                    needsYou: row.needsYou,
                                    unreadCount: row.unread,
                                    word: row.word?.text,
                                    wordIcon: row.word == .stack ? AppIcons.prStack : nil,
                                    selected: selection?.contains(row.id) == true
                                ) { ChatLeadMark(lead: row.lead) }
                                .opacity(row.dim ? Chats.Shell.dimOpacity : 1)
                                .chatSelectedTint(selection?.contains(row.id) == true)
                            }
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                }
                .tabBarBottomInset()
            }
            .pageHeader(Chats.Copy.Headers.chats)
        }
        .environment(\.inboxUnread, true)
        .overlay(alignment: .bottom) {
            if let selection {
                ChatSelectionBar(
                    count: selection.count,
                    items: Chats.chatSelectionActions(rows: selection.map(Self.selectionRow)),
                    onClose: {},
                    onAction: { _ in }
                )
                .padding(.horizontal, 16)
                .padding(.bottom, 4)
            } else {
                MobileTabBar(
                    selected: .chats, board: PhoneShellSpecimen.activeBoard, team: Self.team,
                    chatsNeedYou: true
                )
            }
        }
        .accessibilityIdentifier(selection == nil ? "chat-list-specimen" : "chat-selection-specimen")
    }
}

/// The phone shell specimens' shared values (the web entries' `BOARDS`).
enum PhoneShellSpecimen {
    static let team = TeamEntity(
        id: "t1", name: "Acme", slug: "acme", iconUrl: nil, createdAt: "", updatedAt: ""
    )

    static let boards = [
        board("app", "Mobile App", "code", "#6366f1", 0),
        board("mkt", "Launch Marketing", "square-kanban", "#f59e0b", 10),
        board("fb", "Product Feedback", "megaphone", "#22c55e", 20),
    ]

    static var activeBoard: BoardEntity { boards[0] }

    private static func board(
        _ id: String, _ name: String, _ icon: String, _ color: String, _ order: Double
    ) -> BoardEntity {
        BoardEntity(
            id: id, teamId: "t1", name: name, slug: id, prefix: id.uppercased(), color: color,
            sortOrder: order, repositoryId: nil, icon: icon, createdAt: "", updatedAt: ""
        )
    }

    static func caption(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            .padding(.horizontal, 16)
    }
}

/// `phone-tab-bar` (#EXP-1384): Board · Chats · Apps · Team beside the
/// launcher — the board view (the caret), Chats selected, the Team page (the
/// caret, #EXP-1645), the lens mid-drag between Chats and Apps; the amber dot
/// on Chats.
struct PhoneTabBarSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            PhoneShellSpecimen.caption("On the board view (the caret opens the board menu)")
            bar(.board, onBoardView: true)
            PhoneShellSpecimen.caption("Chats selected")
            bar(.chats)
            PhoneShellSpecimen.caption("On the Team page (the caret opens the team menu)")
            bar(.team, onTeamPage: true)
            PhoneShellSpecimen.caption("Dragging: the lens between Chats and Apps")
            bar(.chats, specimen: .lens(candidate: .apps, offset: -10))
        }
        .padding(.horizontal, -16)
        .accessibilityIdentifier("phone-tab-bar-specimen")
    }

    private func bar(
        _ selected: Chats.Shell.Tab,
        onBoardView: Bool = false,
        onTeamPage: Bool = false,
        specimen: MobileTabBar.Specimen? = nil
    ) -> some View {
        MobileTabBar(
            selected: selected,
            board: PhoneShellSpecimen.activeBoard,
            onBoardView: onBoardView,
            onTeamPage: onTeamPage,
            team: PhoneShellSpecimen.team,
            chatsNeedYou: true,
            specimen: specimen
        )
    }
}

/// `phone-board-menu` (#EXP-1384): the board view's Board tab morphed the
/// whole strip into the glass board menu, shown OPEN over a dimmed page:
/// the team's boards (the active one lit with the check) then New board.
/// `teamMenu` (#EXP-1645, `phone-team-menu`): the same panel over the member's
/// teams, from the Team page's Team tab.
struct PhoneBoardMenuSpecimens: View {
    var teamMenu = false

    var body: some View {
        ZStack(alignment: .bottom) {
            VStack(alignment: .leading, spacing: 10) {
                // The page header behind the menu, its search + inbox
                // circles included (dimmed with the page).
                HStack(spacing: 8) {
                    Text(teamMenu ? PhoneShellSpecimen.team.name : PhoneShellSpecimen.activeBoard.name)
                        .font(.largeTitle.weight(.bold))
                        .foregroundStyle(.white)
                    Spacer(minLength: 8)
                    ForEach([AppIcons.navSearch, AppIcons.navInbox], id: \.self) { icon in
                        PageHeaderGlyph(icon: icon, dot: false)
                            .frame(width: 44, height: 44)
                            .background(GlassTokens.fillCard, in: Circle())
                    }
                }
                ForEach(0..<3, id: \.self) { _ in
                    RoundedRectangle(cornerRadius: 12)
                        .fill(GlassTokens.fillCard)
                        .frame(height: 44)
                }
                Spacer(minLength: 0)
            }
            .padding(16)
            .opacity(0.4)
            MobileTabBar(
                selected: teamMenu ? .team : .board,
                board: PhoneShellSpecimen.activeBoard,
                onBoardView: !teamMenu,
                onTeamPage: teamMenu,
                team: PhoneShellSpecimen.team,
                boards: PhoneShellSpecimen.boards,
                onNewBoard: {},
                teams: [
                    PhoneShellSpecimen.team,
                    TeamEntity(
                        id: "t2", name: "Launch Studio", slug: "launch-studio", iconUrl: TeamAvatarSpecimens.sampleUrl,
                        createdAt: "", updatedAt: ""
                    ),
                ].map { MobileTabBar.TeamRow(team: $0, accountId: "", server: "exponential.at") },
                onNewTeam: {},
                specimen: teamMenu ? .teamMenuOpen : .menuOpen
            )
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .environment(\.teamImageLoader, TeamAvatarSpecimens.sampleLoader)
        .accessibilityIdentifier("phone-board-menu-specimen")
    }
}

/// `team-avatar` (#EXP-1645, team-image.json): the marks (a transparent image
/// at 28 and 20, the glyph at 28, 20 and 18) and the Settings › General row with
/// and without an image. No border and no fill: the star's transparency shows
/// the page.
struct TeamAvatarSpecimens: View {
    /// Any non-blank `icon_url`; `sampleLoader` answers it with the star.
    static let sampleUrl = "/api/attachments/team-avatar-specimen"

    /// The styleguide entry's transparent vector mark.
    private static let star: UIImage = {
        let format = UIGraphicsImageRendererFormat()
        format.opaque = false
        return UIGraphicsImageRenderer(size: CGSize(width: 96, height: 96), format: format).image { context in
            let scale: CGFloat = 4
            let points: [(CGFloat, CGFloat)] = [
                (12, 2), (15, 9), (22, 9), (16.5, 13.5), (18.5, 21), (12, 16.5), (5.5, 21), (7.5, 13.5), (2, 9), (9, 9),
            ]
            let path = UIBezierPath()
            for (index, point) in points.enumerated() {
                let at = CGPoint(x: point.0 * scale, y: point.1 * scale)
                if index == 0 { path.move(to: at) } else { path.addLine(to: at) }
            }
            path.close()
            UIColor(red: 0xF9 / 255, green: 0x73 / 255, blue: 0x16 / 255, alpha: 1).setFill()
            context.cgContext.addPath(path.cgPath)
            context.cgContext.fillPath()
        }
    }()

    static let sampleLoader = TeamImageLoader(cached: { _, _ in star }, load: { _, _ in star })

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                TeamAvatar(iconUrl: Self.sampleUrl, size: 28)
                TeamAvatar(iconUrl: Self.sampleUrl, size: 20)
                TeamAvatar(iconUrl: nil, size: 28)
                TeamAvatar(iconUrl: nil, size: 20)
                TeamAvatar(iconUrl: nil, size: 18)
            }
            .foregroundStyle(.white)
            VStack(spacing: 0) {
                TeamImageRow(iconUrl: Self.sampleUrl)
                Rectangle()
                    .fill(GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
                TeamImageRow(iconUrl: nil)
            }
            .glassSection()
        }
        .environment(\.teamImageLoader, Self.sampleLoader)
        .accessibilityIdentifier("team-avatar-specimen")
    }
}

/// `phone-page-header` (#EXP-1384): THE page header — the large title and
/// the native search + inbox circles, the inbox's sky dot on and off;
/// Devices (pushed from the Team page) with the back control.
struct PhonePageHeaderSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            PhoneShellSpecimen.caption("Unread notification (the inbox dot)")
            header(Chats.Copy.Headers.chats, unread: true)
            PhoneShellSpecimen.caption("All read")
            header(PhoneShellSpecimen.activeBoard.name, unread: false)
            PhoneShellSpecimen.caption("Pushed (Devices, the back control)")
            header(Chats.Copy.Headers.devices, unread: false, pushed: true)
        }
        .padding(.horizontal, -16)
        .accessibilityIdentifier("phone-page-header-specimen")
    }

    /// EXP-1647: each header sits straight on the page (web-mobile/Android
    /// draw no band): a bar-tall frame, no clip, the stack's own background
    /// cleared.
    private func header(_ title: String, unread: Bool, pushed: Bool = false) -> some View {
        PageHeaderFrame(title: title, pushed: pushed)
            .environment(\.inboxUnread, unread)
            .frame(height: 64)
    }
}

/// One page header in its own navigation stack (pushed = one page deep).
private struct PageHeaderFrame: View {
    let title: String
    let pushed: Bool
    @State private var path: [Int] = []

    var body: some View {
        NavigationStack(path: $path) {
            page(pushed ? "Team" : title, pushed: false)
                .navigationDestination(for: Int.self) { _ in page(title, pushed: true).bareBackButton() }
        }
        .onAppear { if pushed { path = [1] } }
    }

    private func page(_ title: String, pushed: Bool) -> some View {
        Color.clear
            .modifier(ClearNavigationBackground())
            .pageHeader(title, pushed: pushed)
    }
}

/// The specimen's stacks paint nothing of their own: the styleguide page's
/// background shows through (no band per header).
private struct ClearNavigationBackground: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 18.0, *) {
            content
                .containerBackground(.clear, for: .navigation)
                .toolbarBackgroundVisibility(.hidden, for: .navigationBar)
        } else {
            content.toolbarBackground(.hidden, for: .navigationBar)
        }
    }
}

/// `chat-header` (#EXP-1688, chats.json `shell.chatHeader`): THE chat header:
/// a chat waiting on you (#EXP-1732: THE tabs under it), a working one, an
/// agent chat with its telephone and THE gear.
struct ChatHeaderSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            PhoneShellSpecimen.caption(
                "A chat that needs you (\(Int(Chats.Shell.ChatHeader.row))px, ghost buttons, always leading)"
            )
            ChatHeader {
                ChatHeaderTitle(
                    title: "Reduce cold start",
                    subtitle: SessionScreen.subtitle(status: SessionScreen.Copy.Status.needsInput, device: "Alex's MacBook", trigger: nil),
                    subtitleTone: .needsYou,
                    cli: "claude",
                    mark: .needsYou
                )
            } trailing: {
                more
            }
            SessionScreenTabs(
                tabs: SessionScreen.screen(
                    kind: .session, requested: .chat, open: nil,
                    counts: SessionScreen.Counts(issues: 1, sessions: 6, results: 4)
                ).tabs,
                selection: .chat
            ) { _ in }
            PhoneShellSpecimen.caption("A working chat")
            ChatHeader {
                ChatHeaderTitle(
                    title: "A title long enough that it truncates on one line",
                    subtitle: SessionScreen.Copy.Status.working,
                    subtitleTone: .working,
                    cli: "claude",
                    mark: .working
                )
            } trailing: {
                more
            }
            PhoneShellSpecimen.caption("An agent chat (the telephone)")
            ChatHeader {
                ChatHeaderTitle(
                    title: "Triage",
                    subtitle: SessionScreen.subtitle(status: SessionScreen.Copy.Status.idle, device: "Alex's MacBook", trigger: "every 30 min"),
                    agentIcon: BoardTypeDisplay.iconName(for: "bug"),
                    agentColor: "#a855f7"
                )
            } trailing: {
                ChatHeaderGhostButton(icon: AppIcons.voiceCall, label: "Call") {}
                SessionScreenGear {}
                more
            }
        }
        .padding(.horizontal, -16)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-header-specimen")
    }

    private var more: some View {
        ChatHeaderGhostButton(icon: AppIcons.uiMore, label: "More") {}
    }
}

/// `chat-transcript`: the web entry's phone column — #EXP-1736 ONE
/// transcript read like a group chat (`TranscriptGrouping.transcriptLayout`
/// over the specimen rows): the viewer's own bubbles right, a member's
/// bubble under their initials, the agent's group of two bubbles under its
/// mark, a child session's report as that session's bubble (`›`), each time
/// inside bottom-right; the superseded plan card, the agent's inline preview
/// (THE surface card through the real renderer, its two `reply` presses),
/// the divider pills.
struct ChatTranscriptSpecimens: View {
    @Environment(AppDependencies.self) private var deps
    @State private var replied: String?

    /// The web entry's `LAUNCH_SURFACE` (the core catalog).
    static let launchSurface: CardSurface? = {
        func press(_ text: String) -> [String: Any] {
            ["press": ["event": ["name": Chats.SurfaceEnter.replyEvent, "context": ["text": text]]]]
        }
        let root: [String: Any] = [
            "id": "root", "component": "Stack", "props": ["gap": "md"],
            "children": [
                [
                    "id": "stat", "component": "StatTile",
                    "props": [
                        "label": "Cold start, Pixel 6a", "value": "740 ms", "delta": "−47 %", "tone": "success",
                        "trend": [1400, 1310, 1180, 980, 820, 740],
                    ],
                    "children": [] as [Any],
                ],
                [
                    "id": "actions", "component": "Stack", "props": ["direction": "horizontal", "gap": "sm"],
                    "children": [
                        ["id": "ship", "component": "Button", "props": ["label": "Ship snapshot"],
                         "children": [] as [Any], "on": press("Ship the snapshot")],
                        ["id": "keep", "component": "Button", "props": ["label": "Keep profiling", "variant": "secondary"],
                         "children": [] as [Any], "on": press("Keep profiling")],
                    ] as [Any],
                ],
            ] as [Any],
        ]
        return CardSurface.parse(["root": root, "data": [String: Any]()])
    }()

    private var context: AgentMarkdownContext {
        AgentMarkdownContext(baseURL: nil, accountId: "", httpClient: deps.httpClient)
    }

    /// The specimen's rows in order (`text` nil = a card drawn below).
    private struct Row {
        let item: TranscriptGrouping.Item
        let text: String?
        let time: String?
        /// #EXP-1772: an `event` row's line (THE inline marker).
        var marker: String? = nil
    }

    private static let member = UserEntity(
        id: "u2", name: "Dana Reyes", email: "dana@example.com", image: nil, createdAt: "", updatedAt: ""
    )

    private static func message(_ id: String, _ kind: String, author: String? = nil, session: String? = nil, _ text: String, _ time: String) -> Row {
        Row(item: TranscriptGrouping.Item(id: id, kind: .message, authorKind: kind, authorId: author, authorSessionId: session), text: text, time: time)
    }

    private static func card(_ id: String) -> Row {
        Row(item: TranscriptGrouping.Item(id: id, kind: .full), text: nil, time: nil)
    }

    private static func event(_ id: String, _ text: String) -> Row {
        Row(item: TranscriptGrouping.Item(id: id, kind: .full), text: nil, time: nil, marker: text)
    }

    private static let rows: [Row] = [
        message("m1", "user", author: "u1", "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it.", "21:02"),
        message("m2", "user", author: "u2", "Seeing the same on the Galaxy A54.", "21:04"),
        message("m3", "agent", "Deferred the shape subscribe until after the first frame and added a board snapshot cache.", "21:14"),
        message("m4", "agent", "Here's where the time went:", "21:14"),
        card("surface"),
        message("m5", "agent", session: "c1", "Measured on the Galaxy A54: 760 ms cold, 310 ms warm.", "21:31"),
        card("superseded"),
        card("compact"),
        message("m6", "user", author: "u1", "Anything new overnight?", "09:02"),
        message("m7", "agent", "Three reports, all about the iPad sign-in. I linked them to #APP-19.", "09:03"),
        // #EXP-1772: a busy agent chat: a schedule fire above the reply it
        // got, a merge event, a session's relay.
        event("e1", SessionTree.ChatCopy.events["trigger_schedule"] ?? "Schedule"),
        message("m8", "agent", "PR #412 merged overnight; the iPad sign-in reports stopped.", "07:00"),
        event("e2", "Merged PR #412"),
        message("x2", "agent", session: "c-fix", "Verified on the iPad: sign-in keeps the return URL, and the redirect loop is gone on Safari too.", "08:41"),
        card("new"),
    ]

    private static let layouts: [String: TranscriptGrouping.RowLayout] = {
        let input = TranscriptGrouping.Input(
            viewerId: "u1",
            chat: TranscriptGrouping.Chat(sessionId: "a1", title: "Triage", agentId: "ag1", originSessionId: nil),
            members: [TranscriptGrouping.Named(id: "u1", name: "Alex"), TranscriptGrouping.Named(id: "u2", name: "Dana Reyes")],
            agents: [TranscriptGrouping.Named(id: "ag1", name: "Triage")],
            sessions: [
                TranscriptGrouping.KnownSession(id: "c1", title: "Measure on the Galaxy A54", agentId: nil),
                TranscriptGrouping.KnownSession(id: "c-fix", title: "Fix the sign-in redirect", agentId: nil),
            ],
            items: rows.map(\.item)
        )
        return Dictionary(uniqueKeysWithValues: TranscriptGrouping.transcriptLayout(input).map { ($0.id, $0) })
    }()

    private static func avatar(_ sender: TranscriptGrouping.Sender) -> SenderAvatarModel {
        switch sender.kind {
        case .member: .member(user: sender.id == member.id ? member : nil, id: sender.id)
        case .agent: .agent(UserAvatarAgent(icon: "bug", color: "#f59e0b"))
        case .session: .session
        }
    }

    @ViewBuilder
    private func rowContent(_ row: Row, layout: TranscriptGrouping.RowLayout?) -> some View {
        if let text = row.text {
            if layout?.side == .own {
                UserMessageBubble(text: text, context: context, caption: row.time)
            } else {
                MessageBubble(
                    side: .other,
                    author: layout?.name == true ? layout?.sender?.displayName : nil,
                    time: row.time,
                    onPress: layout?.sender?.link.map { _ -> () -> Void in {} }
                ) {
                    AgentMarkdownText(text: text, context: context, hugsWidth: true)
                }
            }
        } else if let marker = row.marker {
            TranscriptMarkerRow(label: marker)
        } else {
            switch row.item.id {
            case "surface":
                if let surface = Self.launchSurface {
                    SurfaceCardRow(surface: surface, title: "Launch path", animate: false) { text in
                        replied = text
                    }
                }
                if let replied {
                    Text("reply → \(replied)")
                        .font(.expMono(.caption))
                        .foregroundStyle(DesignTokens.Semantic.green)
                        .accessibilityIdentifier("surface-reply-echo")
                }
            case "superseded":
                SupersededPlanCard()
            case "compact":
                TranscriptDividerPill(icon: AppIcons.codingCompact, text: "\(Chats.Copy.Dividers.compact) · 18:20")
            default:
                TranscriptDividerPill(icon: AppIcons.uiClean, text: "\(Chats.Copy.Dividers.new) · 09:30")
            }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Self.rows, id: \.item.id) { row in
                let layout = Self.layouts[row.item.id]
                TranscriptRow(layout: layout, avatar: Self.avatar) {
                    rowContent(row, layout: layout)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-transcript-specimen")
    }
}

/// `chat-menus` (web `ChatMenusSpecimen`'s set + captions ×4): a live
/// chat's menu, an ended chat's, the agent menu for the busy owner and for
/// an idle member, then the delete confirm (THE `GlassAlert`).
struct ChatMenuSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            captioned("A live chat") {
                menu("Spike: offline queue", Chats.chatMenuEntries(rootStatus: "running", canSteer: true, canDelete: true).map { .chat($0) })
            }
            captioned("An ended chat") {
                menu("#APP-16 Fix flaky scroll restore", Chats.chatMenuEntries(rootStatus: "ended", canSteer: true, canDelete: true).map { .chat($0) })
            }
            captioned("The agent menu (busy, owner)") {
                menu("Triage", Chats.agentMenuEntries(busy: true, isOwner: true).map { .agent($0) })
            }
            captioned("The agent menu (idle, member)") {
                menu("Triage", Chats.agentMenuEntries(busy: false, isOwner: false).map { .agent($0) })
            }
            captioned("The delete confirm") {
                GlassAlert(
                    title: Chats.chatDeleteTitle(1),
                    message: Chats.Copy.ConfirmDelete.body,
                    actions: [
                        GlassAlertAction(PullRequestStack.cancelLabel, role: .outline, isCancel: true, id: "cancel") {},
                        GlassAlertAction(Chats.Copy.Menu.delete, role: .destructive, id: "delete") {},
                    ]
                )
            }
        }
        .accessibilityIdentifier("chat-menus-specimen")
    }

    private func captioned(_ text: String, @ViewBuilder _ content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(text)
                .font(.caption)
                .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            content()
        }
    }

    private func menu(_ title: String, _ entries: [ChatMenuSheet.Entry]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.headline)
                .foregroundStyle(.white)
                .padding(.horizontal, 12)
                .padding(.top, 12)
            ForEach(Array(entries.enumerated()), id: \.element) { index, entry in
                // The live sheet's own separator rule (Stop / Agent settings
                // sit apart from the conversation pair).
                if index > 0, ChatMenuSheet.separates(entries[index - 1], entry) {
                    GlassDivider().padding(.vertical, 4)
                }
                GlassMenuItemLabel(
                    ChatMenuSheet.label(entry),
                    icon: ChatMenuSheet.icon(entry),
                    destructive: ChatMenuSheet.destructive(entry)
                )
            }
        }
        .padding(.bottom, 6)
        .background(GlassSheetTokens.background, in: RoundedRectangle(cornerRadius: GlassSheetTokens.cornerRadius))
    }
}

/// `schedule-picker`: THE trigger rows on the new `minutes` repeat.
struct SchedulePickerSpecimens: View {
    @State private var minutes = ScheduleDraft(cron: "*/10 * * * *", tz: "Europe/Vienna")
    @State private var daily = ScheduleDraft(cron: "0 9 * * 1-5", tz: "Europe/Vienna")

    var body: some View {
        // The trigger sheet's own form chrome (`TriggerForm`).
        Form {
            Section { ScheduleFields(draft: $minutes) }
                .listRowBackground(glassFormRowFill)
            Section { ScheduleFields(draft: $daily) }
                .listRowBackground(glassFormRowFill)
        }
        .scrollContentBackground(.hidden)
        .frame(height: 460)
        .padding(.horizontal, -16)
        .accessibilityIdentifier("schedule-picker-specimen")
    }
}

/// `mention-pill` (session e4a04d22): a chat's `@email` renders as the
/// comments' round name pill once it resolves to a team member; an address
/// that is no member stays text. The real markdown path
/// (`AgentIssueRefContext.mentionMembers`).
struct MentionPillSpecimens: View {
    @Environment(AppDependencies.self) private var deps

    static let members = [MentionMember(name: "Ada Lovelace", email: "ada@example.com")]

    private var context: AgentMarkdownContext {
        AgentMarkdownContext(
            baseURL: nil, accountId: "", httpClient: deps.httpClient,
            issueRefs: AgentIssueRefContext(
                teamId: "", db: deps.db, onOpen: { _ in }, mentionMembers: Self.members
            )
        )
    }

    /// EXP-1553: THE web styleguide entry ×4 — a member's pill in a chat
    /// line, an unresolved address left as written; no bubbles, no card.
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            AgentMarkdownText(text: "Ready for review, @ada@example.com", context: context)
            AgentMarkdownText(text: "cc @x@y.com", context: context)
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("mention-pill-specimen")
    }
}

/// #EXP-1662 `plan-page` capture (`-uiTestingStyleguide plan-page`, presented
/// by the navigator beside `ChatCardsScreen`).
struct PlanPageSpecimenScreen: View {
    static let kinds: Set<String> = ["plan-page"]

    /// plan-page.json `specimen.card` (THE styleguide plan-page specimen ×4).
    static let planPageCard: TreeMessageCard? = TreeMessageCard.parse(###"{"kind": "plan", "plan": "# Plan: editor-first app creation\n\nA new app opens straight in the editor; every app gets an MCP endpoint.\n\n## Current state\n- Creating an app starts from a form.\n- The editor opens only after the first save.\n\n## Part A: editor-first creation\nThe editor opens on an empty canvas with the chat docked.\n\n![web editor new](/api/attachments/sg-1)\n![web editor preview](/api/attachments/sg-2)\n![web editor data](/api/attachments/sg-3)\n![web editor versions](/api/attachments/sg-4)\n\n## Part B: MCP for eApps\nEach team's apps answer at one MCP endpoint, scoped by a team token.\n\n## Phased issues\n1. The contract\n2. The editor\n3. The MCP endpoint", "decision": null, "pictures": [{"label": "web-editor-1", "attachmentId": "sg-1", "width": 1440, "height": 900}, {"label": "web-editor-2", "attachmentId": "sg-2", "width": 1440, "height": 900}, {"label": "web-editor-3", "attachmentId": "sg-3", "width": 1440, "height": 900}, {"label": "web-editor-4", "attachmentId": "sg-4", "width": 1440, "height": 900}], "questions": [{"id": "endpoint", "prompt": "How should clients address the MCP endpoint for a team's apps?", "multi": false, "custom": true, "options": [{"label": "One endpoint per team", "description": "/mcp/t/acme, tools prefixed by app"}, {"label": "One endpoint per app", "description": "/mcp/apps/:id"}]}, {"id": "chat", "prompt": "Keep the floating chat layout in the editor?", "multi": false, "custom": false, "options": [{"label": "Keep it"}, {"label": "Dock it right"}]}]}"###)

    var body: some View {
        ZStack {
            AppBackground()
            PlanPageSpecimen(card: Self.planPageCard)
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-cards-screen")
    }
}

/// #FEED-191 `needs-you` (needs-you.json `cases[1]`, hardcoded ×4): the phone
/// entry row, then THE list. #EXP-1710: ONE stuck PR row (merge-train
/// `stuckRow` cases[0]: `Auto-merge stuck on #12`, `3 bounces`, Retry ·
/// Merge now · Close PR) under `cases[1]`'s rows, drawn by the real row.
struct NeedsYouSpecimens: View {
    private static let stuck = MergeTrain.stuckRow(
        number: 12, state: "held", holdReason: "bounces", deviceOfflineSince: nil,
        now: WireTimestamps.parse("2027-01-15T10:00:00.000Z") ?? Date(), owner: true
    )

    private static let rows = decisions + (stuck.map { stuck in
        [NeedsYou.Row(
            messageId: "pr12", sessionId: "r2", rootId: "r2", kind: .pr, opens: .results,
            title: stuck.title, chat: "Palette", meta: stuck.meta, age: "3h",
            createdAt: "2027-01-15T05:00:00.000Z", actions: stuck.actions, number: 12
        )]
    } ?? [])

    private static let decisions = [
        NeedsYou.Row(
            messageId: "p1", sessionId: "r1", rootId: "r1", kind: .plan, opens: .plan,
            title: "Plan: Group by assignee", chat: "Assignee groups", meta: "", age: "24m",
            createdAt: "2027-01-15T07:36:00.000Z"
        ),
        NeedsYou.Row(
            messageId: "q1", sessionId: "r2", rootId: "r2", kind: .question, opens: .card,
            title: "Which palette?", chat: "Palette", meta: "", age: "1h",
            createdAt: "2027-01-15T07:00:00.000Z"
        ),
    ]

    var body: some View {
        ZStack {
            AppBackground()
            VStack(alignment: .leading, spacing: 16) {
                NeedsYouEntryRow(count: Self.rows.count)
                NeedsYouList(rows: Self.rows, onOpen: { _ in }, onAction: { _, _ in })
            }
            .padding(16)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
    }
}

/// #FEED-191 `results-tab` (results-tab.json `tab.specimen` = `cases[1]`,
/// hardcoded ×4): the tab body, no PR block, THE committed sample pictures.
/// `results-tab-lightbox` = the same with THE lightbox open on picture 1.
struct ResultsTabSpecimens: View {
    var lightbox = false

    private static let results = [
        ResultsTab.Result(
            id: "res-web", sessionId: "c1", sessionName: "Web draft", createdAt: 1_799_992_800_000, age: "2h",
            title: "Label picker", description: nil,
            pictures: [
                ResultsTab.Picture(label: "web picker", attachmentId: "att-w", width: 1600, height: 1000, caption: "Mixed state"),
                ResultsTab.Picture(label: "ios picker", attachmentId: "att-i", width: 1179, height: 2556, caption: nil),
            ],
            surface: true
        ),
        ResultsTab.Result(
            id: "old-result", sessionId: "r1", sessionName: "Batch-edit labels", createdAt: 1_799_222_400_000, age: "9d",
            title: "First draft", description: nil, pictures: [], surface: true
        ),
    ]

    @State private var opened: PictureViewerSet?

    var body: some View {
        ZStack {
            AppBackground()
            ScrollView {
                ResultsTabView(
                    results: Self.results,
                    onOpenPicture: { result, index in opened = resultPictureSet(result, start: index) },
                    lead: { EmptyView() },
                    thumbnail: { ResultThumbnailImage(picture: $0) },
                    surface: { _ in EmptyView() }
                )
                .padding(16)
            }
        }
        .pictureViewer($opened)
        .onAppear {
            if lightbox { opened = resultPictureSet(Self.results[0], start: 1) }
        }
    }
}

/// #EXP-1662 `plan-page`, the phone form (web `PlanPageSpecimen`): #EXP-1732
/// the plan as an OPENED item (a page pushed over the chat): THE chat header
/// with its back and the plan's title, the plan page's body under it, waiting.
private struct PlanPageSpecimen: View {
    let card: TreeMessageCard?

    private var row: ChatPlanRow? {
        let message = TreeMessage(
            id: "plan-specimen", sessionId: "plan-specimen-session", authorKind: "agent", authorId: nil, body: "",
            card: card, answeredAt: nil, consumedAt: nil, createdAt: "1970-01-01T00:00:00.000Z",
            authorSessionId: "plan-specimen-session"
        )
        if case let .plan(row)? = SessionChatRow.askCard(message) { return row }
        return nil
    }

    var body: some View {
        ZStack {
            AppBackground()
            VStack(spacing: 0) {
                ChatHeader {
                    Text(PlanPage.Copy.title)
                        .font(.system(size: CGFloat(Chats.Shell.ChatHeader.title), weight: .semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                } trailing: {
                    EmptyView()
                }
                Rectangle()
                    .fill(GlassTokens.strokeCard)
                    .frame(height: GlassTokens.hairline)
                if let row {
                    PlanPageBody(card: row, pictureSizes: card?.pictures ?? [], onSubmit: { _ in })
                }
            }
        }
    }
}

/// EXP-1459 `sealed-locked` (sealed-content.json `placeholder`): content this
/// install can't open (yet), the web entry's composition: in the transcript
/// THE divider pill with the lock (no time), never a bubble; in the chats
/// list the preview line italic muted, like `working`.
struct SealedLockedSpecimens: View {
    @Environment(AppDependencies.self) private var deps

    private var context: AgentMarkdownContext {
        AgentMarkdownContext(baseURL: nil, accountId: "", httpClient: deps.httpClient)
    }

    private func pill(_ reason: LockedReason) -> some View {
        TranscriptDividerPill(
            icon: AppIcons.conceptOrName(SealedPlaceholder.icon) ?? SealedPlaceholder.icon,
            text: SealedPlaceholder.copy(reason)
        )
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            VStack(alignment: .leading, spacing: 14) {
                UserMessageBubble(
                    text: "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it.",
                    context: context,
                    caption: "21:02"
                )
                pill(.noKey)
                MessageBubble(side: .other, time: "21:14") {
                    AgentMarkdownText(text: "Deferred the shape subscribe until after the first frame.", context: context, hugsWidth: true)
                }
                pill(.unreadable)
            }
            VStack(alignment: .leading, spacing: 0) {
                ChatRowView(
                    title: "Running one", time: "09:25",
                    preview: SealedPlaceholder.copy(.noKey), previewItalic: true
                ) { ChatLeadMark(lead: .working) }
                ChatRowView(
                    title: "Needs you one", time: "09:10",
                    preview: "Question: Snapshot?", previewLabel: "Question:", needsYou: true
                ) { ChatLeadMark(lead: .needsYou) }
                ChatRowView(
                    title: "Open PR", time: "08:30",
                    preview: SealedPlaceholder.copy(.unreadable), previewItalic: true,
                    unreadCount: 1, word: ChatRowWord.readyToMerge.text
                ) { ChatLeadMark(lead: .open) }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("sealed-locked-specimen")
    }
}

/// #EXP-1611 (styleguide `pr-checks`, pr-checks.json): THE header pill —
/// since #EXP-1710 the merge train's STATUS pill (merge-train.json
/// `statusPill`), every case in fixture order — then the Checks panel the
/// pill's tap opens, over the fixture's `specimen` (checks + now).
struct PrChecksSpecimens: View {
    /// pr-checks.json `specimen.checks`.
    static let checks: [PrChecks.Check] = [
        PrChecks.Check(
            name: "lint", status: "completed", conclusion: "success",
            startedAt: "2026-11-01T12:00:00.000Z", completedAt: "2026-11-01T12:00:38.000Z",
            detailsUrl: "https://github.com/o/r/actions/runs/1/job/5"
        ),
        PrChecks.Check(
            name: "e2e", status: "in_progress", startedAt: "2026-11-01T12:07:57.000Z",
            detailsUrl: "https://github.com/o/r/actions/runs/1/job/2"
        ),
        PrChecks.Check(name: "docs", status: "completed", conclusion: "skipped"),
        PrChecks.Check(
            name: "gate", status: "completed", conclusion: "failure",
            startedAt: "2026-11-01T12:00:00.000Z", completedAt: "2026-11-01T12:04:12.000Z",
            detailsUrl: "https://github.com/o/r/actions/runs/1/job/1"
        ),
        PrChecks.Check(
            name: "android", status: "queued",
            detailsUrl: "https://github.com/o/r/actions/runs/1/job/3"
        ),
    ]

    /// pr-checks.json `specimen.now`.
    static let now = WireTimestamps.parse("2026-11-01T12:10:00.000Z") ?? Date()

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            // #EXP-1710: EVERY merge-train.json `statusPill` case, fixture
            // order, in one wrapping row (`MergeTrain.specimen`).
            FlowLayout(spacing: 8, centersLines: true) {
                ForEach(Array(MergeTrain.specimen.enumerated()), id: \.offset) { index, input in
                    let pill = input.pill
                    MergeStatusPill(
                        view: pill, action: pill.tap == nil ? nil : Optional({}), identifier: "pr-checks-pill-\(index)"
                    )
                }
            }
            // The pill's tap: the check rows ARE the menu (no back row).
            panel {
                PrChecksMenuRows(rows: PrChecks.checksList(Self.checks, now: Self.now))
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("pr-checks-specimen")
    }

    /// THE Menu's sheet ground, as the chat-menus specimen draws it.
    private func panel(@ViewBuilder _ content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            content()
        }
        .padding(.vertical, 12)
        .padding(.horizontal, GlassSheetTokens.headerHPadding - GlassMenuTokens.itemHPadding)
        .background(GlassSheetTokens.background, in: RoundedRectangle(cornerRadius: GlassSheetTokens.cornerRadius))
    }
}
