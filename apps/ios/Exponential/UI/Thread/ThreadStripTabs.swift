import ExpCore
import ExpUI
import SwiftUI

/// EXP-1334: what a folded chip's tap or a zone's "+N" opens — THE Menu's
/// sheet: the chip itself first, then its nested rows as THE tree rows
/// (`SessionRow` + `TreeGuides`), or the hidden chips with their subtrees.
struct ThreadStripFold: Identifiable, Equatable {
    enum Row: Equatable, Identifiable {
        case issue(issueId: String, identifier: String, depth: Int)
        case session(sessionId: String, name: String, markState: SessionState, depth: Int)

        var id: String {
            switch self {
            case let .issue(issueId, _, _): "i:\(issueId)"
            case let .session(sessionId, _, _, _): "s:\(sessionId)"
            }
        }

        var depth: Int {
            switch self {
            case let .issue(_, _, depth), let .session(_, _, _, depth): depth
            }
        }
    }

    let id: String
    /// The sheet's title (a chip's name); nil = a "+N" list.
    let title: String?
    let rows: [Row]

    /// A chip's subtree: row 1 = the chip, then its nested rows.
    static func subtree(_ tab: ThreadTab) -> ThreadStripFold? {
        switch tab {
        case let .issue(issueId, identifier, _):
            ThreadStripFold(id: "issue:\(issueId)", title: identifier, rows: rows(tab))
        case let .session(sessionId, name, _, _):
            ThreadStripFold(id: "session:\(sessionId)", title: name, rows: rows(tab))
        default:
            nil
        }
    }

    /// One chip and its nested rows, depth-first (the chip at depth 0).
    static func rows(_ tab: ThreadTab) -> [Row] {
        switch tab {
        case let .issue(issueId, identifier, nested):
            [.issue(issueId: issueId, identifier: identifier, depth: 0)]
                + nested.map { .issue(issueId: $0.issueId, identifier: $0.identifier, depth: $0.depth) }
        case let .session(sessionId, name, markState, nested):
            [.session(sessionId: sessionId, name: name, markState: markState, depth: 0)]
                + nested.map { .session(sessionId: $0.sessionId, name: $0.name, markState: $0.markState, depth: $0.depth) }
        default:
            []
        }
    }
}

