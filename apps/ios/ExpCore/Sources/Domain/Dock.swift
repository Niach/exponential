import Foundation

/// N-12: THE Dock ×4 (`packages/domain-contract/fixtures/dock.json`, TS
/// reference `src/dock.ts`): the floating band over the transcript's bottom
/// edge holding the DockCards (a waiting plan or question, THE Queue, the
/// queued messages, the suggested tasks, a status banner) above THE composer, which never leaves.
/// Several cards pile like the toasts (`Stack.layout` with `Dock.stack`); a
/// tap opens a card's bottom sheet. `DockTests` replays every fixture case.
public enum Dock {
    public enum Kind: String, Equatable, Sendable {
        case plan, question, tasks, queued, suggestions, banner
    }

    public enum Tint: String, Equatable, Sendable {
        case plan, question, muted, warning
    }

    public enum Mode: String, Equatable, Sendable {
        case none, single, pile, fanned
    }

    /// THE Queue's part the card reads (task-queue.json `taskQueue`).
    public struct QueueSummary: Equatable, Sendable {
        public struct Item: Equatable, Sendable {
            public let label: String
            public let description: String?

            public init(label: String, description: String?) {
                self.label = label
                self.description = description
            }
        }

        public let todoTitle: String
        public let items: [Item]
        public let completedTitle: String?

        public init(todoTitle: String, items: [Item], completedTitle: String?) {
            self.todoTitle = todoTitle
            self.items = items
            self.completedTitle = completedTitle
        }

        public init(_ queue: TaskQueue) {
            self.init(
                todoTitle: queue.todo.title,
                items: queue.todo.items.map { Item(label: $0.label, description: $0.description) },
                completedTitle: queue.completed?.title
            )
        }
    }

    public enum Source: Equatable, Sendable {
        /// `sessionName` = the asking session's name in a thread; `questions`
        /// = how many questions the card asks; `createdAt` = the asks' order.
        case plan(id: String, text: String, sessionName: String?, questions: Int, createdAt: Double)
        case question(id: String, text: String, sessionName: String?, createdAt: Double)
        /// THE Queue, or nil when nothing is to do. #EXP-1545: `sessionName`
        /// = a thread's Live child carrying the card (its name leads the
        /// caption); nil = the root / the session view.
        case tasks(QueueSummary?, sessionName: String? = nil)
        /// The queued messages' texts, oldest first.
        case queued([String])
        /// EXP-1323's suggested-tasks row while any item is open
        /// (`suggestionSources`): `label` = the header's count, `caption` =
        /// its started part, `title` = the first open item's, `open` = how
        /// many are still open.
        case suggestions(id: String, label: String, caption: String?, title: String, open: Int)
        /// A status strip: `action` = its one button word, else nil.
        case banner(id: String, icon: String, label: String, text: String, tone: Tint, action: String?)
    }

    public struct Row: Equatable, Sendable, Identifiable {
        public let id: String
        public let kind: Kind
        public let tint: Tint
        /// Concept icon.
        public let icon: String
        public let label: String
        public let caption: String?
        public let meta: String?
        public let title: String
        /// A tap opens the sheet.
        public let expandable: Bool
        /// A banner's button word, else nil.
        public let action: String?

        public init(
            id: String, kind: Kind, tint: Tint, icon: String, label: String, caption: String?,
            meta: String?, title: String, expandable: Bool, action: String?
        ) {
            self.id = id
            self.kind = kind
            self.tint = tint
            self.icon = icon
            self.label = label
            self.caption = caption
            self.meta = meta
            self.title = title
            self.expandable = expandable
            self.action = action
        }
    }

    public struct View: Equatable, Sendable {
        public let mode: Mode
        /// Top to bottom; the front card is the LAST.
        public let rows: [Row]
        public let frontId: String?
        /// The open sheet's card, when that card exists and is expandable.
        public let openId: String?
        /// How many piled cards stay out of sight (beyond `stack.visible`).
        public let hidden: Int

        public init(mode: Mode, rows: [Row], frontId: String?, openId: String?, hidden: Int) {
            self.mode = mode
            self.rows = rows
            self.frontId = frontId
            self.openId = openId
            self.hidden = hidden
        }
    }

    // MARK: - Fixture constants

