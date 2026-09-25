// EXP-900: activity folding — READ-TIME ONLY. `issue_events` rows are never
// deleted or rewritten; every client folds the synced rows itself with this
// pure function (mirrored: desktop `domain::activity_fold`, iOS
// `ExpCore/ActivityFold.swift`, Android `domain/ActivityFold.kt`), all locked
// against `packages/domain-contract/fixtures/activity-fold.json`. A "Show all"
// toggle on the timeline header returns the unfolded list (EXP-468).
//
// THE FOLD RULE: a run of events on the SAME issue and the SAME field by the
// SAME actor, with no event (or comment, see `ActivityBarrier`) from any OTHER
// actor on that issue in between, first-to-last within `FOLD_WINDOW_MS`,
// collapses to its NET effect. A net of "nothing changed" (A → B → A)
// disappears entirely; a net A → C shows as ONE event carrying the first
// event's `from*` payload keys and the last event's everything else,
// timestamped at the LAST event and keyed by ITS id. `created`, `pr_opened`
// and `pr_merged` never fold (`foldFieldKey` → null) and never break a run of
// the same actor; a run spanning MORE than the window from first to last is
// left alone entirely (no partial fold). An event without an actor never
// folds and breaks every open run on its issue.
//
// "Field" derives from the event type: `status_changed` → status,
// `assignee_changed` → assignee, `priority_changed` → priority,
// `estimate_changed` → estimate, `board_moved` → board,
// `label_added`/`label_removed` → that ONE label (`payload.labelId`; an add
// and a remove of the same label cancel),
// `relation_added`/`relation_removed` → that one relation
// (`payload.type` + `payload.relatedIssueId`).
import type { IssueEvent } from "@/db/schema"

/** The row shape the fold reads: the synced `issue_events` columns. */
export type ActivityEvent = Pick<
  IssueEvent,
  `id` | `issueId` | `actorUserId` | `type` | `payload` | `createdAt`
>

/**
 * Something that is not an event but still breaks another actor's run: a
 * comment. "Reviewer says no" is usually a comment, not a status change, so
 * the timeline passes its comments here and the dev's progress → review →
 * progress round trip stays visible.
 */
export interface ActivityBarrier {
  issueId: string
  actorUserId: string | null
  createdAt: ActivityEvent[`createdAt`]
}

/** First-to-last span a run may cover and still fold. */
export const FOLD_WINDOW_MS = 10 * 60 * 1000

type Payload = Record<string, unknown>

function payloadOf(event: ActivityEvent): Payload {
  return (event.payload ?? {}) as Payload
}

function optionalString(value: unknown): string | null {
  return typeof value === `string` && value.length > 0 ? value : null
}

/** The field an event edits, or null for an event that never folds. */
export function foldFieldKey(event: ActivityEvent): string | null {
  const payload = payloadOf(event)
  switch (event.type) {
    case `status_changed`:
      return `status`
    case `assignee_changed`:
      return `assignee`
    case `priority_changed`:
      return `priority`
    case `estimate_changed`:
      return `estimate`
    case `board_moved`:
      return `board`
    case `label_added`:
    case `label_removed`: {
      const labelId = optionalString(payload.labelId)
      return labelId ? `label:${labelId}` : null
    }
    case `relation_added`:
    case `relation_removed`: {
      const type = optionalString(payload.type)
      const relatedIssueId = optionalString(payload.relatedIssueId)
      return type && relatedIssueId ? `relation:${type}:${relatedIssueId}` : null
    }
    default:
      return null
  }
}

function epoch(at: ActivityEvent[`createdAt`]): number {
  return new Date(at).getTime()
}

// The "before" side of the first event and the "after" side of the last one,
// as comparable strings. Equal = the run changed nothing. A status compares
// on the precise `statusId` pair when the payload carries one (EXP-314) and
// on the legacy enum otherwise; presence toggles (labels, relations) compare
// as present/absent.
function beforeOf(event: ActivityEvent): string {
  const payload = payloadOf(event)
  switch (event.type) {
    case `status_changed`:
      return String(payload.fromStatusId ?? payload.from ?? ``)
    case `assignee_changed`:
    case `priority_changed`:
    case `estimate_changed`:
      return String(payload.from ?? ``)
    case `board_moved`:
      return String(payload.fromBoardId ?? ``)
    case `label_added`:
    case `relation_added`:
      return `absent`
    case `label_removed`:
    case `relation_removed`:
      return `present`
    default:
      return ``
  }
}

