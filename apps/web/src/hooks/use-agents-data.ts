import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  codingSessionCollection,
  deviceCollection,
  issueCollection,
} from "@/lib/collections"
import {
  useBoardsForTeams,
  useTeamBoards,
  useTeamUsers,
} from "@/hooks/use-team-data"
import { teamScopeIds } from "@/lib/team-scope"
import type { CodingSession, Device, Issue, Board, User } from "@/db/schema"
import { isCodingSessionStale } from "@exp/db-schema/domain"
import { useNow } from "@/hooks/use-now"
import { sessionDisplayState } from "@/lib/coding-session-display"
import {
  resolveSessionDevice,
  sessionIsPaused,
  type SessionDevice,
} from "@/lib/session-device"
import { deviceCanResumeRun, deviceRowIsOnline } from "@/lib/steer-devices"
import {
  isLiveRun,
  pastRunIdentifier,
  pastRunTitle,
  selectIssueRuns,
  selectPastRuns,
  PAST_RUN_CAP,
} from "@/lib/past-runs"
import { batchRunIssues, isBatchRun } from "@/lib/batch-run"
import { runChain } from "@/lib/sessions/run-chain"

export {
  resolveSessionMergeTarget,
  type SessionMergeTarget,
} from "@/lib/session-merge-target"
import {
  resolveSessionMergeTarget,
  type SessionMergeTarget,
} from "@/lib/session-merge-target"

/** A run's own PR as its merge target, whatever its subject: the fallback of
 * the rows held past their live listing while their issue is not synced. */
function sessionPrTarget(
  session: CodingSession
): SessionMergeTarget | undefined {
  return session.prUrl && session.prNumber != null
    ? { kind: `session`, session }
    : undefined
}

/** The props both merge paths hand `SessionMergeButton` — one shape so the
 * three call sites (Agents row, steering strip, dock) never re-derive it.
 * EXP-917: NO `teamId` here — a synced issue row has none (the board-scoped
 * `issues` shape drops it), and the "Fix conflicts" swap once gated on it,
 * which made the swap unreachable from every issue-fed surface. */
export interface SessionMergeTargetProps {
  issueId?: string
  sessionId?: string
  prState: string | null
  prNumber: number | null
  branch: string | null
  updatedAt: string | Date | null
}

export function mergeTargetProps(
  target: SessionMergeTarget
): SessionMergeTargetProps {
  if (target.kind === `issue`) {
    const { issue } = target
    return {
      issueId: issue.id,
      prState: issue.prState,
      prNumber: issue.prNumber,
      branch: issue.branch,
      updatedAt: issue.updatedAt,
    }
  }
  const { session } = target
  return {
    sessionId: session.id,
    prState: session.prState,
    prNumber: session.prNumber,
    branch: session.branch,
    updatedAt: session.updatedAt,
  }
}

/** The PR state a row renders by: the linked issue's, else the run's own
 * chore PR (EXP-734). */
export function rowPrState(
  session: CodingSession,
  issue: Issue | undefined
): string | null {
  return issue?.prState ?? session.prState
}

export interface AgentSessionRow {
  session: CodingSession
  /** May be undefined while the issue row is still syncing. */
  issue: Issue | undefined
  /** EXP-876: a BATCH row's covered issues, in naming order — empty on every
   * other subject, and on a batch whose issues are not knowable yet. */
  batchIssues: Issue[]
  board: Board | undefined
  /** May be undefined while the user row is still syncing — render via displayUserName. */
  user: User | undefined
  /** EXP-734: what the row's Merge control acts on — the linked issue, or
   * the run row itself once it stamped its own PR (an issue-less run: batch,
   * action or chat). Absent = no PR to merge. */
  mergeTarget: SessionMergeTarget | undefined
  /** EXP-549/550: the host machine as the synced devices row knows it (the
   * RENAMED label, live online-ness) — falls back to the row's snapshot. */
  device: SessionDevice
  /** EXP-550: running/needs-input on an OFFLINE machine — the agent is
   * parked and resumes when the device returns; render grey, never live. */
  paused: boolean
}

