import ExpCore
import ExpUI
import SwiftUI

// Thread 58cb32df (chats.json): the iOS styleguide's chat entries (web
// `apps/styleguide` entries `chat-list`, `phone-tab-bar`, `chat-transcript`,
// `chat-menus`, `chat-selection`, the updated `schedule-picker`): the REAL views over specimen
// values. Capture only: `-uiTestingStyleguide <id>` (`ThreadCardsScreen`).

/// `chat-list`: THE phone Chats screen over the bottom bar — the large
/// "Chats" headline with the search + inbox circles, the agent bubbles, the
/// four filter chips and the flat rows (the web entry's and Android's
/// `ChatListSpecimenScreen` composition and data: the draft's list).
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
        var word: ThreadRowWord?
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
            previewKind: .you, preview: "You: ship it", time: Chats.Copy.Time.yesterday),
    ]

    private static let counts: [Chats.ChatFilter: Int] = [.needsYou: 1, .running: 1, .unread: 1]

    /// chats.json `bubbleCases[0].expected`'s agents, the viewer's
    /// Assistant first (the brand mark). #EXP-1461: only the ones with a
    /// story ring are drawn.
    private static let bubbles = [
        bubble("as", "Assistant", "bot", "#6366f1", assistant: true),
        bubble("ag1", "Triage", "bug", "#a855f7"),
        bubble("ag2", "Designer", "bot", "#6366f1"),
        bubble("ag3", "Release", "package", "#22c55e"),
    ]

    private static func bubble(
        _ id: String, _ name: String, _ icon: String, _ color: String, assistant: Bool = false
    ) -> AgentBubbleItem {
        AgentBubbleItem(
            id: id, name: name,
            icon: BoardTypeDisplay.iconName(for: icon) ?? SessionTree.ListCopy.agentDefaultIcon,
            color: Color(hex: color) ?? .white, assistant: assistant
        )
    }

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

    /// The selected rows as the bar reads them: `a` live (needs you),
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

    @State private var filter: Chats.ChatFilter = .all

    private var shown: [Row] {
        Self.rows.filter { row in
            switch filter {
            case .all: true
            case .needsYou: row.group == "needs_you"
            case .running: row.group == "running"
            case .unread: row.unread > 0
            }
        }
    }

    private var shownAgents: [Chats.AgentChatRow] {
        Self.agentRows.filter { row in
            switch filter {
            case .all: true
            case .needsYou: row.needsYou
            case .running: row.busy
            case .unread: row.unreadCount > 0
            }
        }
    }

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        // #EXP-1461 THE STORY ROW (stories.json specimen):
                        // Triage + Designer wear their rings, the two chat
                        // rings follow; Assistant and Release have none.
                        AgentBubbles(
                            items: Self.bubbles,
                            rings: StorySpecimensScreen.rings,
                            onHold: { _ in },
                            onOpenStory: { _ in }
                        ) { ring in
                            StoryRingAvatarView(avatar: StorySpecimensScreen.avatar(ring))
                        }
                        .padding(.horizontal, -16)
                        .padding(.top, 4)
                        .padding(.bottom, 10)
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 8) {
                                ForEach(Chats.filters, id: \.self) { option in
                                    GlassPill(
                                        Chats.chatFilterLabel(option, counts: Self.counts),
                                        size: .md,
                                        mode: .select(isSelected: filter == option) { filter = option }
                                    )
                                }
                            }
                            .padding(.horizontal, 16)
                        }
                        .padding(.horizontal, -16)
                        .padding(.bottom, 4)
                        let agentRows = shownAgents
                        if selection == nil, !agentRows.isEmpty {
                            ChatSectionHeader(Chats.Copy.Sections.agents)
                            ForEach(agentRows) { row in
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
                            if !shown.isEmpty {
                                ChatSectionHeader(Chats.Copy.Sections.chats)
                            }
                        }
                        if selection != nil, filter == .all {
                            ChatArchivedRow(count: Self.archived)
                        }
                        ForEach(shown) { row in
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
                            .chatSelectedTint(selection?.contains(row.id) == true)
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
/// launcher — the board view (the caret), Chats and Team selected, the lens
/// mid-drag between Chats and Apps; the amber dot on Chats.
struct PhoneTabBarSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            PhoneShellSpecimen.caption("On the board view (the caret opens the board menu)")
            bar(.board, onBoardView: true)
            PhoneShellSpecimen.caption("Chats selected")
            bar(.chats)
            PhoneShellSpecimen.caption("Team selected")
            bar(.team)
            PhoneShellSpecimen.caption("Dragging: the lens between Chats and Apps")
            bar(.chats, specimen: .lens(candidate: .apps, offset: -10))
        }
        .padding(.horizontal, -16)
        .accessibilityIdentifier("phone-tab-bar-specimen")
    }

    private func bar(
        _ selected: Chats.Shell.Tab, onBoardView: Bool = false, specimen: MobileTabBar.Specimen? = nil
    ) -> some View {
        MobileTabBar(
            selected: selected,
            board: PhoneShellSpecimen.activeBoard,
            onBoardView: onBoardView,
            team: PhoneShellSpecimen.team,
            chatsNeedYou: true,
            specimen: specimen
        )
    }
}

