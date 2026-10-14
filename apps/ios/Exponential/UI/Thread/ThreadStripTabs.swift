import ExpCore
import ExpUI
import SwiftUI

/// EXP-1295: the thread header's ONE strip as the phone's pager tabs
/// (fixture `session-thread.json` `tabs`): `Conversation`, one tab per
/// linked issue (status glyph + mono identifier), `Guide` (a green dot while
/// the root's Guide holds anything), scrolling sideways when it outgrows the
/// width, with the "+" (link or file an issue) as a trailing button outside
/// the scroller. The shown tab is the filled capsule. A long press on an
/// issue tab opens its menu (Unlink). An issue linked while the screen was
/// up fades in with a ring.
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

    var body: some View {
        HStack(spacing: 4) {
            ScrollViewReader { scroller in
                ScrollView(.horizontal) {
                    HStack(spacing: 2) {
                        ForEach(pages, id: \.self) { page in
                            tab(page)
                                .id(page)
                        }
                    }
                    .padding(.leading, 8)
                }
                .scrollIndicators(.hidden)
                .onChange(of: selection) { _, next in
                    scroller.scrollTo(next, anchor: .center)
                }
            }
            if let onAdd, tabs.contains(.add) {
                Button(action: onAdd) {
                    AppIcon(AppIcons.uiAdd, size: AppIcon.Size.medium, weight: .medium)
                        .foregroundStyle(.white.opacity(TextOpacity.secondary))
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Add an issue")
                .accessibilityIdentifier("thread-strip-add")
                .padding(.trailing, 8)
            }
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.tabsTestId)
    }

    /// The pages in strip order (the "+" is a button, never a page).
    private var pages: [ThreadPage] {
        tabs.compactMap { tab in
            switch tab {
            case .conversation: .conversation
            case let .issue(issueId, _): .issue(issueId)
            case .guide: .guide
            case .add: nil
            }
        }
    }

    @ViewBuilder
    private func tab(_ page: ThreadPage) -> some View {
        let selected = page == selection
        switch page {
        case let .issue(issueId):
            let identifier = tabs.lazy.compactMap { tab -> String? in
                if case let .issue(id, identifier) = tab, id == issueId { return identifier }
                return nil
            }.first ?? ""
            ThreadStripTab(selected: selected, fresh: isFresh(issueId)) {
                HStack(spacing: 4) {
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
            ThreadStripTab(selected: selected, fresh: false) {
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
            ThreadStripTab(selected: selected, fresh: false) {
                Text(SessionTree.ThreadCopy.conversation)
                    .font(.footnote.weight(.medium))
            }
            .onTapGesture { onSelect(page) }
            .accessibilityAddTraits(.isButton)
            .accessibilityIdentifier("thread-tab-conversation")
        }
    }
}

/// One tab: the filled capsule when shown, plain text otherwise; a FRESH tab
/// fades in (motion tokens, decelerate) with a ring that settles away.
private struct ThreadStripTab<Content: View>: View {
    let selected: Bool
    let fresh: Bool
    @ViewBuilder let content: () -> Content

    @Environment(\.motion) private var motion
    @State private var appeared = false
    @State private var ringed = false

    var body: some View {
        content()
            .foregroundStyle(.white.opacity(selected ? TextOpacity.primary : TextOpacity.secondary))
            .padding(.horizontal, 8)
            .padding(.vertical, 7)
            .background {
                if selected {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .fill(GlassTokens.fillCard)
                        .overlay(
                            RoundedRectangle(cornerRadius: 9, style: .continuous)
                                .strokeBorder(GlassTokens.strokeCard, lineWidth: GlassTokens.hairline)
                        )
                }
            }
            .overlay {
                if fresh {
                    RoundedRectangle(cornerRadius: 9, style: .continuous)
                        .strokeBorder(DesignTokens.Semantic.blue, lineWidth: 1)
                        .opacity(ringed ? 0 : 1)
                }
            }
            .opacity(fresh && !appeared ? 0 : 1)
            .contentShape(Rectangle())
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

/// EXP-1295: the header's title row — a thread's title with the lock glyph
/// when it is private (`!root.shared`), or an agent's icon in its color
/// before its name (the agent channel). Nothing else: no caption.
struct ThreadHeaderTitle: View {
    let title: String
    var isPrivate = false
    /// An agent channel's facets (registry icon name + `#rrggbb`).
    var agentIcon: String?
    var agentColor: String?

    var body: some View {
        HStack(alignment: .center, spacing: 8) {
            if let agentIcon {
                AppIcon(agentIcon, size: 20, weight: .medium)
                    .foregroundStyle(Color(hex: agentColor) ?? .white)
                    .accessibilityHidden(true)
            }
            Text(title)
                .font(.title3.weight(.semibold))
                .foregroundStyle(.white)
                .lineLimit(2)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("thread-title")
            if isPrivate {
                AppIcon(AppIcons.uiPrivate, size: 13)
                    .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                    .accessibilityLabel("Private")
                    .accessibilityIdentifier("thread-private")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
    }
}