/**
 * EXP-876: the issues every BATCH row in `sessions` may name itself after.
 * Two sources, two live queries: the ids the rows stored at start, and — for
 * rows that predate the column or came from a client too old to send it — the
 * issues sharing the branch `pr_open` stamped on both sides (EXP-545). Both
 * keyed on a SORTED dep string, the idiom the rest of this file uses, so an
 * unchanged id set never churns the query.
 *
 * Returns the per-row resolver, so each list hands its rows only their own
 * covered issues instead of the whole pool.
 */
function useBatchIssues(
  sessions: readonly CodingSession[]
): (session: CodingSession) => Issue[] {
  const batches = useMemo(() => sessions.filter(isBatchRun), [sessions])
  const coveredIds = useMemo(() => {
    const ids = [
      ...new Set(batches.flatMap((session) => session.batchIssueIds ?? [])),
    ]
    ids.sort()
    return ids
  }, [batches])

  const { data: coveredRows } = useLiveQuery(
    (query) =>
      coveredIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, coveredIds))
        : undefined,
    [coveredIds.join(`,`)]
  )

  return useMemo(() => {
    const pool = (coveredRows ?? []) as Issue[]
    if (pool.length === 0) return () => []
    return (session: CodingSession) => batchRunIssues(session, pool)
  }, [coveredRows])
}

// Team Agents page + dock data: the caller's OWN live coding sessions in the
// team (synced coding_sessions shape, team-scoped by the denormalized
// team_id), joined client-side to their issue / board / driving user,
// newest-first. Live = `running` OR `in_review` (EXP-194: the
// agent's PR is open, terminal still alive awaiting review — consumers read
// `session.status` to render "Ready for review" vs "Coding now"). Ended
// sessions dropped out with the redesign — the live trail lives on each
// issue, and the dock/Agents page only surface live work.
// EXP-312 follow-up: a live session is viewable/steerable only by its owner,
// so a teammate's row in these lists could only ever read as "unavailable".
// Such rows are filtered out here entirely; they still sync, and still
// surface as status badges on issue detail and in the Reviews queue.
// Both params stay REQUIRED (optional-typed, not optional-arity) so no future
// caller can silently ask for the whole team's sessions again.
// EXP-1186: `teamId` may name SEVERAL teams — the phone's Agent page lists
// my runs across every member team.
export function useAgentsData(
  teamId: string | readonly string[] | undefined,
  currentUserId: string | undefined
) {
  const teamIds = teamScopeIds(teamId)
  const teamKey = teamIds.join(`,`)
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0 && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                inArray(sessions.teamId, teamIds),
                eq(sessions.userId, currentUserId)
              )
            )
        : undefined,
    [teamKey, currentUserId]
  )
  const sessions = useMemo(
    () => (sessionRows ?? []) as CodingSession[],
    [sessionRows]
  )

  // Sorted so the same id set always yields the same dep string (no query
  // churn from heap-order flips).
  const issueIds = useMemo(() => {
    const ids = [...new Set(sessions.map((session) => session.issueId))]
    ids.sort()
    return ids
  }, [sessions])

  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )

  // EXP-549/550: the caller's own + team-shared device rows (the devices
  // shape is already server-scoped) resolve each session's live label and
  // online-ness. Ticks every 30 s against the 90 s online window (the
  // use-remote-start idiom) — also fine-grained enough for the staleness
  // guard below.
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      teamIds.length > 0 && currentUserId
        ? query.from({ d: deviceCollection })
        : undefined,
    [teamKey, currentUserId]
  )
  const devices = useMemo(
    () => (deviceRows ?? []) as Device[],
    [deviceRows]
  )

  const { boards } = useBoardsForTeams(teamIds)
  // Own runs only, so the roster of any one team holds their user.
  const { userMap } = useTeamUsers(teamIds[0])
  const now = useNow(30_000)
  // EXP-876: what names a batch row.
  const resolveBatchIssues = useBatchIssues(sessions)

  return useMemo(() => {
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const boardMap = new Map(boards.map((board) => [board.id, board]))

    const toRow = (session: CodingSession): AgentSessionRow => {
      // Batch-scoped sessions carry no issue — render issueless.
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      const device = resolveSessionDevice(session, devices, now)
      const batchIssues = resolveBatchIssues(session)
      return {
        session,
        issue,
        batchIssues,
        board: issue ? boardMap.get(issue.boardId) : undefined,
        user: userMap.get(session.userId),
        mergeTarget: resolveSessionMergeTarget(session, issue, batchIssues),
        device,
        paused: sessionIsPaused(
          sessionDisplayState(session, rowPrState(session, issue)),
          session.status,
          device
        ),
      }
    }

    // Staleness guard (EXP-153): heartbeat-dead rows render as absent
    // (not "ended" — swept rows leave no recap entry either).
    // EXP-888: `isLiveRun` keeps a SWEPT row (`ended` + `ended_by = 'stale'`)
    // in Running — the sweep's flip is not an end, the device ignores it and
    // its next heartbeat revives the row.
    const running = sessions
      .filter(
        (session) =>
          isLiveRun(session) && !isCodingSessionStale(session.updatedAt, now)
      )
      .sort(
        (a, b) =>
          new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()
      )
      .map(toRow)

    return {
      running,
      // Without a team id or a signed-in user the query is skipped and can
      // never deliver a snapshot — treat that as ready-empty instead of
      // loading forever.
      isLoading: !isReady && Boolean(teamIds.length > 0 && currentUserId),
    }
    // `teamKey` stands for `teamIds` (a fresh array per render).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    sessions,
    issueRows,
    resolveBatchIssues,
    boards,
    userMap,
    devices,
    isReady,
    teamKey,
    currentUserId,
    now,
  ])
}