/// `phone-board-menu` (#EXP-1384): the board view's Board tab morphed the
/// whole strip into the glass board menu, shown OPEN over a dimmed page:
/// the team's boards (the active one lit with the check) then New board.
struct PhoneBoardMenuSpecimens: View {
    var body: some View {
        ZStack(alignment: .bottom) {
            VStack(alignment: .leading, spacing: 10) {
                Text(PhoneShellSpecimen.activeBoard.name)
                    .font(.largeTitle.weight(.bold))
                    .foregroundStyle(.white)
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
                selected: .board,
                board: PhoneShellSpecimen.activeBoard,
                onBoardView: true,
                team: PhoneShellSpecimen.team,
                boards: PhoneShellSpecimen.boards,
                onNewBoard: {},
                specimen: .menuOpen
            )
        }
        .frame(height: 340)
        .background(DesignTokens.Palette.background, in: RoundedRectangle(cornerRadius: 12))
        .padding(.horizontal, -16)
        .accessibilityIdentifier("phone-board-menu-specimen")
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
            header(Chats.Copy.Headers.devices, unread: true, pushed: true)
        }
        .padding(.horizontal, -16)
        .accessibilityIdentifier("phone-page-header-specimen")
    }

    private func header(_ title: String, unread: Bool, pushed: Bool = false) -> some View {
        PageHeaderFrame(title: title, pushed: pushed)
            .environment(\.inboxUnread, unread)
            .frame(height: 120)
            .clipShape(RoundedRectangle(cornerRadius: 12))
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
                .navigationDestination(for: Int.self) { _ in page(title, pushed: true) }
        }
        .onAppear { if pushed { path = [1] } }
    }

    private func page(_ title: String, pushed: Bool) -> some View {
        ZStack {
            AppBackground()
        }
        .pageHeader(title, pushed: pushed)
    }
}

/// `chat-transcript`: the web entry's phone column — the person's and the
/// agent's bubbles, the superseded plan line, the agent's inline preview
/// (THE surface card through the real renderer, its two `reply` presses),
/// the divider pills; "Replay the enter" re-mounts the card as a LIVE
/// arrival (`surfaceCard.enter`).
struct ChatTranscriptSpecimens: View {
    @Environment(AppDependencies.self) private var deps
    @State private var replay = 0
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

