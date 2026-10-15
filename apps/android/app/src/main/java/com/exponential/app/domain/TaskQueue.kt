package com.exponential.app.domain

// EXP-1321/1324: THE Queue, the ONE block at the bottom of a session's
// transcript (right above the queued-messages strip). It replaces the old
// background-task + "Waiting on" strip lines and the Work card's `tasks`
// variant with two Trace collapsibles: "n to do" (open) and "n completed"
// (collapsed). Hand-ported from `packages/domain-contract/src/task-queue.ts`
// and locked to `fixtures/task-queue.json` by TaskQueueTest (whole objects).
// The Compose half is `TaskQueueView` in `ui/components/Trace.kt`.

object TaskQueueSpec {
    // fixture `copy`
    const val TODO = "{count} to do"
    const val COMPLETED = "{count} completed"
    const val WAITING = "Waiting on {subject}"

    // fixture `icons`
    const val ICON_TODO = "trace-todo"
    const val ICON_COMPLETED = "trace-completed"
    const val ICON_RUNNING = "trace-task-running"
    const val ICON_PENDING = "trace-task-pending"
    const val ICON_DONE = "trace-task-done"
    const val ICON_BACKGROUND = "trace-background"

    // fixture `testIds`
    const val TEST_ID_QUEUE = "session-task-queue"
    const val TEST_ID_TODO = "session-task-queue-todo"
    const val TEST_ID_COMPLETED = "session-task-queue-completed"
}

data class TaskQueueTaskInput(val content: String, val status: String)

data class TaskQueueBackgroundInput(
    val id: String,
    val kind: String,
    val description: String,
    /** Epoch ms the device saw it start (wire), else the client's first sight. */
    val startedAt: Long? = null,
)

data class TaskQueueWaitInput(val id: String, val subject: String)

data class TaskQueueInput(
    val tasks: List<TaskQueueTaskInput>,
    val backgroundTasks: List<TaskQueueBackgroundInput>,
    val waits: List<TaskQueueWaitInput>,
    val now: Long,
)

enum class TaskQueueItemKind(val wire: String) { Task("task"), Background("background"), Wait("wait") }

enum class TaskQueueItemState(val wire: String) { Running("running"), Pending("pending"), Done("done") }

data class TaskQueueItem(
    /** Stable key: `task:<index>`, `background:<id>`, `wait:<id>`. */
    val key: String,
    val kind: TaskQueueItemKind,
    val state: TaskQueueItemState,
    /** Concept icon. */
    val icon: String,
    val label: String,
    /** Muted trailing text: a background task's elapsed time. */
    val description: String?,
)

data class TaskQueueSection(val title: String, val icon: String, val items: List<TaskQueueItem>)

data class TaskQueue(
    val todo: TaskQueueSection,
    /** Null when nothing completed. */
    val completed: TaskQueueSection?,
)

object TaskQueues {

    private fun count(n: Int, template: String) = template.replace("{count}", n.toString())

    /** Null = no block at all (an all-completed list hides). Order in "to
     *  do": in-progress tasks, background tasks, waits, then pending tasks.
     *  A background task of kind `agent` never shows. */
    fun of(input: TaskQueueInput): TaskQueue? {
        val running = mutableListOf<TaskQueueItem>()
        val pending = mutableListOf<TaskQueueItem>()
        val done = mutableListOf<TaskQueueItem>()
        input.tasks.forEachIndexed { index, task ->
            val label = Trace.clean(task.content)
            if (label.isEmpty()) return@forEachIndexed
            val key = "task:$index"
            when (task.status) {
                TASK_LIST_STATUS_COMPLETED -> done += TaskQueueItem(
                    key, TaskQueueItemKind.Task, TaskQueueItemState.Done, TaskQueueSpec.ICON_DONE, label, null,
                )
                TASK_LIST_STATUS_IN_PROGRESS -> running += TaskQueueItem(
                    key, TaskQueueItemKind.Task, TaskQueueItemState.Running, TaskQueueSpec.ICON_RUNNING, label, null,
                )
                else -> pending += TaskQueueItem(
                    key, TaskQueueItemKind.Task, TaskQueueItemState.Pending, TaskQueueSpec.ICON_PENDING, label, null,
                )
            }
        }
        val background = input.backgroundTasks.mapNotNull { task ->
            if (task.kind == BACKGROUND_TASK_KIND_AGENT) return@mapNotNull null
            val label = Trace.clean(task.description)
            if (label.isEmpty()) return@mapNotNull null
            TaskQueueItem(
                key = "background:${task.id}",
                kind = TaskQueueItemKind.Background,
                state = TaskQueueItemState.Running,
                icon = TaskQueueSpec.ICON_BACKGROUND,
                label = label,
                description = task.startedAt?.let { WorkCards.durationLong(maxOf(0L, input.now - it)) },
            )
        }
        val waits = input.waits.mapNotNull { wait ->
            val subject = Trace.clean(wait.subject)
            if (subject.isEmpty()) return@mapNotNull null
            TaskQueueItem(
                key = "wait:${wait.id}",
                kind = TaskQueueItemKind.Wait,
                state = TaskQueueItemState.Running,
                icon = TaskQueueSpec.ICON_BACKGROUND,
                label = TaskQueueSpec.WAITING.replace("{subject}", subject),
                description = null,
            )
        }
        val todo = running + background + waits + pending
        if (todo.isEmpty()) return null
        return TaskQueue(
            todo = TaskQueueSection(count(todo.size, TaskQueueSpec.TODO), TaskQueueSpec.ICON_TODO, todo),
            completed = if (done.isEmpty()) {
                null
            } else {
                TaskQueueSection(count(done.size, TaskQueueSpec.COMPLETED), TaskQueueSpec.ICON_COMPLETED, done)
            },
        )
    }
}

/** The open `wait` tool rows as the Queue's waits: the call id (else the
 *  row id) and the detail, else the tool's own name. */
fun openWaits(feed: List<AgentFeedItem>): List<TaskQueueWaitInput> = feed
    .filterIsInstance<AgentFeedItem.Tool>()
    .filter { it.toolKind == TOOL_KIND_WAIT && !it.settled }
    .map { TaskQueueWaitInput(it.callId ?: it.id.toString(), it.detail?.takeIf { d -> d.isNotBlank() } ?: it.name) }
