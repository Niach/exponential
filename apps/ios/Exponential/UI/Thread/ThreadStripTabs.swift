import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1433 (session-thread.json STRIP): everything the strip's ONE group
/// tab holds — every issue of the tree (a chip's sub-issues after it) and
/// every session under the root (THE tree rows, `depth` 0 = a direct child).
/// THE Menu's sheet lists the issue rows, a separator, then the session rows.
struct ThreadStripGroup: Equatable {
    struct Issue: Equatable, Identifiable {
        let issueId: String
        let identifier: String
        var id: String { issueId }
    }

    struct Session: Equatable, Identifiable {
        let sessionId: String
        let name: String
        let markState: SessionState
        let depth: Int
        var id: String { sessionId }
    }

    let issues: [Issue]
    let sessions: [Session]

    /// The strip's tabs flattened (`sessions` = false: a host that shows no
    /// session pages).
    init(tabs: [ThreadTab], sessions withSessions: Bool = true) {
        var issues: [Issue] = []
        var sessions: [Session] = []
        for tab in tabs {
            switch tab {
            case let .issue(issueId, identifier, nested):
                issues.append(Issue(issueId: issueId, identifier: identifier))
                issues += nested.map { Issue(issueId: $0.issueId, identifier: $0.identifier) }
            case let .session(sessionId, name, markState, nested) where withSessions:
                sessions.append(Session(sessionId: sessionId, name: name, markState: markState, depth: 0))
                sessions += nested.map {
                    Session(sessionId: $0.sessionId, name: $0.name, markState: $0.markState, depth: $0.depth)
                }
            default:
                break
            }
        }
        self.issues = issues
        self.sessions = sessions
    }

    var mode: ThreadStripMode {
        SessionTree.threadStripMode(issues: issues.count, sessions: sessions.count)
    }

    /// The chips mode's one issue (nil in group mode, or with no issue).
    var single: Issue? { mode.mode == .chips ? issues.first : nil }

    /// Whether `page` is one of the group's items.
    func holds(_ page: ThreadPage) -> Bool {
        switch page {
        case let .issue(id): issues.contains { $0.issueId == id }
        case let .session(id): sessions.contains { $0.sessionId == id }
        default: false
        }
    }

    /// The open item's label on the group tab: the identifier, or the
    /// session's name; nil = nothing of the group is open.
    func openLabel(_ page: ThreadPage) -> String? {
        switch page {
        case let .issue(id): issues.first { $0.issueId == id }?.identifier
        case let .session(id): sessions.first { $0.sessionId == id }?.name
        default: nil
        }
    }
}

/// EXP-1295/1316/1334/#EXP-1433: the thread header's ONE strip as the
/// phone's pager tabs, and it NEVER scrolls: the "+" (link or file an issue)
/// and `Conversation` (ICON-ONLY on phones, `action-chat`) pinned left,
/// #EXP-1662 the plan chip right after it while the thread holds a plan;
/// stories.json: the Stories tab (icon-only, `thread-stories`, a dot while
/// `storyTab.unseen`) pinned right. Between them `threadStripMode`: at most
/// ONE issue and no session = that issue's chip (status glyph + mono
/// identifier, the pager's short chip; a long press opens THE Menu: Link an
/// issue · New issue · Unlink); anything more = ONE group tab (the
/// `session-tree` glyph + the count muted; ACTIVE with the open item's label
/// while one of its items is the shown page) whose tap opens THE Menu's
/// sheet (`ThreadStripGroupSheet`). Every chip is ONE height (entity-chip.json
/// tab size); the shown page is the filled chip. An issue linked while the
/// screen was up fades in with a ring.
struct ThreadStripTabs: View {
    let tabs: [ThreadTab]
    let selection: ThreadPage
    /// The synced issue behind a tab (its title).
    var issue: (String) -> IssueEntity? = { _ in nil }
    /// S-3: a tab's status, resolved through the team's statuses; nil = no
    /// glyph (an unresolved issue never reads as Backlog).
    var issueStatus: (String) -> ResolvedIssueStatus? = { _ in nil }
    /// Whether an issue tab was just linked (the fade-in ring, once).
    var isFresh: (String) -> Bool = { _ in false }
    let onSelect: (ThreadPage) -> Void
    /// A long press on the issue chip.
    var onIssueMenu: (String) -> Void = { _ in }
    /// The "+" (nil = hidden: a reader who cannot add to the thread).
    var onAdd: (() -> Void)?
    /// Whether the strip holds the sessions under the root (false = an
    /// issue-only strip).
    var showsSessions = true
    /// The group tab's tap: the host presents THE Menu's sheet.
    var onOpenGroup: () -> Void = {}
    /// stories.json STORIES TAB: the Stories tab pinned right (nil = none,
    /// an app editor's chat); true = its dot (`storyTab(input).unseen`).
    var storiesDot: Bool?
    /// #EXP-1562 (waiting-cards.json CHIPS): the waiting cards; phones draw
    /// ONE button right after the Conversation tab while one waits.
    var waiting: WaitingCards.Chips?
    /// The waiting button's tap: the host presents THE Menu's sheet.
    var onOpenWaiting: () -> Void = {}
    /// #EXP-1662 (plan-page.json): the thread holds a plan (nil = no chip);
    /// true = its plan waits (the needs-you dot).
    var planWaiting: Bool?