function afterOf(event: ActivityEvent): string {
  const payload = payloadOf(event)
  switch (event.type) {
    case `status_changed`:
      return String(payload.toStatusId ?? payload.to ?? ``)
    case `assignee_changed`:
    case `priority_changed`:
    case `estimate_changed`:
      return String(payload.to ?? ``)
    case `board_moved`:
      return String(payload.toBoardId ?? ``)
    case `label_added`:
    case `relation_added`:
      return `present`
    case `label_removed`:
    case `relation_removed`:
      return `absent`
    default:
      return ``
  }
}

/** The last event wearing the first event's `from*` keys. */
function mergePayload(first: ActivityEvent, last: ActivityEvent): Payload {
  const merged: Payload = {}
  for (const [key, value] of Object.entries(payloadOf(last))) {
    if (!key.startsWith(`from`)) merged[key] = value
  }
  for (const [key, value] of Object.entries(payloadOf(first))) {
    if (key.startsWith(`from`)) merged[key] = value
  }
  return merged
}

interface Run {
  issueId: string
  actorUserId: string
  events: { event: ActivityEvent; index: number }[]
}

interface Out {
  event: ActivityEvent
  index: number
}

function flushRun(run: Run, out: Out[]) {
  const { events } = run
  if (events.length === 1) {
    out.push(events[0])
    return
  }
  const first = events[0]
  const last = events[events.length - 1]
  const span = epoch(last.event.createdAt) - epoch(first.event.createdAt)
  if (span > FOLD_WINDOW_MS) {
    out.push(...events)
    return
  }
  if (beforeOf(first.event) === afterOf(last.event)) return
  out.push({
    index: last.index,
    event: {
      ...last.event,
      payload: mergePayload(first.event, last.event),
    },
  })
}

/**
 * Input in chronological order (oldest first); output likewise. `barriers`
 * (comments) only ever break runs, they are never returned.
 */
export function foldActivity(
  events: ActivityEvent[],
  barriers: ActivityBarrier[] = []
): ActivityEvent[] {
  type Step =
    | { kind: `event`; at: number; index: number; event: ActivityEvent }
    | { kind: `barrier`; at: number; index: number; barrier: ActivityBarrier }
  const steps: Step[] = [
    ...events.map((event, index) => ({
      kind: `event` as const,
      at: epoch(event.createdAt),
      index,
      event,
    })),
    ...barriers.map((barrier, index) => ({
      kind: `barrier` as const,
      at: epoch(barrier.createdAt),
      // A barrier at the same instant as an event sorts BEFORE it, so a
      // comment written together with a change still separates it from
      // what follows.
      index: index - barriers.length,
      barrier,
    })),
  ]
  steps.sort((a, b) => a.at - b.at || a.index - b.index)

  // Open runs keyed by `${issueId}\0${actor}\0${field}`.
  const open = new Map<string, Run>()
  const out: Out[] = []

  const breakOthers = (issueId: string, actorUserId: string | null) => {
    for (const [key, run] of open) {
      if (run.issueId !== issueId) continue
      if (actorUserId !== null && run.actorUserId === actorUserId) continue
      flushRun(run, out)
      open.delete(key)
    }
  }

  for (const step of steps) {
    if (step.kind === `barrier`) {
      breakOthers(step.barrier.issueId, step.barrier.actorUserId)
      continue
    }
    const { event, index } = step
    const actor = event.actorUserId
    breakOthers(event.issueId, actor)
    const field = actor ? foldFieldKey(event) : null
    if (!actor || !field) {
      out.push({ event, index })
      continue
    }
    const key = `${event.issueId}\0${actor}\0${field}`
    const run = open.get(key)
    if (run) {
      run.events.push({ event, index })
    } else {
      open.set(key, {
        issueId: event.issueId,
        actorUserId: actor,
        events: [{ event, index }],
      })
    }
  }
  for (const run of open.values()) flushRun(run, out)

  out.sort(
    (a, b) =>
      epoch(a.event.createdAt) - epoch(b.event.createdAt) || a.index - b.index
  )
  return out.map((entry) => entry.event)
}