// ── One session by id (EXP-740) ──────────────────────────────────────────────

/**
 * EXP-740: the row behind `/t/$teamSlug/sessions/$sessionId`. The session
 * route has only an id, but everything the view renders (the issue join, the
 * Merge target, the device label) is row-shaped — so this resolves ONE id to
 * the same `AgentSessionRow` the strip and the Devices list carry.
 *
 * A RUNNING row comes straight off `useAgentsData` (joins included). Anything
 * else — a run that ended while the page was open, a deliberately opened
 * finished run — is queried by id in ANY status and synthesized here, so the
 * page stays readable instead of blanking the moment the run stops.
 *
 * EXP-312 stays the caller's job: this resolves a row, it does not decide who
 * may steer it. The team guard below is only scoping — a session id from
 * another team must not render under this team's slug.
 */
export function useSessionRow(
  teamId: string | undefined,
  currentUserId: string | undefined,
  sessionId: string | undefined
): {
  row: AgentSessionRow | null
  session: CodingSession | null
  /** The by-id query delivered a snapshot — `session === null` then means
   *  "no such session", not "still loading". */
  isReady: boolean
} {
  const { running } = useAgentsData(teamId, currentUserId)
  const boards = useTeamBoards(teamId)

  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      sessionId
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.id, sessionId))
        : undefined,
    [sessionId]
  )
  const found = ((sessionRows ?? []) as CodingSession[])[0] ?? null
  const session = found && found.teamId === teamId ? found : null

  const runningById = useMemo(
    () => new Map(running.map((row) => [row.session.id, row])),
    [running]
  )

  // The issue join only when the row is NOT among the running ones — those
  // already carry it.
  const needsIssueJoin = Boolean(
    session && session.issueId && !runningById.has(session.id)
  )
  const { data: issueRows } = useLiveQuery(
    (query) =>
      needsIssueJoin && session?.issueId
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => eq(i.id, session.issueId))
        : undefined,
    [needsIssueJoin, session?.issueId]
  )

  // EXP-876: what names a batch row — a run opened past its live listing is
  // exactly where "Batch run" used to be all the header had to say.
  const ownSessions = useMemo(() => (session ? [session] : []), [session])
  const resolveBatchIssues = useBatchIssues(ownSessions)

  const row = useMemo<AgentSessionRow | null>(() => {
    if (!session) return null
    const existing = runningById.get(session.id)
    if (existing) return existing
    const issue = session.issueId
      ? ((issueRows ?? [])[0] as Issue | undefined)
      : undefined
    const board = issue ? boards.find((b) => b.id === issue.boardId) : undefined
    const batchIssues = resolveBatchIssues(session)
    return {
      session,
      issue,
      batchIssues,
      board,
      user: undefined,
      // EXP-734: an issue-less run held open past its live listing still
      // carries its OWN chore PR on the row — keep its Merge pill.
      mergeTarget:
        resolveSessionMergeTarget(session, issue, batchIssues) ??
        sessionPrTarget(session),
      // A row resolved past its live listing: the snapshot label suffices and
      // nothing there is "paused" — the session view resolves the live device
      // itself.
      device: { label: session.deviceLabel, online: null },
      paused: false,
    }
  }, [session, runningById, issueRows, resolveBatchIssues, boards])

  return {
    row,
    session,
    // Without a session id the query is skipped and can never deliver a
    // snapshot — treat that as ready-empty rather than loading forever.
    isReady: sessionId ? isReady : true,
  }
}