    /// entity-chip.json `tab` size: every chip in the strip is one height —
    /// web `STRIP_TAB_CLASS` `h-8 rounded-md`: 32pt tall, the 10pt row radius
    /// (`GlassTokens.rowRadius`), a rounded rectangle, not a capsule.
    static let chipHeight: CGFloat = 32
    static let chipShape = RoundedRectangle(cornerRadius: GlassTokens.rowRadius, style: .continuous)
    /// The strip's one gap.
    static let gap: CGFloat = 4

    private var group: ThreadStripGroup { ThreadStripGroup(tabs: tabs, sessions: showsSessions) }

    var body: some View {
        let group = group
        HStack(spacing: Self.gap) {
            if tabs.contains(.add), let onAdd {
                addButton(onAdd)
            }
            // NEW-46: always there (a one-session thread's session page).
            if tabs.contains(.conversation) {
                pageTab(.conversation)
                    .fixedSize()
            }
            if let planWaiting {
                planChip(waiting: planWaiting)
            }
            if let waiting, waiting.layout == .button {
                waitingButton(waiting.count)
            }
            HStack(spacing: Self.gap) {
                if let single = group.single {
                    issueChip(single)
                } else if group.mode.mode == .group {
                    groupTab(group)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: Self.chipHeight)
            if storiesDot != nil {
                pageTab(.stories)
                    .fixedSize()
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.tabsTestId)
    }

    // MARK: - Chips

    private func addButton(_ onAdd: @escaping () -> Void) -> some View {
        Button(action: onAdd) {
            AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium, weight: .medium)
                .foregroundStyle(.white.opacity(TextOpacity.secondary))
                .frame(width: Self.chipHeight, height: Self.chipHeight)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Add an issue")
        .accessibilityIdentifier("thread-strip-add")
    }

    /// The chips mode's ONE issue chip: status glyph · mono identifier (the
    /// phone pager's short chip, web `short`; the title is its accessible
    /// name).
    private func issueChip(_ single: ThreadStripGroup.Issue) -> some View {
        let issueId = single.issueId
        return ThreadStripTab(selected: selection == .issue(issueId), bordered: true, fresh: isFresh(issueId)) {
            HStack(spacing: ThreadStripMetrics.innerSpacing) {
                if let status = issueStatus(issueId) {
                    AppIcon(status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(status.color)
                }
                Text(single.identifier)
                    .font(ThreadStripMetrics.identifierFont)
                    .fixedSize()
            }
        }
        .onTapGesture { onSelect(.issue(issueId)) }
        .onLongPressGesture(minimumDuration: 0.4) { onIssueMenu(issueId) }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel("\(single.identifier) \(issue(issueId)?.title ?? "")")
        .accessibilityAction(named: "More") { onIssueMenu(issueId) }
        .accessibilityIdentifier("thread-tab-\(single.identifier)")
    }

    /// #EXP-1662: the plan tab on a phone: the `coding-plan` glyph, `copy.title`
    /// and the 6 pt needs-you dot while its plan waits.
    private func planChip(waiting: Bool) -> some View {
        var selected = false
        if case .plan = selection { selected = true }
        return ThreadStripTab(selected: selected, bordered: true, fresh: false) {
            HStack(spacing: ThreadStripMetrics.innerSpacing) {
                AppIcon(AppIcons.codingPlan, size: AppIcon.Size.small, weight: .medium)
                Text(PlanPage.Copy.title)
                    .font(ThreadStripMetrics.nameFont)
                    .lineLimit(1)
                if waiting {
                    Circle()
                        .fill(DesignTokens.Semantic.yellow)
                        .frame(width: 6, height: 6)
                }
            }
        }
        .fixedSize()
        .onTapGesture { onSelect(.plan(nil)) }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(waiting ? "\(PlanPage.Copy.title), \(PlanPage.Copy.needsYou)" : PlanPage.Copy.title)
        .accessibilityIdentifier(PlanPage.TestIds.tab)
    }

    /// #EXP-1562: THE waiting button — ONE icon-only chip, the question
    /// tint's `ui-help` with the count muted beside it; a tap opens THE
    /// Menu's sheet (`ThreadWaitingSheet`).
    private func waitingButton(_ count: Int) -> some View {
        ThreadStripTab(selected: false, bordered: true, fresh: false) {
            HStack(spacing: ThreadStripMetrics.innerSpacing) {
                AppIcon(
                    AppIcons.conceptOrName(WaitingCards.Icons.waiting) ?? AppIcons.uiHelp,
                    size: AppIcon.Size.small, weight: .medium
                )
                .foregroundStyle(WaitingCardTint.color(.question))
                Text("\(count)")
                    .font(ThreadStripMetrics.nameFont.monospacedDigit())
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
            }
        }
        .fixedSize()
        .onTapGesture { onOpenWaiting() }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(WaitingCards.waitingCountLabel(count))
        .accessibilityIdentifier(WaitingCards.TestIds.button)
    }

    /// THE group tab: the `session-tree` glyph and the count (muted) at
    /// rest; ACTIVE with the open item's label (truncated at
    /// `stripGroupMaxWidth`) while one of its items is the shown page.
    private func groupTab(_ group: ThreadStripGroup) -> some View {
        let label = group.openLabel(selection)
        let count = group.mode.count
        return ThreadStripTab(selected: label != nil, bordered: true, fresh: false) {
            HStack(spacing: ThreadStripMetrics.innerSpacing) {
                AppIcon(AppIcons.sessionTree, size: AppIcon.Size.small, weight: .medium)
                if let label {
                    Text(label)
                        .font(ThreadStripMetrics.nameFont)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(width: ThreadStripMetrics.groupLabelWidth(label), alignment: .leading)
                        .accessibilityIdentifier("thread-strip-group-label")
                } else {
                    Text("\(count)")
                        .font(ThreadStripMetrics.nameFont.monospacedDigit())
                        .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                        .accessibilityIdentifier("thread-strip-group-count")
                }
            }
        }
        .fixedSize()
        .onTapGesture { onOpenGroup() }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(
            label.map { "\(SessionTree.ThreadCopy.stripGroup): \($0)" }
                ?? "\(SessionTree.ThreadCopy.stripGroup) (\(count))"
        )
        .accessibilityIdentifier(SessionTree.ThreadCopy.groupTestId)
    }

    @ViewBuilder
    private func pageTab(_ page: ThreadPage) -> some View {
        let selected = page == selection
        switch page {
        case .issue, .session, .plan:
            EmptyView()
        case .stories:
            // Phones: icon-only like Conversation (the word = the label),
            // the Guide's emerald dot while a story is waiting or unseen.
            ThreadStripTab(selected: selected, bordered: false, fresh: false) {
                AppIcon(AppIcons.threadStories, size: ThreadStripMetrics.pinnedIconSize, weight: .medium)
                    .overlay(alignment: .topTrailing) {
                        if storiesDot == true {
                            Circle()
                                .fill(DesignTokens.Semantic.green)
                                .frame(width: 6, height: 6)
                                .offset(x: 4, y: -3)
                                .accessibilityHidden(true)
                        }
                    }
            }
            .onTapGesture { onSelect(page) }
            .accessibilityElement(children: .ignore)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel(Stories.Tab.Copy.label)
            .accessibilityIdentifier(Stories.Tab.TestIds.tab)
        case .conversation:
            ThreadStripTab(selected: selected, bordered: false, fresh: false) {
                AppIcon(AppIcons.actionChat, size: ThreadStripMetrics.pinnedIconSize, weight: .medium)
            }
            .onTapGesture { onSelect(page) }
            .accessibilityElement(children: .ignore)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel(SessionTree.ThreadCopy.conversation)
            .accessibilityIdentifier("thread-tab-conversation")
        }
    }
}

/// The strip's type and glyph metrics.
enum ThreadStripMetrics {
    static let innerSpacing: CGFloat = 5
    /// The icon-only Conversation / Stories tabs (phones).
    static let pinnedIconSize: CGFloat = 16
    static let identifierFont = Font.system(.footnote, design: .monospaced)
    static let nameFont = Font.footnote.weight(.medium)

    /// The group tab's open label: its own width, capped at
    /// `stripGroupMaxWidth` (truncated past it).
    static func groupLabelWidth(_ label: String) -> CGFloat {
        let font = UIFont.systemFont(ofSize: UIFont.preferredFont(forTextStyle: .footnote).pointSize, weight: .medium)
        let width = ceil((label as NSString).size(withAttributes: [.font: font]).width) + 1
        return min(width, CGFloat(SessionTree.ThreadCopy.stripGroupMaxWidth))
    }
}

/// A session chip's lead (`sessionTabMarkState` ×4): the working mark while
/// it works, the brand mark amber-badged when it needs you, the bare mark
/// while idle, dimmed once done (S-3).
struct ThreadSessionChipMark: View {
    let state: SessionState

    var body: some View {
        switch state {
        case .working:
            AgentWorkingMark(agent: nil)
        case .needsYou:
            BeatingAgentMark(agent: nil, beating: false)
                .overlay(alignment: .topTrailing) {
                    Circle()
                        .fill(DesignTokens.Semantic.yellow)
                        .frame(width: 6, height: 6)
                        .offset(x: 2, y: -2)
                }
        case .done:
            BeatingAgentMark(agent: nil, beating: false)
                .opacity(0.5)
        default:
            BeatingAgentMark(agent: nil, beating: false)
        }
    }
}

/// One tab at the strip's ONE height: the filled capsule when shown; a chip
/// (`bordered`: an issue or a session) keeps its hairline capsule otherwise,
/// a word tab is plain text. A FRESH tab fades in (motion tokens,
/// decelerate) with a ring that settles away.
private struct ThreadStripTab<Content: View>: View {
    let selected: Bool
    let bordered: Bool
    let fresh: Bool
    @ViewBuilder let content: () -> Content

    @Environment(\.motion) private var motion
    @State private var appeared = false
    @State private var ringed = false

    var body: some View {
        content()
            .foregroundStyle(.white.opacity(selected ? TextOpacity.primary : TextOpacity.secondary))
            .padding(.horizontal, 10)
            .frame(height: ThreadStripTabs.chipHeight)
            .background {
                if selected {
                    ThreadStripTabs.chipShape
                        .fill(GlassTokens.fillCard)
                        .overlay(
                            ThreadStripTabs.chipShape
                                .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                        )
                } else if bordered {
                    ThreadStripTabs.chipShape
                        .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                }
            }
            .overlay {
                if fresh {
                    ThreadStripTabs.chipShape
                        .strokeBorder(DesignTokens.Semantic.blue, lineWidth: 1)
                        .opacity(ringed ? 0 : 1)
                }
            }
            .opacity(fresh && !appeared ? 0 : 1)
            .contentShape(ThreadStripTabs.chipShape)
            .onAppear {
                guard fresh else { return }
                withAnimation(motion.decelerate(DesignTokens.Motion.Duration.slow)) { appeared = true }
                Task { @MainActor in
                    try? await Task.sleep(for: .seconds(2))
                    withAnimation(motion.standard) { ringed = true }
                }
            }
    }
}

/// EXP-1373 (chats.json, the chat screen on phones): the header's leading
/// block — the lead DISC (the agent's icon in its colour, else the run mark:
/// amber-badged while it needs you, the working mark while busy) and the
/// title over ONE muted subtitle (the state, the device, the linked issues,
/// the PR; an agent chat: `Agent · <model>`).
struct ChatHeaderTitle: View {
    enum Tone { case muted, live }

    let title: String
    var subtitle: String?
    var subtitleTone: Tone = .muted
    var isPrivate = false
    var agentIcon: String?
    var agentColor: String?
    /// #EXP-1448: the viewer's Assistant wears the brand mark (chats.json),
    /// never its stored icon.
    var assistant = false
    var cli: String?
    /// The root's PR states: the lead's PR glyph while nothing works.
    var prStates: [String] = []
    var mark: ThreadRowMark = .waiting

    /// EXP-1552: THE chat lead ×4 (web `ChatLeadDisc`, Android
    /// `chatLeadOf`, the Chats list's `ChatLeadMark`): the mark first, then
    /// the root's PR glyph, else the bare chat glyph.
    private var lead: Chats.ChatLead {
        switch mark {
        case .needsYou: .needsYou
        case .working: .working
        case .waiting:
            switch SessionTree.threadRowLeadPr(prStates: prStates) {
            case .open: .open
            case .merged: .merged
            case nil: .waiting
            }
        }
    }

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle().fill(DesignTokens.Palette.muted)
                if assistant {
                    ExpLogoMark(size: 18, color: .white)
                } else if let agentIcon {
                    AppIcon(agentIcon, size: 18, weight: .medium)
                        .foregroundStyle(Color(hex: agentColor) ?? .white)
                } else {
                    ChatLeadMark(lead: lead, cli: cli, size: 18)
                }
            }
            .frame(width: 36, height: 36)
            .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 4) {
                    Text(title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .accessibilityAddTraits(.isHeader)
                        .accessibilityIdentifier("thread-title")
                    if isPrivate {
                        AppIcon(AppIcons.uiPrivate, size: 11)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .accessibilityLabel("Private")
                            .accessibilityIdentifier("thread-private")
                    }
                }
                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.caption)
                        .foregroundStyle(subtitleTone == .live
                            ? DesignTokens.Semantic.yellow
                            : .white.opacity(TextOpacity.secondary))
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .accessibilityIdentifier("thread-subtitle")
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private static func live(_ rows: ThreadRows) -> [SessionEntity] {
        rows.sessions.filter { $0.status != DomainContract.sessionStatusEnded }
    }

    /// The subtitle's parts, ` · `-joined.
    static func subtitle(rows: ThreadRows) -> String {
        if rows.isAgent {
            return Chats.agentChatSubtitle(modelLabel: rows.root.model.map(LaunchVocabulary.modelLabel))
        }
        let live = live(rows)
        let state: String? = if live.contains(where: \.needsInput) {
            ThreadRowWord.needsInput.text
        } else if live.contains(where: \.busy) {
            Chats.Copy.Preview.working
        } else {
            nil
        }
        let issues: String? = rows.issues.first.flatMap { first in
            first.identifier.map { rows.issues.count > 1 ? "\($0) +\(rows.issues.count - 1)" : $0 }
        }
        let pr = rows.pullRequests.last.map { "#\($0.number)" }
        return [state, rows.rootDevice?.label, issues, pr].compactMap { $0 }.joined(separator: " · ")
    }

    static func tone(rows: ThreadRows) -> Tone {
        let live = live(rows)
        return !rows.isAgent && live.contains(where: { $0.busy || $0.needsInput }) ? .live : .muted
    }
}