    public enum Copy {
        public static let queuedOne = "1 queued"
        public static let queuedMany = "{count} queued"
        public static let questionsOne = "1 question"
        public static let questionsMany = "{count} questions"
        public static let close = "Close"
        public static let showAll = "Show all"
    }

    public enum Icons {
        public static let plan = "coding-plan"
        public static let question = "ui-help"
        public static let tasks = "trace-todo"
        public static let queued = "ui-queued"
        public static let open = "ui-chevron-right"
        public static let close = "ui-close"
        public static let suggestions = "ui-checklist"
    }

    public enum TestIds {
        public static let dock = "dock"
        public static let pile = "dock-pile"
        public static let card = "dock-card"
        public static let sheet = "dock-sheet"
        public static let sheetBody = "dock-sheet-body"
        public static let sheetFooter = "dock-sheet-footer"
        public static let close = "dock-sheet-close"
    }

    /// The pile's numbers (`Stack.layout`).
    public static let stack = StackConstants(peek: 14, scaleStep: 0.05, visible: 3, gap: 6)
    /// The sheet: at most this fraction of the viewport; top corner radius.
    public static let sheetMaxHeightFraction: Double = 0.85
    public static let sheetRadius: Double = 24
    /// `order`: queued, tasks, suggestions, banners, then the asks.
    public static let order: [String] = ["queued", "tasks", "suggestions", "banner", "ask"]
    /// `autoOpenGraceMs`: how long after a host's data loads its cards
    /// still count as already waiting (auto-open seeds over it).
    public static let autoOpenGraceMs: Int = 1500

    // MARK: - Model

    static func counted(_ n: Int, one: String, many: String) -> String {
        n == 1 ? one : many.replacingOccurrences(of: "{count}", with: String(n))
    }

    private static func rank(_ kind: Kind) -> Int {
        switch kind {
        case .queued: order.firstIndex(of: "queued") ?? 0
        case .tasks: order.firstIndex(of: "tasks") ?? 1
        case .suggestions: order.firstIndex(of: "suggestions") ?? 2
        case .banner: order.firstIndex(of: "banner") ?? 3
        case .plan, .question: order.firstIndex(of: "ask") ?? 4
        }
    }

    /// The cards, in the Dock's order (queued, tasks, suggestions, banners, then the asks
    /// oldest first, ties by id). An empty queue or no queued message draws
    /// nothing.
    public static func rows(_ sources: [Source]) -> [Row] {
        var rows: [(row: Row, createdAt: Double)] = []
        for source in sources {
            switch source {
            case let .queued(messages):
                guard let first = messages.first else { continue }
                rows.append((Row(
                    id: "queued", kind: .queued, tint: .muted, icon: Icons.queued,
                    label: counted(messages.count, one: Copy.queuedOne, many: Copy.queuedMany),
                    caption: nil, meta: nil, title: first, expandable: true, action: nil
                ), 0))
            case let .tasks(queue, sessionName):
                guard let queue, let first = queue.items.first else { continue }
                let caption = [sessionName, queue.completedTitle].compactMap { $0 }.joined(separator: " · ")
                rows.append((Row(
                    id: "tasks", kind: .tasks, tint: .muted, icon: Icons.tasks, label: queue.todoTitle,
                    caption: caption.isEmpty ? nil : caption, meta: first.description, title: first.label,
                    expandable: true, action: nil
                ), 0))
            case let .suggestions(id, label, caption, title, _):
                rows.append((Row(
                    id: id, kind: .suggestions, tint: .muted, icon: Icons.suggestions, label: label,
                    caption: caption, meta: nil, title: title, expandable: true, action: nil
                ), 0))
            case let .banner(id, icon, label, text, tone, action):
                rows.append((Row(
                    id: "banner:\(id)", kind: .banner, tint: tone, icon: icon, label: label,
                    caption: nil, meta: nil, title: text, expandable: false, action: action
                ), 0))
            case let .plan(id, text, sessionName, questions, createdAt):
                let label = AskDock.label(.plan)
                let title = AskDock.title(text)
                rows.append((Row(
                    id: id, kind: .plan, tint: .plan, icon: Icons.plan, label: label, caption: sessionName,
                    meta: questions > 0
                        ? counted(questions, one: Copy.questionsOne, many: Copy.questionsMany) : nil,
                    title: title.isEmpty ? label : title, expandable: true, action: nil
                ), createdAt))
            case let .question(id, text, sessionName, createdAt):
                let label = AskDock.label(.question)
                let title = AskDock.title(text)
                rows.append((Row(
                    id: id, kind: .question, tint: .question, icon: Icons.question, label: label,
                    caption: sessionName, meta: nil, title: title.isEmpty ? label : title,
                    expandable: true, action: nil
                ), createdAt))
            }
        }
        return rows.sorted { a, b in
            let ra = rank(a.row.kind), rb = rank(b.row.kind)
            if ra != rb { return ra < rb }
            if a.createdAt != b.createdAt { return a.createdAt < b.createdAt }
            return a.row.id < b.row.id
        }.map(\.row)
    }

