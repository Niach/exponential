// EXP-746: the "Past" section under Devices — the caller's own FINISHED runs
// on the machines they own. The rules live here so the four clients agree:
// iOS PastRuns.swift, Android AgentsViewModel + Daos.kt, desktop
// queries::own_ended_runs. Same predicate, same ordering key, same cap, same
// byline, same test names.
//
// An AUTOMATED run (`started_reason` set — schedule, event or a
// sessions_start child) is NOT past work of the person: it belongs to the
// Automations tab's "Recent automated runs" (EXP-676), which is the only
// finished-runs list keyed on that column. Past is person-started runs only.

import type { CodingSession, Issue } from "@/db/schema"

/** EXP-746: how many Past rows a devices screen lists. ×4 lockstep. */
export const PAST_RUN_CAP = 20

type PastRunSession = Pick<
  CodingSession,
  | `id`
  | `status`
  | `startedReason`
  | `userId`
  | `teamId`
  | `issueId`
  | `actionName`
  | `branch`
  | `agent`
  | `endedBy`
  | `endedAt`
  | `updatedAt`
>

function stamp(value: Date | string | null | undefined): number {
  if (!value) return 0
  const at = typeof value === `string` ? new Date(value) : value
  const ms = at.getTime()
  return Number.isNaN(ms) ? 0 : ms
}

/** Ordering key: the end stamp, falling back to the heartbeat stamp for
 *  pre-EXP-637 rows that never stamped `ended_at`. ×4 lockstep. */
export function pastRunEndedAt(
  session: Pick<CodingSession, `endedAt` | `updatedAt`>
): number {
  return stamp(session.endedAt) || stamp(session.updatedAt)
}

/** The caller's OWN, PERSON-started, ENDED runs in one team, newest first,
 *  capped. `startedReason !== null` is an automation/agent run and belongs to
 *  the Automations tab, never here. */
export function selectPastRuns<T extends PastRunSession>(
  sessions: readonly T[],
  currentUserId: string | undefined,
  teamId: string | undefined,
  cap: number = PAST_RUN_CAP
): T[] {
  if (!currentUserId || !teamId) return []
  return sessions
    .filter(
      (session) =>
        session.status === `ended` &&
        session.startedReason == null &&
        session.userId === currentUserId &&
        session.teamId === teamId
    )
    .sort((a, b) => pastRunEndedAt(b) - pastRunEndedAt(a))
    .slice(0, cap)
}

/** The row's name: the issue's title, else the action snapshot (which
 *  survives the action's deletion), else the run's own kind. */
export function pastRunTitle(
  session: Pick<CodingSession, `issueId` | `actionName` | `branch`>,
  issue: Pick<Issue, `title`> | undefined
): string {
  if (session.issueId) return issue?.title ?? `Issue syncing…`
  if (session.actionName) return session.actionName
  // The launcher's own branch vocabulary: `exp/chat-<id8>` is a chat run,
  // `exp/batch-<id8>` (and anything else issue-less) a batch one.
  if (session.branch?.startsWith(`exp/chat-`)) return `Chat session`
  return `Batch session`
}

/** How the run ended, as the byline says it. NULL `ended_by` (a row ended by
 *  a pre-EXP-637 server) drops the clause rather than guessing. */
function endedByPhrase(endedBy: string | null | undefined): string | null {
  switch (endedBy) {
    case `agent`:
      return `agent`
    case `user`:
      return `you`
    case `client`:
      return `the app`
    case `merge`:
      return `a merge`
    case `system`:
      return `the system`
    default:
      return null
  }
}

/** The row's caption: `<device> · <agent label> · ended by <who> · <rel
 *  time>`. Every part is optional — an old row that named no device, no agent
 *  or no ender simply drops that segment instead of printing a placeholder.
 *  The agent label and the relative time are formatted by the caller (each
 *  client owns its own vocabulary and date formatter); the ORDER, the
 *  separator and the "ended by" wording are what the four clients share. */
export function pastRunByline(
  session: Pick<CodingSession, `endedBy`>,
  parts: {
    deviceLabel: string | null | undefined
    agentLabel: string | null | undefined
    relativeTime: string
  }
): string {
  const ended = endedByPhrase(session.endedBy)
  return [
    parts.deviceLabel,
    parts.agentLabel,
    ended ? `ended by ${ended}` : null,
    parts.relativeTime,
  ]
    .filter((part): part is string => Boolean(part && part.length > 0))
    .join(` · `)
}