    private func agent(_ text: String, _ time: String) -> some View {
        AgentChatBubble(time: time) {
            AgentMarkdownText(text: text, context: context, options: [.hardLineBreaks])
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            UserMessageBubble(
                text: "Cold start is at 1.4 s on the Pixel 6a. Profile the launch path and cut the sync bootstrap out of it.",
                context: context,
                caption: "21:02"
            )
            HStack(spacing: 8) {
                AppIcon(AppIcons.codingPlan, size: 13)
                Text(Chats.Copy.superseded).font(.footnote)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            agent("Deferred the shape subscribe until after the first frame and added a board snapshot cache. Here's where the time went:", "21:09")
            if let surface = Self.launchSurface {
                SurfaceCardRow(surface: surface, title: "Launch path", animate: replay > 0) { text in
                    replied = text
                }
                .id(replay)
            }
            if let replied {
                Text("reply → \(replied)")
                    .font(.caption.monospaced())
                    .foregroundStyle(DesignTokens.Semantic.green)
                    .accessibilityIdentifier("surface-reply-echo")
            }
            TranscriptDividerPill(icon: AppIcons.codingCompact, text: "\(Chats.Copy.Dividers.compact) · 18:20")
            UserMessageBubble(text: "Anything new overnight?", context: context, caption: "09:02")
            agent("Three reports, all about the iPad sign-in. I linked them to #APP-19.", "09:03")
            TranscriptDividerPill(icon: AppIcons.uiClean, text: "\(Chats.Copy.Dividers.new) · 09:30")
            GlassPill("Replay the enter", size: .md, mode: .action { replay += 1 })
                .accessibilityIdentifier("surface-replay-enter")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("chat-transcript-specimen")
    }
}

/// `chat-menus`: a live chat's menu, an ended chat's, the agent menu.
struct ChatMenuSpecimens: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            menu("Spike: offline queue", Chats.threadMenuEntries(rootStatus: "running", canSteer: true, canDelete: true).map { .thread($0) })
            menu("#APP-16 Fix flaky scroll restore", Chats.threadMenuEntries(rootStatus: "ended", canSteer: true, canDelete: true).map { .thread($0) })
            menu("Triage", Chats.agentMenuEntries(busy: true, isOwner: true).map { .agent($0) })
        }
        .accessibilityIdentifier("chat-menus-specimen")
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

/// `mention-pill` (thread e4a04d22): a chat's `@email` renders as the
/// comments' round name pill once it resolves to a team member; an address
/// that is no member stays text. The REAL bubbles over the real markdown
/// path (`AgentIssueRefContext.mentionMembers`).
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

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            UserMessageBubble(
                text: "Ask @ada@example.com to review the sign-in fix.",
                context: context,
                caption: "09:12"
            )
            AgentChatBubble(time: "09:13") {
                AgentMarkdownText(
                    text: "Pinged @ada@example.com on the PR. @x@y.com is not on the team, so it stays text.",
                    context: context,
                    options: [.hardLineBreaks]
                )
            }
        }
    }
}

/// S-3 (styleguide `session-xref`, session-thread.json): THE inter-session
/// link header, once per kind — an agent's tree carries its mark, a
/// person's session the session glyph.
struct SessionXrefSpecimens: View {
    private static let agent = SessionXrefHeader.AgentMark(
        icon: SessionTree.ListCopy.agentDefaultIcon,
        color: Color(hex: SessionTree.ListCopy.agentDefaultColor) ?? .white
    )

    private static let links: [(SessionXref, SessionXrefHeader.AgentMark?)] = [
        (SessionXref(kind: .message, sessionId: "x1", name: "Triage", agentId: "a1"), agent),
        (SessionXref(kind: .report, sessionId: "x2", name: "Fix the sign-in redirect", agentId: nil), nil),
        (SessionXref(kind: .started, sessionId: "x3", name: "Triage", agentId: "a1"), agent),
        (SessionXref(kind: .reported, sessionId: "x4", name: "Ship the login flow", agentId: nil), nil),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            ForEach(Self.links, id: \.0.sessionId) { xref, mark in
                SessionXrefHeader(xref: xref, agent: mark) {}
            }
        }
    }
}

/// S-3 STORIES (stories.json `specimen`, styleguide `story-ring` /
/// `story-viewer`): THE same data ×4, hardcoded from `specimen.input` and
/// folded by `Stories.rings`. story-ring = the rings at the bubble size with
/// their titles, the same rings at `smallRing` without titles, then a
/// StoryProgress; story-viewer = THE viewer on ring 0, story 0, the progress
/// frozen at 0. Capture only: `-uiTestingStyleguide story-ring|story-viewer`
/// (presented by the navigator beside `ThreadCardsScreen`).
struct StorySpecimensScreen: View {
    let kind: String

    static let kinds: Set<String> = ["story-ring", "story-viewer", "story-tab"]

    private static let now: Double = 1800000000000
    private static let smallRing: CGFloat = 36

    private static let sessions: [Stories.SessionInput] = [
        Stories.SessionInput(
            id: "triage", rootId: "triage", name: "Triage", agentId: "ag1", startedByAgentId: nil
        ),
        Stories.SessionInput(
            id: "board", rootId: "board", name: "Board header polish", agentId: nil, startedByAgentId: nil
        ),
        Stories.SessionInput(
            id: "designer", rootId: "designer", name: "Designer", agentId: "ag2", startedByAgentId: nil
        ),
        Stories.SessionInput(
            id: "ship", rootId: "ship", name: "Ship the launch checklist", agentId: nil, startedByAgentId: nil
        ),
    ]

    /// The chat sessions' CLI (`specimen.input.sessions[].cli`).
    private static let clis: [String: String] = ["board": "claude", "ship": "claude"]