// ── Past runs (EXP-746) ──────────────────────────────────────────────────────

/** One row of the Agent page's "Recent" band (and the run switcher's entries). */
export interface PastRunRow {
  session: CodingSession
  /** May be undefined while the issue row is still syncing (or for a
   * batch/action/chat run, which links none). */
  issue: Issue | undefined
  /** EXP-876: a BATCH row's covered issues, in naming order. */
  batchIssues: Issue[]
  board: Board | undefined
  /** EXP-549: the host machine as the synced devices row knows it. */
  device: SessionDevice
  /** EXP-637: that machine is online and advertises `resume-run`. */
  canResume: boolean
  title: string
  /** The issue's identifier — or a batch's `EXP-874 +2` — for the row's mono
   * lead-in. */
  identifier: string | null
}

/**
 * EXP-746: the caller's OWN finished, PERSON-started runs in one team — the
 * "Recent" band (EXP-886, was "Past") on the Agent page, mirrored on iOS, Android and the desktop.
 * Automated runs (`started_reason` set) belong to their action's Runs
 * (EXP-676, SLOP-2) and are filtered out by `selectPastRuns`.
 *
 * Known scoping caveat: the coding-sessions shape is team-scoped with the
 * static trash/archive predicate (`buildTeamScopedChildWhere`), so an ended
 * ISSUE run whose board was trashed or archived stops syncing and silently
 * drops out of Recent. Batch/action/chat rows keep NULL board mirrors and
 * always sync. Ended rows survive the sweep either way —
 * `coding-session-sweep.ts` only deletes `running`/`in_review`.
 */
export function usePastRuns(
  /** EXP-1186: one team, or several (the phone's Recent sheet). */
  teamId: string | readonly string[] | undefined,
  currentUserId: string | undefined,
  options?: {
    /** EXP-739: narrow the list BEFORE the 20-row cap — the chat page's
     * "Past chats" wants the newest 20 CHATS, not whatever chats survive the
     * newest 20 runs. Pass a module-level predicate: it is a memo dependency. */
    only?: (session: CodingSession) => boolean
  }
) {
  const only = options?.only
  const teamIds = teamScopeIds(teamId)
  const teamKey = teamIds.join(`,`)
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      teamIds.length > 0 && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                inArray(sessions.teamId, teamIds),
                eq(sessions.userId, currentUserId),
                eq(sessions.status, `ended`)
              )
            )
        : undefined,
    [teamKey, currentUserId]
  )
  const past = useMemo(() => {
    const rows = (sessionRows ?? []) as CodingSession[]
    return selectPastRuns(
      only ? rows.filter(only) : rows,
      currentUserId,
      teamIds,
      PAST_RUN_CAP
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionRows, currentUserId, teamKey, only])

  const { rows, isLoading } = usePastRunRows(teamIds, currentUserId, past, isReady)
  return { past: rows, isLoading }
}

