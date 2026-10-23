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
        /// THE Queue, or nil when nothing is to do.
        case tasks(QueueSummary?)
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
            case let .tasks(queue):
                guard let queue, let first = queue.items.first else { continue }
                rows.append((Row(
                    id: "tasks", kind: .tasks, tint: .muted, icon: Icons.tasks, label: queue.todoTitle,
                    caption: queue.completedTitle, meta: first.description, title: first.label,
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
