import ExpCore
import ExpUI
import SwiftUI

/// #EXP-1732 (session-screen.json): the tree a session screen's Issues and
/// Sessions tabs list, flattened from the chat's tree (`SessionChatView.tabs`):
/// every issue of the tree (a sub-issue right after its parent, `depth` 0 =
/// a top-level linked issue) and every session under the root (THE tree rows,
/// `depth` 0 = a direct child; subagents included).
struct ChatTreeItems: Equatable {
    struct Issue: Equatable, Identifiable {
        let issueId: String
        let identifier: String
        let depth: Int
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

    init(tabs: [ChatTab]) {
        var issues: [Issue] = []
        var sessions: [Session] = []
        for tab in tabs {
            switch tab {
            case let .issue(issueId, identifier, nested):
                issues.append(Issue(issueId: issueId, identifier: identifier, depth: 0))
                issues += nested.map { Issue(issueId: $0.issueId, identifier: $0.identifier, depth: $0.depth) }
            case let .session(sessionId, name, markState, nested):
                sessions.append(Session(sessionId: sessionId, name: name, markState: markState, depth: 0))
                sessions += nested.map {
                    Session(sessionId: $0.sessionId, name: $0.name, markState: $0.markState, depth: $0.depth)
                }
            case .add, .conversation:
                break
            }
        }
        self.issues = issues
        self.sessions = sessions
    }

    /// #EXP-1732 (session-screen.json COUNTS): THE screen's Issues and
    /// Sessions from the RAW tree, every kind alike (an agent's chat too,
    /// whose view folds no chips): every session UNDER the root (all
    /// descendants, never the root; subagents included), depth from
    /// `parentId` (0 = a direct child), siblings in creation order; every
    /// issue of the tree (`rows.issues`), a sub-issue right after its
    /// nearest linked ancestor.
    init(rows: ChatRows, now: Date) {
        let rootId = rows.root.id
        let tree = rows.sessions.filter { $0.rootId == rootId && $0.id != rootId }
        let treeIds = Set(tree.map(\.id))
        let ordered = tree.sorted { a, b in a.createdAt != b.createdAt ? a.createdAt < b.createdAt : a.id < b.id }
        sessions = Self.forest(
            ordered,
            id: \.id,
            parent: { session in session.parentId.flatMap { $0 != rootId && treeIds.contains($0) ? $0 : nil } }
        ).map { session, depth in
            let tree = TreeSession(sessionRow: session)
            return Session(
                sessionId: session.id,
                name: SessionTree.sessionCard(tree, title: session.title ?? "", now: now).name,
                markState: SessionTree.sessionTabMarkState(tree),
                depth: depth
            )
        }
        let issueIds = Set(rows.issues.map(\.id))
        issues = Self.forest(
            rows.issues,
            id: \.id,
            parent: { issue in
                // The nearest LINKED ancestor (walking past unlinked ones).
                var seen = Set<String>()
                var parent = rows.issueParents[issue.id]
                while let current = parent, !seen.contains(current) {
                    if issueIds.contains(current) { return current }
                    seen.insert(current)
                    parent = rows.issueParents[current]
                }
                return nil
            }
        ).map { issue, depth in
            Issue(issueId: issue.id, identifier: issue.identifier ?? "", depth: depth)
        }
    }

    /// Depth-first over a forest: each top-level item (no parent in the set)
    /// in input order, its children right after it, `depth` 0 = top level.
    private static func forest<T>(
        _ items: [T], id: (T) -> String, parent: (T) -> String?
    ) -> [(T, Int)] {
        let keyed = items.map { (key: id($0), parent: parent($0), item: $0) }
        var children: [String: [Int]] = [:]
        var tops: [Int] = []
        for (index, entry) in keyed.enumerated() {
            if let parentId = entry.parent {
                children[parentId, default: []].append(index)
            } else {
                tops.append(index)
            }
        }
        var out: [(T, Int)] = []
        var visited = Set<Int>()
        // Iterative depth-first walk (an explicit stack, children in order).
        var stack: [(index: Int, depth: Int)] = []
        let starts = tops + keyed.indices.filter { !tops.contains($0) }
        for start in starts where !visited.contains(start) {
            stack.append((start, 0))
            while let next = stack.popLast() {
                let index = next.index, depth = next.depth
                guard !visited.contains(index) else { continue }
                visited.insert(index)
                out.append((keyed[index].item, depth))
                for child in (children[keyed[index].key] ?? []).reversed() where !visited.contains(child) {
                    stack.append((child, depth + 1))
                }
            }
        }
        return out
    }
}

/// #EXP-1732: THE header tabs under the chat header, on every kind: THE
/// segmented control (`GlassSegmentedControl`), its options the screen's
/// `tabs` in order, each the label followed by its count as a muted number
/// (none at 0; #EXP-1834: an empty counted tab is a disabled segment,
/// muted and not pressable). Labels only, never icons; each segment sized to its label;
/// past the phone's width it scrolls sideways (never wraps, never icon-only).
struct SessionScreenTabs: View {
    let tabs: [SessionScreen.TabEntry]
    let selection: SessionScreen.Tab
    let onSelect: (SessionScreen.Tab) -> Void