/**
 * EXP-886: an issue's RUNS — the caller's OWN runs of THAT issue, live and
 * ended alike, live first then newest end first, UNCAPPED (`selectIssueRuns`).
 * Feeds the session view's run switcher; the rows carry the same joins as
 * Recent's (device label for the byline), so `pastRunRowByline` applies.
 */
export function useIssueRuns(
  issueId: string | undefined,
  teamId: string | undefined,
  currentUserId: string | undefined
) {
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      issueId && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.issueId, issueId),
                eq(sessions.userId, currentUserId)
              )
            )
        : undefined,
    [issueId, currentUserId]
  )
  const runs = useMemo(
    () =>
      selectIssueRuns(
        (sessionRows ?? []) as CodingSession[],
        currentUserId,
        issueId
      ),
    [sessionRows, currentUserId, issueId]
  )
  const { rows, isLoading } = usePastRunRows(teamId, currentUserId, runs, isReady)
  return { runs: rows, isLoading }
}

/**
 * EXP-974: a run's RESUME CHAIN — the rows `resumed_from_id` links into one
 * succession (`runChain`, oldest-first), served NEWEST FIRST like the run
 * menu's other source. An issue-bound run's menu reads `useIssueRuns` (the
 * issue's own runs already include its resumes); an ISSUE-LESS run (chat,
 * action, batch) has no issue to list runs under, so its predecessor and
 * successor come from here — the "Continues an earlier run" band that used
 * to be the only link between them is gone. Same joins as Recent's rows.
 */
export function useRunChain(
  sessionId: string | undefined,
  teamId: string | undefined,
  currentUserId: string | undefined
) {
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      sessionId && teamId && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                eq(sessions.userId, currentUserId)
              )
            )
        : undefined,
    [sessionId, teamId, currentUserId]
  )
  const chain = useMemo(
    () =>
      sessionId
        ? runChain((sessionRows ?? []) as CodingSession[], sessionId).reverse()
        : [],
    [sessionRows, sessionId]
  )
  const { rows, isLoading } = usePastRunRows(teamId, currentUserId, chain, isReady)
  return { runs: rows, isLoading }
}

/** Joins selected ended runs into `PastRunRow`s: issue, board, live device
 *  label and whether that machine can resume the run. */
