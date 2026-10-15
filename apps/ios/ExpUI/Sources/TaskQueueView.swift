import ExpCore
import SwiftUI

// EXP-1321/1324: THE Queue — the ONE block at the bottom of a session's
// transcript, right above the queued-messages strip: a `Trace` (size sm) of
// two collapsibles, "n to do" (open by default) listing the running work
// (shimmering) then the pending tasks (faded), and "n completed" (collapsed)
// whose items are faded lines. The model is ExpCore `TaskQueue.build`
// (`fixtures/task-queue.json`); a running background item's elapsed time
// ticks once a second, so the view rebuilds the model off a 1s clock.

public struct TaskQueueView: View {
    let build: (Date) -> TaskQueue?
    @Binding var todoOpen: Bool
    @Binding var completedOpen: Bool

    /// `build` = the queue at a moment (`now` drives the background
    /// durations). The two open states belong to the caller so the reader's
    /// toggle sticks for the session.
    public init(
        todoOpen: Binding<Bool>,
        completedOpen: Binding<Bool>,
        build: @escaping (Date) -> TaskQueue?
    ) {
        self._todoOpen = todoOpen
        self._completedOpen = completedOpen
        self.build = build
    }

    public var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            if let queue = build(context.date) {
                TaskQueueBlock(queue: queue, todoOpen: $todoOpen, completedOpen: $completedOpen)
            }
        }
    }
}

/// The Queue painted from one model value (no clock) — the styleguide draws
/// this directly.
public struct TaskQueueBlock: View {
    let queue: TaskQueue
    @Binding var todoOpen: Bool
    @Binding var completedOpen: Bool

    public init(queue: TaskQueue, todoOpen: Binding<Bool>, completedOpen: Binding<Bool>) {
        self.queue = queue
        self._todoOpen = todoOpen
        self._completedOpen = completedOpen
    }

    private static let size: TraceSize = .sm

    public var body: some View {
        TraceStack(size: Self.size) {
            section(queue.todo, open: $todoOpen, id: TaskQueue.TestIds.todo, done: false)
            if let completed = queue.completed {
                section(completed, open: $completedOpen, id: TaskQueue.TestIds.completed, done: true)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(TaskQueue.TestIds.queue)
    }

    private func section(
        _ section: TaskQueueSection, open: Binding<Bool>, id: String, done: Bool
    ) -> some View {
        TraceCollapsible(size: Self.size, open: open, accessibilityId: id) {
            TraceLine(
                TraceLineModel(icon: section.icon, title: section.title, description: nil, status: nil),
                size: Self.size
            )
        } content: {
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
    }

    private func status(_ item: TaskQueueItem, done: Bool) -> TraceStatus? {
        if done { return .pending }
        switch item.state {
        case .running: return .running
        case .pending, .done: return .pending
        }
    }
}
