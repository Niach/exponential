import ExpCore
import ExpUI
import SwiftUI

// Thread 58cb32df (chats.json): the iOS styleguide's chat entries (web
// `apps/styleguide` entries `chat-list`, `phone-tab-bar`, `chat-transcript`,
// `chat-menus`, the updated `schedule-picker`): the REAL views over specimen
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

    /// chats.json `bubbleCases[0].expected`: amber = needs you, white =
    /// unread, a hairline = read.
    private static let bubbles = [
        bubble("ag1", "Triage", "bug", "#a855f7", .needsYou),
        bubble("ag2", "Designer", "bot", "#6366f1", .unread),
        bubble("ag3", "Release", "package", "#22c55e", .none),
    ]

    private static func bubble(
        _ id: String, _ name: String, _ icon: String, _ color: String, _ ring: AgentBubbleItem.Ring
    ) -> AgentBubbleItem {
        AgentBubbleItem(
            id: id, name: name,
            icon: BoardTypeDisplay.iconName(for: icon) ?? SessionTree.ListCopy.agentDefaultIcon,
            color: Color(hex: color) ?? .white, ring: ring
        )
    }

    private static let team = TeamEntity(
        id: "t1", name: "Acme", slug: "acme", iconUrl: nil, createdAt: "", updatedAt: ""
    )

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

    var body: some View {
        NavigationStack {
            ZStack {
                AppBackground()
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        AgentBubbles(items: Self.bubbles, onTap: { _ in }, onHold: { _ in })
                            .padding(.horizontal, -16)
                            .padding(.top, 4)
                            .padding(.bottom, 14)
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
                                wordIcon: row.word == .stack ? AppIcons.prStack : nil
                            ) { ChatLeadMark(lead: row.lead) }
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                }
                .tabBarBottomInset()
            }
            .navigationTitle(Chats.Copy.title)
            .toolbarTitleDisplayMode(.inlineLarge)
            .ownChromeToolbarItem(placement: .topBarTrailing) {
                HStack(spacing: 10) {
                    ChatsView.headerCircle(AppIcons.navSearch, dot: false)
                    ChatsView.headerCircle(AppIcons.navInbox, dot: true)
                }
            }
        }
        .overlay(alignment: .bottom) {
            MobileTabBar(
                agentActive: false, boardActive: false, devicesActive: false, chatsActive: true,
                teamActive: false, team: Self.team, chatsRunning: false, chatsNeedYou: true,
                composeEnabled: true,
                onBoard: {}, onDevices: {}, onChats: {}, onTeam: {}, onCompose: {}, onChat: {}
            )
        }
        .accessibilityIdentifier("chat-list-specimen")
    }
}

/// `phone-tab-bar`: Board · Devices · Chats · Team beside the launcher.
struct PhoneTabBarSpecimens: View {
    private static let team = TeamEntity(
        id: "t1", name: "Acme", slug: "acme", iconUrl: nil, createdAt: "", updatedAt: ""
    )

    var body: some View {
        VStack(spacing: 18) {
            bar(chats: true)
            bar(team: true)
            bar(board: true)
        }
        .padding(.horizontal, -16)
        .accessibilityIdentifier("phone-tab-bar-specimen")
    }

    private func bar(board: Bool = false, chats: Bool = false, team: Bool = false) -> some View {
        MobileTabBar(
            agentActive: false,
            boardActive: board,
            devicesActive: false,
            chatsActive: chats,
            teamActive: team,
            team: Self.team,
            chatsRunning: true,
            chatsNeedYou: true,
            composeEnabled: true,
            onBoard: {}, onDevices: {}, onChats: {}, onTeam: {}, onCompose: {}, onChat: {}
        )
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
            menu("Spike: offline queue", Chats.threadMenuEntries(rootStatus: "running", canSteer: true).map { .thread($0) })
            menu("#APP-16 Fix flaky scroll restore", Chats.threadMenuEntries(rootStatus: "ended", canSteer: true).map { .thread($0) })
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
            ForEach(entries, id: \.self) { entry in
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