    /// TAB SIZING ×4: every segment HUGS its label (+ count), never equal
    /// widths and never squeezed; the strip leads and scrolls sideways once
    /// it overflows (no bounce while it fits).
    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            control
                .padding(.horizontal, 16)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 8)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionScreen.TestIds.tabs)
    }

    private func entry(_ id: SessionScreen.Tab) -> SessionScreen.TabEntry? {
        tabs.first { $0.id == id }
    }

    private var control: some View {
        GlassSegmentedControl(
            options: tabs.map(\.id),
            selection: selection,
            label: { entry($0)?.label ?? $0.label },
            identifier: { "\(SessionScreen.TestIds.tabs)-\($0.rawValue)" },
            accessory: { id in
                entry(id)?.count.map { count in
                    AnyView(
                        Text("\(count)")
                            .font(.subheadline.weight(.medium).monospacedDigit())
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                    )
                }
            },
            accessoryGap: 4,
            spokenLabel: { id in
                entry(id).flatMap { tab in tab.count.map { "\(tab.label) \($0)" } }
            },
            hugging: true,
            // #EXP-1834: an empty counted tab = THE disabled option.
            disabled: { entry($0)?.disabled ?? false },
            onSelect: onSelect
        )
    }
}

/// A session row's lead (`sessionTabMarkState` ×4): the working mark while
/// it works, the brand mark amber-badged when it needs you, the bare mark
/// while idle, dimmed once done (S-3).
struct ChatSessionMark: View {
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

/// #EXP-1732 the Issues tab's body: THE issue rows of the tree (status
/// glyph, `IDENT Title`; a sub-issue indented under its parent).
/// #EXP-1834: no `+` any more (Link or file an issue is the `…`'s entry).
/// A row opens that issue in place; a long press offers Unlink.
struct ChatIssuesTab: View {
    let issues: [ChatTreeItems.Issue]
    /// The synced issue behind a row (its title).
    var issue: (String) -> IssueEntity? = { _ in nil }
    /// S-3: a row's status through the team's statuses; nil = no glyph.
    var issueStatus: (String) -> ResolvedIssueStatus? = { _ in nil }
    let onOpen: (String) -> Void
    /// A row's long press (nil = none).
    var onMenu: ((String) -> Void)?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(issues) { row in
                    issueRow(row)
                        .padding(.leading, CGFloat(row.depth) * 14)
                }
            }
            .padding(.vertical, 8)
            .transcriptColumn()
        }
        .scrollIndicators(.hidden)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionScreen.TestIds.issues)
    }

    private func issueRow(_ row: ChatTreeItems.Issue) -> some View {
        let status = issueStatus(row.issueId)
        let title = issue(row.issueId)?.title ?? ""
        return GlassPickerSheetRow(
            item: PickerItem(
                value: row.issueId,
                label: title.isEmpty ? row.identifier : "\(row.identifier) \(title)",
                icon: status?.iconName,
                color: status?.color,
                keywords: [row.identifier, title]
            ),
            checked: .none,
            style: .check
        ) {
            onOpen(row.issueId)
        }
        .simultaneousGesture(LongPressGesture(minimumDuration: 0.4).onEnded { _ in onMenu?(row.issueId) })
        .accessibilityIdentifier("session-issues-row-\(row.identifier)")
    }
}

/// #EXP-1732 the Sessions tab's body: THE tree rows (`SessionRow` small +
/// `TreeGuides`, the mark state) of every session under the root; a row
/// opens that session in place.
struct ChatSessionsTab: View {
    let sessions: [ChatTreeItems.Session]
    let onOpen: (String) -> Void

    var body: some View {
        ScrollView {
            let guides = TreeGuides.compute(depths: sessions.map(\.depth))
            VStack(spacing: 0) {
                ForEach(Array(sessions.enumerated()), id: \.element.id) { index, row in
                    Button {
                        onOpen(row.sessionId)
                    } label: {
                        SessionRow(title: row.name, guide: guides[index], word: row.markState.word) {
                            ChatSessionMark(state: row.markState)
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("session-sessions-row-\(row.sessionId)")
                }
            }
            .padding(.vertical, 8)
            .transcriptColumn()
        }
        .scrollIndicators(.hidden)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionScreen.TestIds.sessions)
    }
}

/// #EXP-1732: an OPENED item's page on a phone (an issue, a session, a
/// plan: layout `item`, pushed over the tab): THE chat header with its `‹`
/// back (the pushed page's own) over the item's body.
struct SessionScreenItemPage<Content: View>: View {
    let title: String
    @ViewBuilder let content: () -> Content

    var body: some View {
        ZStack {
            AppBackground()
            // #EXP-1759: the header OWNS its band. As a safe-area inset over
            // the body, the body's feed scrolled up beneath it, and a pinned
            // feed's rows (real UIKit scroll content) won the hit test over
            // the header's SwiftUI buttons: XCUITest's (and VoiceOver's) hit
            // at Back's centre found a feed row. The band's strip still fades
            // the body's top edge.
            VStack(spacing: 0) {
                ChatHeader {
                    Text(title)
                        .font(.system(size: CGFloat(Chats.Shell.ChatHeader.title), weight: .semibold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .accessibilityAddTraits(.isHeader)
                        .accessibilityIdentifier("session-item-title")
                } trailing: {
                    EmptyView()
                }
                .frame(maxWidth: .infinity)
                .headerEdgeChrome()
                .zIndex(1)
                content()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .toolbar(.hidden, for: .navigationBar)
        .bareBackButton()
        // A pushed page is never the detail column's root: its back shows.
        .environment(\.detailColumnRoot, false)
    }
}
