// EXP-1321/1324: THE Queue, the ONE block at the bottom of a session's
// transcript (right above the queued-messages strip and the composer). It
// replaces the bottom strip's background-task + "Waiting on" lines and the
// Work card's `tasks` variant with two Trace collapsibles: "n to do", open,
// listing the running work (shimmering) then the pending tasks (muted), and
// "n completed", collapsed. TS reference; desktop `steer::task_queue`, iOS
// ExpCore `TaskQueue.swift`, Android `domain/TaskQueue.kt` replay
// `fixtures/task-queue.json`.

import fixture from "../fixtures/task-queue.json" with { type: "json" }
import { workDurationLong } from "./work-card"

export interface TaskQueueInput {
  /** The agent's own task list (`task_list` state), in order. */
  tasks: readonly { content: string; status: string }[]
  /** The CLI's background tasks (`background_tasks` state). */
  backgroundTasks: readonly {
    id: string
    kind: string
    description: string
    /** Epoch ms the device saw it start (wire), else the client's first sight. */
    startedAt?: number | null
  }[]
  /** Unsettled `wait` tool rows' subjects (detail, else the tool name). */
  waits: readonly { id: string; subject: string }[]
  /** Epoch ms, for the background durations. */
  now: number
}

export type TaskQueueItemKind = `task` | `background` | `wait`
export type TaskQueueItemState = `running` | `pending` | `done`

export interface TaskQueueItem {
  /** Stable key: `task:<index>`, `background:<id>`, `wait:<id>`. */
  key: string
  kind: TaskQueueItemKind
  state: TaskQueueItemState
  /** Concept icon. */
  icon: string
  label: string
  /** Muted trailing text: a background task's elapsed time. */
  description: string | null
}

export interface TaskQueueSection {
  title: string
  icon: string
  items: TaskQueueItem[]
}

export interface TaskQueue {
  todo: TaskQueueSection
  /** Null when nothing completed. */
  completed: TaskQueueSection | null
}

function clean(value: string | null | undefined): string {
  return (value ?? ``).replace(/\s+/g, ` `).trim()
}

function count(n: number, template: string): string {
  return template.replace(`{count}`, String(n))
}

/** Null = no block at all: nothing to do (an all-completed list hides, as
 *  the strip always did). Order in "to do": in-progress tasks, background
 *  tasks, waits, then pending tasks — everything running first. A background
 *  task of kind `agent` never shows (a subagent is its Trace line + tab). */
export function taskQueue(input: TaskQueueInput): TaskQueue | null {
  const icons = fixture.icons
  const running: TaskQueueItem[] = []
  const pending: TaskQueueItem[] = []
  const done: TaskQueueItem[] = []
  input.tasks.forEach((task, index) => {
    const label = clean(task.content)
    if (!label) return
    const key = `task:${index}`
    if (task.status === `completed`) {
      done.push({ key, kind: `task`, state: `done`, icon: icons.done, label, description: null })
    } else if (task.status === `in_progress`) {
      running.push({ key, kind: `task`, state: `running`, icon: icons.running, label, description: null })
    } else {
      pending.push({ key, kind: `task`, state: `pending`, icon: icons.pending, label, description: null })
    }
  })
  const background: TaskQueueItem[] = []
  for (const task of input.backgroundTasks) {
    if (task.kind === `agent`) continue
    const label = clean(task.description)
    if (!label) continue
    const started = task.startedAt ?? null
    background.push({
      key: `background:${task.id}`,
      kind: `background`,
      state: `running`,
      icon: icons.background,
      label,
      description: started === null ? null : workDurationLong(Math.max(0, input.now - started)),
    })
  }
  const waits: TaskQueueItem[] = []
  for (const wait of input.waits) {
    const subject = clean(wait.subject)
    if (!subject) continue
    waits.push({
      key: `wait:${wait.id}`,
      kind: `wait`,
      state: `running`,
      icon: icons.background,
      label: fixture.copy.waiting.replace(`{subject}`, subject),
      description: null,
    })
  }
  const todo = [...running, ...background, ...waits, ...pending]
  if (todo.length === 0) return null
  return {
    todo: { title: count(todo.length, fixture.copy.todo), icon: icons.todo, items: todo },
    completed:
      done.length === 0
        ? null
        : { title: count(done.length, fixture.copy.completed), icon: icons.completed, items: done },
  }
}