    private static let agents: [(Stories.AgentInput, StoryRingAvatar)] = [
        (Stories.AgentInput(id: "ag1", name: "Triage", assistant: false), .agent(icon: "bug", color: "#a855f7")),
        (Stories.AgentInput(id: "ag2", name: "Designer", assistant: false), .agent(icon: "bot", color: "#6366f1")),
    ]

    private static let messages: [Stories.MessageInput] = [
        Stories.MessageInput(
            id: "triage-plan", sessionId: "triage", authorSessionId: "triage",
            createdAt: 1799999460000, answeredAt: nil,
            card: TreeMessageCard.parse(###"{"kind":"plan","plan":"## Close the duplicates\n\nMerge FB-8 into FB-3 and label the rest.","decision":null}"###)
        ),
        Stories.MessageInput(
            id: "triage-result", sessionId: "triage", authorSessionId: "triage",
            createdAt: 1799998200000, answeredAt: nil,
            card: TreeMessageCard.parse(###"{"kind":"surface","story":true,"title":"Today's triage","surface":{"root":{"id":"r","component":"Text","props":{"text":"3 labelled"}}}}"###)
        ),
        Stories.MessageInput(
            id: "board-question", sessionId: "board", authorSessionId: "board",
            createdAt: 1799999760000, answeredAt: nil,
            card: TreeMessageCard.parse(###"{"kind":"question","question":"Which accent for the board header?","options":["Indigo","Emerald"],"optionDescriptions":["The brand accent","Matches the merged state"],"custom":false,"surface":{"root":{"id":"root","component":"Stack","props":{"gap":"md"},"children":[{"id":"title","component":"Text","props":{"text":"Board header"},"children":[]},{"id":"accent","component":"ChoiceTiles","props":{"name":"accent","options":[{"label":"Indigo","value":"indigo"},{"label":"Emerald","value":"emerald"}],"value":{"path":"/accent"}},"children":[]},{"id":"stat","component":"StatTile","props":{"label":"Header height","value":"48 px","delta":"−8 px","tone":"success"},"children":[]}]},"data":{"accent":"indigo"}}}"###)
        ),
        Stories.MessageInput(
            id: "designer-result", sessionId: "designer", authorSessionId: "designer",
            createdAt: 1799996400000, answeredAt: nil,
            card: TreeMessageCard.parse(###"{"kind":"surface","story":true,"title":"Spacing review","surface":{"root":{"id":"r","component":"Text","props":{"text":"No drift"}}},"description":"Paddings match on all four platforms."}"###)
        ),
        Stories.MessageInput(
            id: "ship-result", sessionId: "ship", authorSessionId: "ship",
            createdAt: 1799992800000, answeredAt: nil,
            card: TreeMessageCard.parse(###"{"kind":"surface","story":true,"title":"Checklist done","surface":{"root":{"id":"r","component":"Text","props":{"text":"12 of 12"}}}}"###)
        ),
    ]

    static let rings: [Stories.StoryRing] = Stories.rings(
        now: now,
        sessions: sessions,
        agents: agents.map(\.0),
        messages: messages,
        reads: [Stories.ReadInput(sessionId: "ship", readAt: 1_799_996_400_000, storySeenAt: nil)]
    )

    static func card(_ storyId: String) -> TreeMessageCard? {
        messages.first { $0.id == storyId }?.card
    }

    /// An agent ring = its icon in its colour; a chat ring = the bare brand
    /// mark of its root's CLI.
    static func avatar(_ ring: Stories.StoryRing) -> StoryRingAvatar {
        guard let agentId = ring.agentId, let avatar = agents.first(where: { $0.0.id == agentId })?.1 else {
            return .chat(cli: ring.rootId.flatMap { clis[$0] })
        }
        guard case let .agent(icon, color) = avatar else { return avatar }
        return .agent(icon: BoardTypeDisplay.iconName(for: icon) ?? icon, color: color)
    }

    // MARK: - story-tab (stories.json `tab.specimen` = tabCases[1])

    private static let tabSessions: [Stories.SessionInput] = [
        Stories.SessionInput(id: "r1", rootId: "r1", name: "Batch-edit labels", agentId: nil, startedByAgentId: nil),
        Stories.SessionInput(id: "c1", rootId: "r1", name: "Web draft", agentId: nil, startedByAgentId: nil),
        Stories.SessionInput(id: "r2", rootId: "r2", name: "Other chat", agentId: nil, startedByAgentId: nil),
    ]

    private static func tabMessage(
        _ id: String, _ sessionId: String, _ author: String, _ createdAt: Double, _ answeredAt: Double?, _ card: String
    ) -> Stories.MessageInput {
        Stories.MessageInput(
            id: id, sessionId: sessionId, authorSessionId: author, createdAt: createdAt, answeredAt: answeredAt,
            card: TreeMessageCard.parse(card)
        )
    }

    private static let tabMessages: [Stories.MessageInput] = [
        tabMessage("q-wait", "r1", "r1", 1799999400000, nil, ###"{"kind":"question","question":"Ship the picker as is?","options":["Ship","Polish"],"answer":null}"###),
        tabMessage("res-web", "c1", "c1", 1799992800000, nil, ###"{"kind":"surface","surface":{"root":{"id":"c","component":"Card"}},"title":"Label picker","pictures":[{"label":"web picker","attachmentId":"att-w","width":1600,"height":1000,"caption":"Mixed state"},{"label":"ios picker","attachmentId":"att-i","width":1179,"height":2556,"caption":null},{"label":"android picker","attachmentId":null,"width":null,"height":null,"caption":null}]}"###),
        tabMessage("plan-ok", "r1", "r1", 1799892000000, 1799895600000, ###"{"kind":"plan","plan":"## Batch-edit labels\n\nOne request per change.","decision":"approved"}"###),
        tabMessage("plan-old", "r1", "r1", 1799888400000, 1799892000000, ###"{"kind":"plan","plan":"Old plan","decision":"superseded"}"###),
        tabMessage("plan-no", "c1", "c1", 1799982000000, 1799985600000, ###"{"kind":"plan","plan":"Drop the undo step","decision":"rejected"}"###),
        tabMessage("q-done", "c1", "c1", 1799989200000, 1799989800000, ###"{"kind":"question","question":"Which board?","options":[],"answer":"Mobile App"}"###),
        tabMessage("old-result", "r1", "r1", 1799222400000, nil, ###"{"kind":"surface","story":true,"surface":{"root":{"id":"c","component":"Card"}},"title":"First draft"}"###),
        tabMessage("status", "r1", "r1", 1799999940000, nil, ###"{"kind":"surface","surface":{"root":{"id":"c","component":"Card"}},"title":"Working"}"###),
        tabMessage("relay", "c1", "c1", 1799998800000, nil, ###"{"kind":"question","question":"Engine ask","options":["Yes"],"questionId":"q1"}"###),
        tabMessage("mirror", "r1", "c1", 1799999100000, nil, ###"{"kind":"ask","sessionId":"c1","messageId":"plan-no"}"###),
        tabMessage("elsewhere", "r2", "r2", 1799999940000, nil, ###"{"kind":"question","question":"Not here","options":["A"]}"###),
    ]

    /// tabCases[1]'s ring (`Stories.storyTab` over its input).
    static let tabRing: Stories.StoryRing? = Stories.storyTab(
        now: now,
        sessions: tabSessions,
        agents: [],
        messages: tabMessages,
        reads: [Stories.ReadInput(sessionId: "r1", readAt: 1_799_996_400_000, storySeenAt: nil)],
        rootId: "r1"
    )

    var body: some View {
        ZStack {
            AppBackground()
            if kind == "story-tab" {
                // The web specimen's frame: the 24pt card, the hairline, 20 in.
                let shape = RoundedRectangle(cornerRadius: 24, style: .continuous)
                ScrollView {
                    StoryTabView(ring: Self.tabRing)
                        .padding(20)
                        .background(DesignTokens.Palette.background, in: shape)
                        .overlay(shape.strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline))
                        .padding(16)
                        .accessibilityIdentifier("story-tab-specimen")
                }
            } else if kind == "story-viewer" {
                StoryViewer(
                    rings: Self.rings, startKey: Self.rings[0].key, card: Self.card, avatar: Self.avatar, fraction: 0,
                    onClose: {}
                )
            } else {
                VStack(alignment: .leading, spacing: 20) {
                    StoryRingStrip(rings: Self.rings, avatar: Self.avatar) { _ in }
                        .padding(.horizontal, -16)
                    HStack(spacing: 16) {
                        ForEach(Self.rings) { ring in
                            StoryRing(stories: ring.stories, size: Self.smallRing) {
                                StoryRingAvatarView(avatar: Self.avatar(ring))
                            }
                        }
                    }
                    StoryProgress(count: 4, index: 1, fraction: 0.4)
                }
                .padding(16)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            }
        }
        .preferredColorScheme(.dark)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("thread-cards-screen")
    }
}
