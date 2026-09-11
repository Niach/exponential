// EXP-746: the "Past" section under Devices — the caller's own FINISHED runs
// on the machines they own. The rules live here so the four clients agree:
// iOS PastRuns.swift, Android PastRuns.kt + AgentsViewModel, desktop
// queries::own_ended_runs + sessions_section::session_title. Same predicate,
// same ordering key, same cap, same title, same byline, same test names.
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
 *  survives the action's deletion and is how a chat run reads "Chat" — the
 *  name is reserved for it, EXP-615), else the batch. Every fallback string is
 *  byte-identical ×4 (iOS `PastRuns.title`, Android `pastRunTitle`, desktop
 *  `sessions_section::session_title`), so the same ended row is named the same
 *  on every client; test `a row titles itself from whatever it has`. */
export function pastRunTitle(
  session: Pick<CodingSession, `issueId` | `actionName`>,
  issue: Pick<Issue, `title`> | undefined
): string {
  if (issue) return issue.title.trim() || `Untitled issue`
  // An issue-scoped run whose issue row has not landed yet.
  if (session.issueId) return `Issue syncing…`
  return session.actionName?.trim() || `Batch run`
}

/** The row's caption: `<device> · <rel time>`. Both parts are optional — an
 *  old row that named no device, or one that stamped no time, simply drops
 *  that segment instead of printing a placeholder. The relative time is
 *  formatted by the caller (each client owns its date formatter); the ORDER
 *  and the separator are what the four clients share. EXP-833 dropped the
 *  agent and the "ended by" clause: the list's right side had grown wider
 *  than its titles, and the agent already shows as the row's lead glyph. */
export function pastRunByline(parts: {
  deviceLabel: string | null | undefined
  relativeTime: string
}): string {
  return [parts.deviceLabel, parts.relativeTime]
    .filter((part): part is string => Boolean(part && part.length > 0))
    .join(` · `)
}
