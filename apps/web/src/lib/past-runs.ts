// EXP-746: the "Recent" section (EXP-886, was "Past") under Devices — the caller's own FINISHED runs
// on the machines they own. The rules live here so the four clients agree:
// iOS PastRuns.swift, Android PastRuns.kt + AgentsViewModel, desktop
// queries::own_ended_runs + sessions_section::session_title. Same predicate,
// same ordering key, same cap, same title, same byline, same test names.
// EXP-886 adds `selectIssueRuns`, the issue's OWN runs behind the header's
// Run/Runs label and the session view's run switcher (same ×4 mirrors).
//
// An AUTOMATED run (`started_reason` set — schedule, event or a
// sessions_start child) is NOT past work of the person: it belongs to the
// Automations tab's "Recent automated runs" (EXP-676), which is the only
// finished-runs list keyed on that column. Recent is person-started runs only.

import type { CodingSession, Issue } from "@/db/schema"
import { batchRunName, type BatchRunIssue } from "@/lib/batch-run"

/** EXP-746: how many Recent rows a devices screen lists. ×4 lockstep. */
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

/** The end facts every "is this run over?" decision reads. Structural on
 *  purpose (`endedBy` optional) so a caller holding only a partial row still
 *  passes one. ×4. */
export type RunEndFacts = {
  status: string
  endedBy?: string | null
}

/** EXP-888: the staleness sweep's end — `ended_by = 'stale'`. The sweep flips
 *  a silent row to `ended` only when its host device advertises the
 *  `stale-end` cap; that device IGNORES the flip and its next heartbeat
 *  (up to 30 min later) revives the row to `running`. So a sweep end says
 *  "silent for the staleness window", NEVER "this run is over". ×4. */
export function runIsStaleEnd(session: RunEndFacts): boolean {
  return session.status === `ended` && session.endedBy === `stale`
}

/** THE predicate: whether the run is OVER. `runIsLive` is its negation
 *  (`status !== 'ended' || endedBy === 'stale'`). Every Running-vs-Past,
 *  Stop-vs-Resume and composer-enabled decision goes through this and never
 *  through `status === 'ended'` alone — otherwise a swept-but-alive run lists
 *  as past, greys its composer out and offers Resume, which would put a
 *  SECOND agent on the same worktree. Byte-identical ×4 (desktop
 *  `run_rows::run_has_ended`, iOS `PastRuns.hasEnded`, Android
 *  `runHasEnded`). */
export function runHasEnded(session: RunEndFacts): boolean {
  return session.status === `ended` && !runIsStaleEnd(session)
}

/** The negation of [`runHasEnded`] — the run may still be alive. ×4. */
export function runIsLive(session: RunEndFacts): boolean {
  return !runHasEnded(session)
}

/** The caller's OWN, PERSON-started, ENDED runs in one team, newest first,
 *  capped. `startedReason !== null` is an automation/agent run and belongs to
 *  the Automations tab, never here. A sweep end (EXP-888) is not an end, so
 *  those rows stay OUT of Recent and keep their place in Running. */
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
        runHasEnded(session) &&
        session.startedReason == null &&
        session.userId === currentUserId &&
        session.teamId === teamId
    )
    .sort((a, b) => pastRunEndedAt(b) - pastRunEndedAt(a))
    .slice(0, cap)
}

/** A run that is alive by STATUS — running or in review. Staleness is not
 *  consulted here: a run whose machine went quiet is still one of the issue's
 *  runs, it only sorts by its last heartbeat. */
export function isLiveRunStatus(status: string): boolean {
  return status === `running` || status === `in_review`
}

/** The row-level twin of [`isLiveRunStatus`]: live by status, plus EXP-888's
 *  sweep end, which is a live run whose heartbeat has not landed yet. Every
 *  list that paints a live badge or sorts live runs first reads THIS. ×4
 *  (desktop `queries::is_live_run_status`, iOS `PastRuns.isLiveRun`, Android
 *  `isLiveRun`). */
export function isLiveRun(session: RunEndFacts): boolean {
  return runIsStaleEnd(session) || isLiveRunStatus(session.status)
}

/** The word the run switcher shows in a live run's time slot, in place of
 *  the ended relative time. Byte-identical ×4. */
export const LIVE_RUN_LABEL = `Live`

/** EXP-886: an issue's RUNS — the caller's OWN runs of THAT issue, whatever
 *  their status and started reason (a live run, an ended one, an automated
 *  one: all of them are the issue's), UNCAPPED. Live runs first, then by the
 *  Recent ordering key (`pastRunEndedAt`: the end, else the heartbeat). Backs
 *  the work header's `Run`/`Runs` face label and the session view's run
 *  switcher. A batch run (`issueId` NULL) never matches. ×4 lockstep: the
 *  native twins mirror this predicate, order and test names. */
export function selectIssueRuns<T extends PastRunSession>(
  sessions: readonly T[],
  currentUserId: string | undefined,
  issueId: string | undefined
): T[] {
  if (!currentUserId || !issueId) return []
  const live = (session: T) => (isLiveRun(session) ? 1 : 0)
  return sessions
    .filter(
      (session) =>
        session.userId === currentUserId && session.issueId === issueId
    )
    .sort(
      (a, b) => live(b) - live(a) || pastRunEndedAt(b) - pastRunEndedAt(a)
    )
}

/** The row's name: the issue's title, else the action snapshot (which
 *  survives the action's deletion and is how a chat run reads "Chat" — the
 *  name is reserved for it, EXP-615), else the batch's own name (EXP-876: its
 *  first covered issue's title, else `Batch run`). Every fallback string is
 *  byte-identical ×4 (iOS `PastRuns.title`, Android `pastRunTitle`, desktop
 *  `sessions_section::session_title`), so the same ended row is named the same
 *  on every client; test `a row titles itself from whatever it has`. */
export function pastRunTitle(
  session: Pick<
    CodingSession,
    `issueId` | `actionName` | `batchIssueIds` | `branch`
  >,
  issue: Pick<Issue, `title`> | undefined,
  /** EXP-876: the issues a batch row may name itself after — whatever the
   *  caller has synced. Absent = the generic label. */
  batchIssues: readonly BatchRunIssue[] = []
): string {
  if (issue) return issue.title.trim() || `Untitled issue`
  // An issue-scoped run whose issue row has not landed yet.
  if (session.issueId) return `Issue syncing…`
  if (session.actionName?.trim()) return session.actionName.trim()
  return batchRunName(session, batchIssues).subject
}

/** EXP-876: the row's mono lead-in — the issue's identifier, a batch's
 *  `EXP-874 +2`, else none. The twin of `pastRunTitle`, ×4 lockstep. */
export function pastRunIdentifier(
  session: Pick<
    CodingSession,
    `issueId` | `actionName` | `batchIssueIds` | `branch`
  >,
  issue: Pick<Issue, `identifier`> | undefined,
  batchIssues: readonly BatchRunIssue[] = []
): string | null {
  if (issue) return issue.identifier
  if (session.issueId || session.actionName != null) return null
  return batchRunName(session, batchIssues).identifier
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
