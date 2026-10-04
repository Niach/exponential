import { cn } from "./cn"

// EXP-1191 — the agent task list's own mark ×4, so its line above the
// composer never reads as a queued steer message or as the composer: one
// short segment per task, done ones solid, the current one half, the rest
// faint. A list longer than `TASK_LIST_PROGRESS_MAX_SEGMENTS` draws one
// continuous track filled to `done / total` instead. Desktop
// `task_list_progress`, iOS `TaskListProgress`, Android `TaskListProgress`.

export const TASK_LIST_PROGRESS_MAX_SEGMENTS = 12

export type TaskListProgressStatus = `pending` | `in_progress` | `completed`

export function TaskListProgress({
  statuses,
  className,
}: {
  statuses: readonly TaskListProgressStatus[]
  className?: string
}) {
  const total = statuses.length
  if (total === 0) return null
  const done = statuses.filter((status) => status === `completed`).length
  if (total > TASK_LIST_PROGRESS_MAX_SEGMENTS) {
    return (
      <span
        aria-hidden
        data-testid="task-list-progress"
        className={cn(
          `relative h-1 w-20 shrink-0 overflow-hidden rounded-full bg-foreground/12`,
          className
        )}
      >
        <span
          className="absolute inset-y-0 left-0 rounded-full bg-foreground/70"
          style={{ width: `${(done / total) * 100}%` }}
        />
      </span>
    )
  }
  return (
    <span
      aria-hidden
      data-testid="task-list-progress"
      className={cn(`flex shrink-0 items-center gap-[3px]`, className)}
    >
      {statuses.map((status, index) => (
        <span
          // Positional: the agent rewrites the list whole.
          key={index}
          data-status={status}
          className={cn(
            `h-1 w-2.5 rounded-full`,
            status === `completed`
              ? `bg-foreground/70`
              : status === `in_progress`
                ? `bg-foreground/35`
                : `bg-foreground/12`
          )}
        />
      ))}
    </span>
  )
}
