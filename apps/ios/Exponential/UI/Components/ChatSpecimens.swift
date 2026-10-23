import ExpCore
import ExpUI
import SwiftUI

// Thread 58cb32df (chats.json): the iOS styleguide's chat entries (web
// `apps/styleguide` entries `chat-list`, `phone-tab-bar`, `chat-transcript`,
// `chat-menus`, the updated `schedule-picker`): the REAL views over specimen
// values. Capture only: `-uiTestingStyleguide <id>` (`ThreadCardsScreen`).

/// `chat-list`: the Chats header's bubbles, the filter chips, the flat rows.
struct ChatListSpecimens: View {
    private static let bubbles = [
        AgentBubbleItem(id: "a1", name: "Triage", icon: AppIcons.uiWarning, color: Color(hex: "#a855f7") ?? .purple, ring: .needsYou),
        AgentBubbleItem(id: "a2", name: "Designer", icon: AppIcons.editorImage, color: Color(hex: "#ec4899") ?? .pink, ring: .unread),
        AgentBubbleItem(id: "a3", name: "Release", icon: AppIcons.settingsAgents, color: Color(hex: "#22c55e") ?? .green, ring: .none),
        AgentBubbleItem(id: "a4", name: "Support", icon: AppIcons.settingsAgents, color: Color(hex: "#0ea5e9") ?? .blue, ring: .unread),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            AgentBubbles(items: Self.bubbles, onTap: { _ in }, onHold: { _ in })
                .padding(.horizontal, -16)
            HStack(spacing: 8) {
                GlassPill(Chats.Copy.Filters.all, size: .md, mode: .select(isSelected: true) {})
                GlassPill("\(Chats.Copy.Filters.needsYou) 1", size: .md, mode: .select(isSelected: false) {})
                GlassPill("\(Chats.Copy.Filters.running) 2", size: .md, mode: .select(isSelected: false) {})
            }
            VStack(spacing: 0) {
                ChatRowView(
                    title: "#APP-5 Reduce cold start below 800 ms",
                    time: "21:40",
                    preview: "Question: Lazy-load the editor or ship a snapshot?",
                    previewLabel: "Question:",
                    needsYou: true,
                    unreadCount: 1
                ) { ChatLeadMark(lead: .needsYou) }
                ChatRowView(
                    title: "Spike: offline queue",
                    time: "21:38",
                    preview: Chats.Copy.Preview.working,
                    previewItalic: true
                ) { ChatLeadMark(lead: .working) }
                ChatRowView(
                    title: "#APP-14 Group board issues by assignee",
                    time: "20:32",
                    preview: "Opened PR #212, waiting for review",
                    unreadCount: 3,
                    word: ThreadRowWord.stack.text,
                    wordIcon: AppIcons.prStack
                ) { ChatLeadMark(lead: .open) }
                ChatRowView(
                    title: "#APP-16 Fix flaky scroll restore",
                    time: "Yesterday",
                    preview: "Merged. Restores the offset on back."
                ) { ChatLeadMark(lead: .merged) }
            }
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
            bar(board: true)
            bar(team: true)
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
