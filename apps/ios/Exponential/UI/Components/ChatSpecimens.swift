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
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    HStack(spacing: 10) {
                        ChatsView.headerCircle(AppIcons.navSearch, dot: false)
                        ChatsView.headerCircle(AppIcons.navInbox, dot: true)
                    }
                }
                .ownChrome()
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

/// `chat-transcript`: the phone's chat bubbles, the divider pills, the
/// superseded plan line and the inline surface card's frame.
struct ChatTranscriptSpecimens: View {
    @Environment(AppDependencies.self) private var deps

    private var context: AgentMarkdownContext {
        AgentMarkdownContext(baseURL: nil, accountId: "", httpClient: deps.httpClient)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            UserMessageBubble(
                text: "Triage everything that came in from the widget today.",
                context: context,
                caption: "18:10"
            )
            AgentChatBubble(time: "18:14") {
                AgentMarkdownText(
                    text: "Filed 6 issues on Product Feedback, merged 2 duplicates into #FB-3.",
                    context: context,
                    options: [.hardLineBreaks]
                )
            }
            TranscriptDividerPill(
                icon: AppIcons.codingCompact,
                text: "\(Chats.Copy.Dividers.compact) · 18:20"
            )
            UserMessageBubble(text: "Anything new overnight?", context: context, caption: "09:02")
            AgentChatBubble(time: "09:03") {
                AgentMarkdownText(
                    text: "Three reports, all about the iPad sign-in. I linked them to #APP-19.",
                    context: context,
                    options: [.hardLineBreaks]
                )
            }
            HStack(spacing: 8) {
                AppIcon(AppIcons.codingPlan, size: 13)
                Text(Chats.Copy.superseded).font(.footnote)
            }
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            HStack(spacing: 0) {
                SurfaceCardFrame(title: "Cold start, Pixel 6a", animate: false) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("740 ms").font(.title2.weight(.semibold)).foregroundStyle(.white)
                        Text("−47 %").font(.footnote).foregroundStyle(DesignTokens.Semantic.green)
                    }
                }
                Spacer(minLength: ChatBubbleMetrics.agentTrailingGap)
            }
            TranscriptDividerPill(icon: AppIcons.uiClean, text: "\(Chats.Copy.Dividers.new) · 09:30")
        }
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