/// EXP-1295/1316/1334: the thread header's ONE strip as the phone's pager
/// tabs (fixture `session-thread.json` `tabs`), and it NEVER scrolls: the
/// "+" (link or file an issue) and `Conversation` pinned left, `Guide` (a
/// green dot while the root's Guide holds anything) pinned right, both
/// ICON-ONLY on phones (`action-chat`, `guide-changes`); between
/// them the issue chips (status glyph + mono identifier, + the title while
/// everything fits) then the session chips (the row mark + name; a tap
/// shows that session's page, EXP-1343), laid out by `SessionTree.partitionThreadStrip`
/// over MEASURED widths (`ThreadStripMetrics`): titles go first, then each
/// zone folds the rest behind its own "+N" (THE Menu's sheet of the hidden
/// chips). A chip with `nested` wears a chevron-down and its TAP opens the
/// subtree sheet (row 1 = the chip itself, then THE tree rows); it reads
/// `<chip> › <open one>` while a nested issue is the shown page. Every chip
/// is ONE height (entity-chip.json tab size); the shown page is the filled
/// chip. A long press on an issue chip opens THE Menu (Link an issue · New
/// issue · Unlink). An issue linked while the screen was up fades in with a
/// ring.
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
    /// A session chip's tap (nil = session chips are not shown); EXP-1343:
    /// the thread selects that session's page.
    var onOpenSession: ((String) -> Void)?
    /// A folded chip's tap or a "+N": the host presents THE Menu's sheet.
    var onOpenFold: (ThreadStripFold) -> Void = { _ in }

    /// entity-chip.json `tab` size: every chip in the strip is one height —
    /// web `STRIP_TAB_CLASS` `h-8 rounded-md`: 32pt tall, the 10pt row radius
    /// (`GlassTokens.rowRadius`), a rounded rectangle, not a capsule.
    static let chipHeight: CGFloat = 32
    static let chipShape = RoundedRectangle(cornerRadius: GlassTokens.rowRadius, style: .continuous)
    /// The strip's one gap (between chips and between the zones).
    static let gap: CGFloat = 4

    /// The room between Conversation and Guide (measured).
    @State private var available: CGFloat = 0

    private var issueTabs: [ThreadTab] {
        tabs.filter { if case .issue = $0 { true } else { false } }
    }

    private var sessionTabs: [ThreadTab] {
        guard onOpenSession != nil else { return [] }
        return tabs.filter { if case .session = $0 { true } else { false } }
    }

    var body: some View {
        HStack(spacing: Self.gap) {
            if tabs.contains(.add), let onAdd {
                addButton(onAdd)
            }
            // NEW-6 (N-1): a one-session thread has no Conversation tab.
            if tabs.contains(.conversation) {
                pageTab(.conversation)
                    .fixedSize()
            }
            // The room is measured off a flexible spacer the chips overlay,
            // so the chips never widen what they are measured against.
            Color.clear
                .frame(maxWidth: .infinity)
                .frame(height: Self.chipHeight)
                .onGeometryChange(for: CGFloat.self, of: { $0.size.width }) { width in
                    available = width
                }
                .overlay(alignment: .leading) { middle }
                .clipped()
            pageTab(.guide)
                .fixedSize()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(SessionTree.ThreadCopy.tabsTestId)
    }

    // MARK: - The two zones

    private var middle: some View {
        let issues = issueTabs
        let sessions = sessionTabs
        let activeIssue = issues.firstIndex { openNested(in: $0) != nil || isShown($0) }
        let layout = SessionTree.partitionThreadStrip(
            issues: issues.map { tab in
                (full: Double(ThreadStripMetrics.issueWidth(tab, title: title(tab), open: openIdentifier(tab), full: true)),
                 short: Double(ThreadStripMetrics.issueWidth(tab, title: nil, open: openIdentifier(tab), full: false)))
            },
            sessions: sessions.map { Double(ThreadStripMetrics.sessionWidth($0)) },
            available: Double(available),
            gap: Double(Self.gap),
            overflowW: Double(ThreadStripMetrics.overflowWidth),
            activeIssue: activeIssue,
            activeSession: sessions.firstIndex { holdsShownSession($0) }
        )
        // Each zone is drawn at its MEASURED width (its chips, then its "+N");
        // the session zone never clips past the edge.
        let gap = Self.gap
        let overflowW = ThreadStripMetrics.overflowWidth
        func zoneWidth(_ widths: [CGFloat], hidden: Bool) -> CGFloat {
            let all = widths + (hidden ? [overflowW] : [])
            return all.isEmpty ? 0 : all.reduce(0, +) + gap * CGFloat(all.count - 1)
        }
        var shownSessions = layout.sessions
        var shownIssues = layout.issues
        let full = layout.tier == .full
        func sessionsWidth() -> CGFloat {
            sessions.isEmpty ? 0 : zoneWidth(
                shownSessions.map { ThreadStripMetrics.sessionWidth(sessions[$0]) },
                hidden: shownSessions.count < sessions.count
            )
        }
        func issuesWidth() -> CGFloat {
            issues.isEmpty ? 0 : zoneWidth(
                shownIssues.map {
                    ThreadStripMetrics.issueWidth(
                        issues[$0], title: full ? title(issues[$0]) : nil,
                        open: openIdentifier(issues[$0]), full: full
                    )
                },
                hidden: shownIssues.count < issues.count
            )
        }
        func fits() -> Bool {
            let sep: CGFloat = issues.isEmpty || sessions.isEmpty ? 0 : gap
            return issuesWidth() + sep + sessionsWidth() <= available
        }
        // Whatever the contract keeps, nothing may clip: an over-full zone
        // folds WHOLE into its "+N" — the sessions first while an issue is
        // the shown page (it must stay visible), else the issues first.
        let activeSession = sessions.firstIndex { holdsShownSession($0) }
        let order: [String] = activeIssue != nil ? ["sessions", "issues"] : ["issues", "sessions"]
        for zone in order where !fits() {
            if zone == "sessions" {
                shownSessions = activeSession.map { [$0] } ?? []
            } else {
                shownIssues = activeIssue.map { [$0] } ?? []
            }
        }
        let sessionW = sessionsWidth()
        let issueW = issuesWidth()
        let hiddenIssues = issues.indices.filter { !shownIssues.contains($0) }.map { issues[$0] }
        let hiddenSessions = sessions.indices.filter { !shownSessions.contains($0) }.map { sessions[$0] }
        let issueRoom = max(min(issueW, available - (sessions.isEmpty ? 0 : sessionW + gap)), 0)
        return HStack(spacing: gap) {
            if !issues.isEmpty {
                HStack(spacing: gap) {
                    ForEach(shownIssues, id: \.self) { index in
                        issueChip(issues[index], full: layout.tier == .full)
                    }
                    if !hiddenIssues.isEmpty {
                        overflowChip(hiddenIssues, zone: "issues")
                    }
                }
                // Over-full (the shown issue plus its "+N"), the open
                // sub-issue's label truncates; identifiers and "+N" never do.
                .frame(width: issueRoom, alignment: .leading)
                .clipped()
            }
            if !sessions.isEmpty {
                HStack(spacing: gap) {
                    ForEach(shownSessions, id: \.self) { index in
                        sessionChip(sessions[index])
                    }
                    if !hiddenSessions.isEmpty {
                        overflowChip(hiddenSessions, zone: "sessions")
                    }
                }
                .fixedSize()
            }
        }
        .frame(maxWidth: max(available, 0), alignment: .leading)
    }

    private func title(_ tab: ThreadTab) -> String? {
        guard case let .issue(issueId, _, _) = tab else { return nil }
        return issue(issueId)?.title
    }

    /// The shown page is this chip's own issue (or, EXP-1343, session).
    private func isShown(_ tab: ThreadTab) -> Bool {
        if case let .issue(issueId, _, _) = tab { return selection == .issue(issueId) }
        if case let .session(sessionId, _, _, _) = tab { return selection == .session(sessionId) }
        return false
    }

    /// EXP-1343: the shown page is this session chip or one it folds.
    private func holdsShownSession(_ tab: ThreadTab) -> Bool {
        guard case let .session(sessionId, _, _, nested) = tab, case let .session(shown) = selection
        else { return false }
        return shown == sessionId || nested.contains { $0.sessionId == shown }
    }

    /// The nested issue that is the shown page, if any.
    private func openNested(in tab: ThreadTab) -> ThreadTabNestedIssue? {
        guard case let .issue(_, _, nested) = tab, case let .issue(shown) = selection else { return nil }
        return nested.first { $0.issueId == shown }
    }

    private func openIdentifier(_ tab: ThreadTab) -> String? {
        openNested(in: tab)?.identifier
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

    @ViewBuilder
    private func issueChip(_ tab: ThreadTab, full: Bool) -> some View {
        if case let .issue(issueId, identifier, nested) = tab {
            let open = openNested(in: tab)
            let selected = isShown(tab) || open != nil
            let status = IssueStatus.from(issue(issueId)?.status)
            ThreadStripTab(selected: selected, bordered: true, fresh: isFresh(issueId)) {
                HStack(spacing: ThreadStripMetrics.innerSpacing) {
                    AppIcon(status.iconName, size: AppIcon.Size.small)
                        .foregroundStyle(status.color)
                    Text(identifier)
                        .font(ThreadStripMetrics.identifierFont)
                        .fixedSize()
                    if let open {
                        Text("› \(open.identifier)")
                            .font(ThreadStripMetrics.identifierFont)
                            .lineLimit(1)
                    }
                    if full, let title = title(tab), !title.isEmpty {
                        Text(title)
                            .font(ThreadStripMetrics.nameFont)
                            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
                            .lineLimit(1)
                            .frame(maxWidth: ThreadStripMetrics.titleMax, alignment: .leading)
                    }
                    if !nested.isEmpty { chevron }
                }
            }
            .onTapGesture {
                if nested.isEmpty {
                    onSelect(.issue(issueId))
                } else if let fold = ThreadStripFold.subtree(tab) {
                    onOpenFold(fold)
                }
            }
            .onLongPressGesture(minimumDuration: 0.4) { onIssueMenu(issueId) }
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel("\(identifier) \(issue(issueId)?.title ?? "")")
            .accessibilityAction(named: "More") { onIssueMenu(issueId) }
            .accessibilityIdentifier("thread-tab-\(identifier)")
        }
    }

    @ViewBuilder
    private func sessionChip(_ tab: ThreadTab) -> some View {
        if case let .session(sessionId, name, markState, nested) = tab, let onOpenSession {
            ThreadStripTab(selected: holdsShownSession(tab), bordered: true, fresh: false) {
                HStack(spacing: ThreadStripMetrics.innerSpacing) {
                    ThreadSessionChipMark(state: markState)
                        .frame(width: ThreadStripMetrics.markSize, height: ThreadStripMetrics.markSize)
                    Text(name)
                        .font(ThreadStripMetrics.nameFont)
                        .lineLimit(1)
                    if !nested.isEmpty { chevron }
                }
            }
            .onTapGesture {
                if nested.isEmpty {
                    onOpenSession(sessionId)
                } else if let fold = ThreadStripFold.subtree(tab) {
                    onOpenFold(fold)
                }
            }
            .accessibilityElement(children: .combine)
            .accessibilityAddTraits(.isButton)
            .accessibilityLabel("\(name), \(markState.word)")
            .accessibilityIdentifier("thread-tab-session-\(sessionId)")
        }
    }

    /// A zone's "+N": THE Menu's sheet of the hidden chips (each with its
    /// subtree).
    private func overflowChip(_ hidden: [ThreadTab], zone: String) -> some View {
        ThreadStripTab(selected: false, bordered: true, fresh: false) {
            Text("+\(hidden.count)")
                .font(ThreadStripMetrics.nameFont)
                .fixedSize()
        }
        .layoutPriority(1)
        .onTapGesture {
            onOpenFold(ThreadStripFold(id: "more:\(zone)", title: nil, rows: hidden.flatMap(ThreadStripFold.rows)))
        }
        .accessibilityAddTraits(.isButton)
        .accessibilityLabel(SessionTree.ThreadCopy.more(hidden.count, zone: zone))
        .accessibilityIdentifier("thread-strip-more-\(zone)")
    }

    private var chevron: some View {
        AppIcon(AppIcons.uiChevronDown, size: ThreadStripMetrics.chevronSize)
            .foregroundStyle(.white.opacity(TextOpacity.tertiary))
    }

    @ViewBuilder
    private func pageTab(_ page: ThreadPage) -> some View {
        let selected = page == selection
        switch page {
        case .issue, .session:
            EmptyView()
        case .guide:
            // EXP-1334: phones draw Conversation and Guide ICON-ONLY in the
            // same chip box (the word stays the accessibility label), so the
            // middle zones get the room.
            let dot = tabs.contains(.guide(dot: true))
            ThreadStripTab(selected: selected, bordered: false, fresh: false) {
                AppIcon(AppIcons.guideChanges, size: ThreadStripMetrics.pinnedIconSize, weight: .medium)
                    .overlay(alignment: .topTrailing) {
                        if dot {
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
            .accessibilityLabel(SessionTree.ThreadCopy.guide)
            .accessibilityIdentifier("thread-tab-guide")
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

/// EXP-1334: the chip widths `partitionThreadStrip` packs — measured off the
/// same fonts the chips draw (UIKit text metrics, no layout pass), so the
/// strip decides before it draws.
enum ThreadStripMetrics {
    static let horizontalPadding: CGFloat = 10
    static let innerSpacing: CGFloat = 5
    static let markSize: CGFloat = 13
    static let chevronSize: CGFloat = 10
    /// The icon-only Conversation / Guide tabs (phones).
    static let pinnedIconSize: CGFloat = 16
    /// A full chip's title never grows past this.
    static let titleMax: CGFloat = 140

    static let identifierFont = Font.system(.footnote, design: .monospaced)
    static let nameFont = Font.footnote.weight(.medium)

    private static var footnoteSize: CGFloat { UIFont.preferredFont(forTextStyle: .footnote).pointSize }

    private static func measure(_ text: String, mono: Bool) -> CGFloat {
        let font = mono
            ? UIFont.monospacedSystemFont(ofSize: footnoteSize, weight: .regular)
            : UIFont.systemFont(ofSize: footnoteSize, weight: .medium)
        return ceil((text as NSString).size(withAttributes: [.font: font]).width)
    }

    static func issueWidth(_ tab: ThreadTab, title: String?, open: String?, full: Bool) -> CGFloat {
        guard case let .issue(_, identifier, nested) = tab else { return 0 }
        var width = horizontalPadding * 2 + AppIcon.Size.small + innerSpacing + measure(identifier, mono: true)
        if let open { width += innerSpacing + measure("› \(open)", mono: true) }
        if full, let title, !title.isEmpty { width += innerSpacing + min(measure(title, mono: false), titleMax) }
        if !nested.isEmpty { width += innerSpacing + chevronSize }
        // SwiftUI's text boxes round up past UIKit's metrics: a few points
        // of slack per text run keep a measured zone from clipping.
        return width + (open == nil ? 2 : 6)
    }

    static func sessionWidth(_ tab: ThreadTab) -> CGFloat {
        guard case let .session(_, name, _, nested) = tab else { return 0 }
        var width = horizontalPadding * 2 + markSize + innerSpacing + measure(name, mono: false)
        if !nested.isEmpty { width += innerSpacing + chevronSize }
        return width + 2
    }

    /// A "+N" chip (one digit's room, as drawn).
    static var overflowWidth: CGFloat { horizontalPadding * 2 + measure("+9", mono: false) + 1 }
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