function usePastRunRows(
  teamId: string | readonly string[] | undefined,
  currentUserId: string | undefined,
  past: readonly CodingSession[],
  isReady: boolean
) {
  // Sorted so the same id set always yields the same dep string (the
  // useAgentsData idiom).
  const issueIds = useMemo(() => {
    const ids = [
      ...new Set(
        past
          .map((session) => session.issueId)
          .filter((id): id is string => id !== null)
      ),
    ]
    ids.sort()
    return ids
  }, [past])

  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )

  const teamIds = teamScopeIds(teamId)
  const teamKey = teamIds.join(`,`)
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      teamIds.length > 0 && currentUserId
        ? query.from({ d: deviceCollection })
        : undefined,
    [teamKey, currentUserId]
  )
  const devices = useMemo(() => (deviceRows ?? []) as Device[], [deviceRows])

  const { boards } = useBoardsForTeams(teamIds)
  const now = useNow(30_000)
  // EXP-876: what names a batch row.
  const resolveBatchIssues = useBatchIssues(past)

  return useMemo(() => {
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const boardMap = new Map(boards.map((board) => [board.id, board]))
    // EXP-637: Resume relaunches the run on the machine that still holds its
    // worktree — hide the button when that machine is offline or too old to
    // resume, rather than failing after the click.
    const resumableDeviceIds = new Set(
      devices
        .filter(
          (device) =>
            deviceRowIsOnline(device.lastSeenAt, now) &&
            deviceCanResumeRun({ caps: device.caps })
        )
        .map((device) => device.deviceId)
    )
    const rows: PastRunRow[] = past.map((session) => {
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      // EXP-876: a batch row names itself after the issues it covered.
      const batchIssues = resolveBatchIssues(session)
      return {
        session,
        issue,
        batchIssues,
        board: issue ? boardMap.get(issue.boardId) : undefined,
        device: resolveSessionDevice(session, devices, now),
        canResume: Boolean(
          session.deviceId && resumableDeviceIds.has(session.deviceId)
        ),
        title: pastRunTitle(session, issue, batchIssues),
        identifier: pastRunIdentifier(session, issue, batchIssues),
      }
    })
    return {
      rows,
      // Without a team id or a signed-in user the query is skipped and can
      // never deliver a snapshot — ready-empty, not loading forever.
      isLoading: !isReady && Boolean(teamIds.length > 0 && currentUserId),
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    past,
    issueRows,
    resolveBatchIssues,
    boards,
    devices,
    now,
    isReady,
    teamKey,
    currentUserId,
  ])
}

// ── Arbitrary session rows (EXP-874) ─────────────────────────────────────────

/** What the shared session list rows (`components/session-list-rows.tsx`)
 * render off — an `AgentSessionRow` minus the user join. */
export type SessionListRow = Pick<
  AgentSessionRow,
  | `session`
  | `issue`
  | `batchIssues`
  | `board`
  | `device`
  | `paused`
  | `mergeTarget`
>

/**
 * EXP-874: joins an ARBITRARY set of session rows (an action's runs — not
 * the caller's own person-started ones `useAgentsData` serves) into row shape: the issue + board, the live device label and
 * online-ness, and the Merge target (the issue, else the run's own chore PR),
 * plus the issues a batch row names itself after (EXP-876).
 * A batch run's representative-issue lookup is `useAgentsData`'s alone.
 */
export function useSessionListRows(
  teamId: string | undefined,
  sessions: readonly CodingSession[]
): SessionListRow[] {
  const issueIds = useMemo(() => {
    const ids = [
      ...new Set(
        sessions
          .map((session) => session.issueId)
          .filter((id): id is string => id !== null)
      ),
    ]
    ids.sort()
    return ids
  }, [sessions])
  const { data: issueRows } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.id, issueIds))
        : undefined,
    [issueIds.join(`,`)]
  )
  const { data: deviceRows } = useLiveQuery(
    (query) => (teamId ? query.from({ d: deviceCollection }) : undefined),
    [teamId]
  )
  const boards = useTeamBoards(teamId)
  const now = useNow(30_000)
  // EXP-876: what names a batch row.
  const resolveBatchIssues = useBatchIssues(sessions)

  return useMemo(() => {
    const devices = (deviceRows ?? []) as Device[]
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const boardMap = new Map(boards.map((board) => [board.id, board]))
    return sessions.map((session) => {
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      const device = resolveSessionDevice(session, devices, now)
      const batchIssues = resolveBatchIssues(session)
      return {
        session,
        issue,
        batchIssues,
        board: issue ? boardMap.get(issue.boardId) : undefined,
        device,
        paused:
          session.status !== `ended` &&
          sessionIsPaused(
            sessionDisplayState(session, rowPrState(session, issue)),
            session.status,
            device
          ),
        mergeTarget:
          resolveSessionMergeTarget(session, issue, batchIssues) ??
          sessionPrTarget(session),
      } satisfies SessionListRow
    })
  }, [sessions, issueRows, deviceRows, resolveBatchIssues, boards, now])
}
