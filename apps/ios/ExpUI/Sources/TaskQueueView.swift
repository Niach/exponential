import ExpCore
import SwiftUI

// EXP-1321/1324: THE Queue — the `tasks` DockCard's sheet body: a `Trace`
// (size sm) of the todo items (running shimmering, pending faded) with no
// title line of their own (the sheet's header IS the todo title), then
// "n completed" (collapsed) whose items are faded lines. The model is ExpCore
// `TaskQueue.build` (`fixtures/task-queue.json`); a running background
// item's elapsed time ticks once a second, so the view rebuilds the model
// off a 1s clock.

public struct TaskQueueView: View {
    let build: (Date) -> TaskQueue?
    @Binding var completedOpen: Bool

    /// `build` = the queue at a moment (`now` drives the background
    /// durations). The completed open state belongs to the caller so the
    /// reader's toggle sticks for the session.
    public init(
        completedOpen: Binding<Bool>,
        build: @escaping (Date) -> TaskQueue?
    ) {
        self._completedOpen = completedOpen
        self.build = build
    }

    public var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            if let queue = build(context.date) {
                TaskQueueBlock(queue: queue, completedOpen: $completedOpen)
            }
        }
    }
}

/// The Queue painted from one model value (no clock) — the styleguide draws
/// this directly.
public struct TaskQueueBlock: View {
    let queue: TaskQueue
    @Binding var completedOpen: Bool

    public init(queue: TaskQueue, completedOpen: Binding<Bool>) {
        self.queue = queue
        self._completedOpen = completedOpen
    }

    private static let size: TraceSize = .sm

    public var body: some View {
        TraceStack(size: Self.size) {
            TraceStack(size: Self.size) {
                lines(queue.todo, done: false)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier(TaskQueue.TestIds.todo)
            if let completed = queue.completed {
                TraceCollapsible(size: Self.size, open: $completedOpen, accessibilityId: TaskQueue.TestIds.completed) {
                    TraceLine(
                        TraceLineModel(icon: completed.icon, title: completed.title, description: nil, status: nil),
                        size: Self.size
                    )
                } content: {
                    lines(completed, done: true)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(TaskQueue.TestIds.queue)
    }

    @ViewBuilder
    private func lines(_ section: TaskQueueSection, done: Bool) -> some View {
        ForEach(section.items) { item in
            TraceLine(
                TraceLineModel(
                    icon: item.icon,
                    title: item.label,
                    description: item.description,
                    status: status(item, done: done)
                ),
                size: Self.size
            )
        }
    }

    private func status(_ item: TaskQueueItem, done: Bool) -> TraceStatus? {
        if done { return .pending }
        switch item.state {
        case .running: return .running
        case .pending, .done: return .pending
        }
    }
}