    /// #EXP-1545: a session of a thread's tree that may carry the Dock's ONE
    /// `tasks` card. `activeAt` = epoch ms of its last turn (else its start),
    /// nil = no activity yet; `open` = its Queue has a todo item to show.
    public struct ThreadQueueCandidate: Equatable, Sendable {
        public let sessionId: String
        public let name: String
        public let root: Bool
        public let ended: Bool
        public let activeAt: Double?
        public let open: Bool

        public init(sessionId: String, name: String, root: Bool, ended: Bool, activeAt: Double?, open: Bool) {
            self.sessionId = sessionId
            self.name = name
            self.root = root
            self.ended = ended
            self.activeAt = activeAt
            self.open = open
        }
    }

    /// #EXP-1545: whose Queue a thread's Dock shows; `sessionName` nil = the root.
    public struct ThreadDockQueue: Equatable, Sendable {
        public let sessionId: String
        public let sessionName: String?

        public init(sessionId: String, sessionName: String?) {
            self.sessionId = sessionId
            self.sessionName = sessionName
        }
    }

    /// #EXP-1545: the root's Queue while it is open, else the most recently
    /// active Live child's (no activity = oldest, ties by id), else none. An
    /// ended session contributes nothing.
    public static func threadDockQueue(_ candidates: [ThreadQueueCandidate]) -> ThreadDockQueue? {
        let live = candidates.filter { $0.open && !$0.ended }
        if let root = live.first(where: \.root) {
            return ThreadDockQueue(sessionId: root.sessionId, sessionName: nil)
        }
        var best: ThreadQueueCandidate?
        for candidate in live {
            guard let current = best else { best = candidate; continue }
            let a = candidate.activeAt ?? -.infinity, b = current.activeAt ?? -.infinity
            if a > b || (a == b && candidate.sessionId < current.sessionId) { best = candidate }
        }
        return best.map { ThreadDockQueue(sessionId: $0.sessionId, sessionName: $0.name) }
    }

    /// #EXP-1623: what a thread knows of one Live child's Queue before it
    /// pays for that child's stream.
    public enum ChildQueueState: Equatable, Sendable {
        case open
        case closed
        /// Never read (no stream yet, or its replay has not landed).
        case unknown
    }

    public struct ThreadQueueProbe: Equatable, Sendable {
        public let sessionId: String
        public let activeAt: Double?
        public let state: ChildQueueState

        public init(sessionId: String, activeAt: Double?, state: ChildQueueState) {
            self.sessionId = sessionId
            self.activeAt = activeAt
            self.state = state
        }
    }

    /// #EXP-1623: the ONE Live child a thread attaches a stream to for THE
    /// THREAD QUEUE, instead of one per Live child (#EXP-1545 opened a
    /// stream and a full replay for every one of them on open). None while
    /// the root's own Queue is open (the root wins); else the children in
    /// `threadDockQueue`'s order (most recently active first, none = oldest,
    /// ties by id) and the first that is not known closed: an open one is
    /// the pick itself, an unknown one is read next. Every child known
    /// closed = none.
    public static func threadQueueProbe(rootOpen: Bool, children: [ThreadQueueProbe]) -> String? {
        guard !rootOpen else { return nil }
        let ordered = children.sorted { a, b in
            let x = a.activeAt ?? -.infinity, y = b.activeAt ?? -.infinity
            return x != y ? x > y : a.sessionId < b.sessionId
        }
        return ordered.first { $0.state != .closed }?.sessionId
    }

