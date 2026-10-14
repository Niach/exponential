import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295/1316: the thread header's ONE strip as the phone's pager tabs
/// (fixture `session-thread.json` `tabs`), in the contract's order:
/// `Conversation` · one CHIP per linked issue (status glyph + mono identifier
/// in a bordered capsule) · the "+" (link or file an issue) · one chip per
/// session at the viewed level (its row mark + name; a tap opens that
/// session, phones push it) · `Guide` (a green dot while the root's Guide
/// holds anything), scrolling sideways when it outgrows the width. Every
/// chip is ONE height (entity-chip.json tab size); the shown page is the
/// filled capsule. A long press on an issue chip opens its menu (Unlink). An
/// issue linked while the screen was up fades in with a ring.
struct ThreadStripTabs: View {
    let tabs: [ThreadTab]
    let selection: ThreadPage
    /// The synced issue behind a tab (its status glyph); nil = anchor default.
    var issue: (String) -> IssueEntity? = { _ in nil }
    /// Whether an issue tab was just linked (the fade-in ring, once).
    var isFresh: (String) -> Bool = { _ in false }
    let onSelect: (ThreadPage) -> Void
    /// A long press on an issue tab.
    var onIssueMenu: (String) -> Void = { _ in }
    /// The "+" (nil = hidden: a reader who cannot add to the thread).
    var onAdd: (() -> Void)?
    /// A session chip's tap (nil = session chips are not shown).
    var onOpenSession: ((String) -> Void)?

    /// entity-chip.json `tab` size: every chip in the strip is one height.
    static let chipHeight: CGFloat = 32

    var body: some View {
        ScrollViewReader { scroller in
            ScrollView(.horizontal) {
                HStack(spacing: 4) {
                    ForEach(Array(tabs.enumerated()), id: \.offset) { _, tab in
                        item(tab)
                    }
                }
                .padding(.horizontal, 8)
            }
            .scrollIndicators(.hidden)
            .onChange(of: selection) { _, next in
                scroller.scrollTo(next, anchor: .center)
            }
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.tabsTestId)
    }

    @ViewBuilder
    private func item(_ tab: ThreadTab) -> some View {
        switch tab {
        case .conversation:
            pageTab(.conversation).id(ThreadPage.conversation)
        case let .issue(issueId, _):
            pageTab(.issue(issueId)).id(ThreadPage.issue(issueId))
        case .guide:
            pageTab(.guide).id(ThreadPage.guide)
        case .add:
            if let onAdd {
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
        case let .session(sessionId, name, markState):
            if let onOpenSession {
                ThreadStripTab(selected: false, bordered: true, fresh: false) {
                    HStack(spacing: 5) {
                        ThreadSessionChipMark(state: markState)
                            .frame(width: 13, height: 13)
                        Text(name)
                            .font(.footnote.weight(.medium))
                            .lineLimit(1)
                    }
                }
                .onTapGesture { onOpenSession(sessionId) }
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(.isButton)
                .accessibilityLabel("\(name), \(markState.word)")
                .accessibilityIdentifier("thread-tab-session-\(sessionId)")
            }
        }
    }

    @ViewBuilder
    private func pageTab(_ page: ThreadPage) -> some View {
        let selected = page == selection
        switch page {
        case let .issue(issueId):
            let identifier = tabs.lazy.compactMap { tab -> String? in
                if case let .issue(id, identifier) = tab, id == issueId { return identifier }
                return nil
            }.first ?? ""
            ThreadStripTab(selected: selected, bordered: true, fresh: isFresh(issueId)) {
                HStack(spacing: 5) {
                    let status = IssueStatus.from(issue(issueId)?.status)
                    AppIcon(status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(status.color)
                    Text(identifier)
                        .font(.system(.footnote, design: .monospaced))
                }
            }
            .onTapGesture { onSelect(page) }
            .onLongPressGesture(minimumDuration: 0.4) { onIssueMenu(issueId) }
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel("\(identifier) \(issue(issueId)?.title ?? "")")
            .accessibilityAction(named: "More") { onIssueMenu(issueId) }
            .accessibilityIdentifier("thread-tab-\(identifier)")
        case .guide:
            let dot = tabs.contains(.guide(dot: true))
            ThreadStripTab(selected: selected, bordered: false, fresh: false) {
                HStack(spacing: 5) {
                    Text(SessionTree.ThreadCopy.guide)
                        .font(.footnote.weight(.medium))
                    if dot {
                        Circle()
                            .fill(DesignTokens.Semantic.green)
                            .frame(width: 6, height: 6)
                            .accessibilityHidden(true)
                    }
                }
            }
            .onTapGesture { onSelect(page) }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("thread-tab-guide")
        case .conversation:
            ThreadStripTab(selected: selected, bordered: false, fresh: false) {
                Text(SessionTree.ThreadCopy.conversation)
                    .font(.footnote.weight(.medium))
            }
            .onTapGesture { onSelect(page) }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("thread-tab-conversation")
        }
    }
}

/// A session chip's lead (`sessionTabMarkState` ×4): the working mark while
/// it works, the brand mark amber-badged when it needs you, the bare mark
/// while it waits, dimmed once it ended.
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
        case .ended:
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
                    Capsule(style: .continuous)
                        .fill(GlassTokens.fillCard)
                        .overlay(
                            Capsule(style: .continuous)
                                .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                        )
                } else if bordered {
                    Capsule(style: .continuous)
                        .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                }
            }
            .overlay {
                if fresh {
                    Capsule(style: .continuous)
                        .strokeBorder(DesignTokens.Semantic.blue, lineWidth: 1)
                        .opacity(ringed ? 0 : 1)
                }
            }
            .opacity(fresh && !appeared ? 0 : 1)
            .contentShape(Capsule())
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

/// EXP-1295/1316: the header's title — the nav row's principal item: a
/// thread's title on ONE line (never tappable, nobody renames a thread) with
/// the lock glyph when it is private (`!root.shared`), or an agent's icon in
/// its color before its name (the agent channel). Nothing else: no caption.
struct ThreadHeaderTitle: View {
    let title: String
    var isPrivate = false
    /// An agent channel's facets (registry icon name + `#rrggbb`).
    var agentIcon: String?
    var agentColor: String?

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            if let agentIcon {
                AppIcon(agentIcon, size: 17, weight: .medium)
                    .foregroundStyle(Color(hex: agentColor) ?? .white)
                    .accessibilityHidden(true)
            }
            Text(title)
                .font(.headline)
                .foregroundStyle(.white)
                .lineLimit(1)
                .truncationMode(.tail)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("thread-title")
            if isPrivate {
                AppIcon(AppIcons.uiPrivate, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityLabel("Private")
                    .accessibilityIdentifier("thread-private")
            }
        }
    }
}
