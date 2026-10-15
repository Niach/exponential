import Foundation

// EXP-1321/1324: THE Queue, the ONE block at the bottom of a session's
// transcript (right above the queued-messages strip and the composer). It
// replaces the background-task + "Waiting on" lines and the Work card's
// `tasks` variant with two Trace collapsibles: "n to do" (open) and
// "n completed" (collapsed). Hand-mirrored from the TS reference
// `packages/domain-contract/src/task-queue.ts` and byte-locked by
// `packages/domain-contract/fixtures/task-queue.json` (`TaskQueueTests`).

public struct TaskQueueInput: Equatable, Sendable {
    public struct BackgroundTask: Equatable, Sendable {
        public let id: String
        public let kind: String
        public let description: String
        /// Epoch ms the device saw it start, else the client's first sight.
        public let startedAt: Int64?

        public init(id: String, kind: String, description: String, startedAt: Int64? = nil) {
            self.id = id
            self.kind = kind
            self.description = description
            self.startedAt = startedAt
        }
    }

    public struct Wait: Equatable, Sendable {
        public let id: String
        public let subject: String

        public init(id: String, subject: String) {
            self.id = id
            self.subject = subject
        }
    }

    /// The agent's own task list, in order.
    public let tasks: [WorkCardTask]
    public let backgroundTasks: [BackgroundTask]
    /// Unsettled `wait` tool rows' subjects.
    public let waits: [Wait]
    /// Epoch ms.
    public let now: Int64

    public init(tasks: [WorkCardTask], backgroundTasks: [BackgroundTask], waits: [Wait], now: Int64) {
        self.tasks = tasks
        self.backgroundTasks = backgroundTasks
        self.waits = waits
        self.now = now
    }
}

public enum TaskQueueItemKind: String, Equatable, Sendable {
    case task
    case background
    case wait
}

public enum TaskQueueItemState: String, Equatable, Sendable {
    case running
    case pending
    case done
}

public struct TaskQueueItem: Equatable, Sendable, Identifiable {
    /// Stable key: `task:<index>`, `background:<id>`, `wait:<id>`.
    public let key: String
    public let kind: TaskQueueItemKind
    public let state: TaskQueueItemState
    /// Concept icon.
    public let icon: String
    public let label: String
    /// Muted trailing text: a background task's elapsed time.
    public let description: String?

    public var id: String { key }

    public init(
        key: String, kind: TaskQueueItemKind, state: TaskQueueItemState, icon: String,
        label: String, description: String?
    ) {
        self.key = key
        self.kind = kind
        self.state = state
        self.icon = icon
        self.label = label
        self.description = description
    }
}

public struct TaskQueueSection: Equatable, Sendable {
    public let title: String
    public let icon: String
    public let items: [TaskQueueItem]

    public init(title: String, icon: String, items: [TaskQueueItem]) {
        self.title = title
        self.icon = icon
        self.items = items
    }
}

public struct TaskQueue: Equatable, Sendable {
    public let todo: TaskQueueSection
    /// Nil when nothing completed.
    public let completed: TaskQueueSection?

    public init(todo: TaskQueueSection, completed: TaskQueueSection?) {
        self.todo = todo
        self.completed = completed
    }

    /// Fixture `icons`.
    public enum Icons {
        public static let todo = "trace-todo"
        public static let completed = "trace-completed"
        public static let running = "trace-task-running"
        public static let pending = "trace-task-pending"
        public static let done = "trace-task-done"
        public static let background = "trace-background"
    }

    /// Fixture `testIds`.
    public enum TestIds {
        public static let queue = "session-task-queue"
        public static let todo = "session-task-queue-todo"
        public static let completed = "session-task-queue-completed"
    }

    /// Nil = no block at all (an all-completed list hides). "To do" order:
    /// in-progress tasks, background tasks, waits, then pending tasks. A
    /// background task of kind `agent` never shows (a subagent is its own
    /// Trace line + tab).
    public static func build(_ input: TaskQueueInput) -> TaskQueue? {
        var running: [TaskQueueItem] = []
        var pending: [TaskQueueItem] = []
        var done: [TaskQueueItem] = []
        for (index, task) in input.tasks.enumerated() {
            let label = Trace.clean(task.content)
            if label.isEmpty { continue }
            let key = "task:\(index)"
            switch task.status {
            case "completed":
                done.append(TaskQueueItem(key: key, kind: .task, state: .done, icon: Icons.done, label: label, description: nil))
            case "in_progress":
                running.append(TaskQueueItem(key: key, kind: .task, state: .running, icon: Icons.running, label: label, description: nil))
            default:
                pending.append(TaskQueueItem(key: key, kind: .task, state: .pending, icon: Icons.pending, label: label, description: nil))
            }
        }
        var background: [TaskQueueItem] = []
        for task in input.backgroundTasks where task.kind != "agent" {
            let label = Trace.clean(task.description)
            if label.isEmpty { continue }
            let description = task.startedAt.map {
                WorkCards.durationLong(ms: Int(max(0, input.now - $0)))
            }
            background.append(TaskQueueItem(
                key: "background:\(task.id)", kind: .background, state: .running,
                icon: Icons.background, label: label, description: description
            ))
        }
        var waits: [TaskQueueItem] = []
        for wait in input.waits {
            let subject = Trace.clean(wait.subject)
            if subject.isEmpty { continue }
            waits.append(TaskQueueItem(
                key: "wait:\(wait.id)", kind: .wait, state: .running,
                icon: Icons.background, label: "Waiting on \(subject)", description: nil
            ))
        }
        let todo = running + background + waits + pending
        if todo.isEmpty { return nil }
        return TaskQueue(
            todo: TaskQueueSection(title: "\(todo.count) to do", icon: Icons.todo, items: todo),
            completed: done.isEmpty
                ? nil
                : TaskQueueSection(title: "\(done.count) completed", icon: Icons.completed, items: done)
        )
    }
}