    /// #EXP-1678 (dock.json ONE CHILD STREAM): a child stream as the probe
    /// reads it.
    public struct ChildStream: Equatable, Sendable {
        public enum Phase: String, Sendable { case connecting, live, starting, closed, ended }
        public let phase: Phase
        public let replayed: Bool
        public let attachedMs: Int

        public init(phase: Phase, replayed: Bool, attachedMs: Int) {
            self.phase = phase
            self.replayed = replayed
            self.attachedMs = attachedMs
        }
    }

    /// dock.json `probeDeadlineMs` / `closedQueueTtlMs`.
    public static let probeDeadlineMs = 15_000
    public static let closedQueueTtlMs = 60_000

    /// #EXP-1678 (`childStateCases`): one Live child's state for
    /// [threadQueueProbe]: open = its stream has a todo item; closed = read
    /// closed and remembered, or its stream is starting / dropped / ended,
    /// replayed without a todo, or attached `probeDeadlineMs` without one.
    public static func threadQueueChildState(open: Bool, remembered: Bool, stream: ChildStream?) -> ChildQueueState {
        if open { return .open }
        if remembered { return .closed }
        guard let stream else { return .unknown }
        switch stream.phase {
        case .starting, .closed, .ended: return .closed
        case .connecting, .live: break
        }
        if stream.replayed || stream.attachedMs >= probeDeadlineMs { return .closed }
        return .unknown
    }

    /// The band: the pile (or the fan while `expanded`), the front card, the
    /// open sheet.
    public static func view(rows: [Row], expanded: Bool, openId: String?) -> View {
        let open = openId.flatMap { id in rows.first { $0.id == id } }
        let mode: Mode = rows.isEmpty ? .none : rows.count == 1 ? .single : expanded ? .fanned : .pile
        return View(
            mode: mode,
            rows: rows,
            frontId: rows.last?.id,
            openId: open?.expandable == true ? open?.id : nil,
            hidden: mode == .pile ? max(0, rows.count - stack.visible) : 0
        )
    }

    /// The ask (plan or question) that just arrived, front-most first, else
    /// nil. `previous` = every card id seen before this frame.
    public static func autoOpen(previous: Set<String>, rows: [(id: String, kind: Kind)]) -> String? {
        for row in rows.reversed() where (row.kind == .plan || row.kind == .question) && !previous.contains(row.id) {
            return row.id
        }
        return nil
    }

    /// One suggested-tasks row as `suggestionSources` reads it: its id, the
    /// header, the items' states and titles.
    public struct SuggestionInput: Equatable, Sendable {
        public let id: String
        public let header: String
        public let items: [(state: String, title: String)]

        public init(id: String, header: String, items: [(state: String, title: String)]) {
            self.id = id
            self.header = header
            self.items = items
        }

        public static func == (a: Self, b: Self) -> Bool {
            a.id == b.id && a.header == b.header
                && a.items.map(\.state) == b.items.map(\.state) && a.items.map(\.title) == b.items.map(\.title)
        }
    }

    /// EXP-1323's suggested-tasks rows with any OPEN item, as Dock sources:
    /// the header splits at ` · ` into the label (the count) and the started
    /// caption; the title is the first open item's. A row with no open item
    /// is none of the Dock's (it stays in the Conversation).
    public static func suggestionSources(_ rows: [SuggestionInput]) -> [Source] {
        rows.compactMap { row in
            let open = row.items.filter { $0.state == SuggestionRow.State.open.rawValue }
            guard let first = open.first else { return nil }
            let parts = row.header.components(separatedBy: " · ")
            return .suggestions(
                id: row.id, label: parts[0], caption: parts.count > 1 ? parts[1] : nil,
                title: first.title, open: open.count
            )
        }
    }

    /// The Conversation's suggested-tasks rows as Dock sources.
    public static func suggestionSources(_ rows: [ThreadRow]) -> [Source] {
        suggestionSources(rows.compactMap { row in
            guard case let .suggestions(id, _, header, _, items) = row else { return nil }
            return SuggestionInput(id: id, header: header, items: items.map { ($0.state.rawValue, $0.title) })
        })
    }

    /// The pile's geometry (oldest first), bottom-anchored.
    public static func layout(heights: [Double], expanded: Bool) -> (height: Double, items: [ToastStack.ItemGeometry]) {
        Stack.layout(heights: heights, expanded: expanded, anchoredBottom: true, constants: stack)
    }
}
