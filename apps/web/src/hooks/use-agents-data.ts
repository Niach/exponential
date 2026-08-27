import { useMemo } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  codingSessionCollection,
  deviceCollection,
  issueCollection,
} from "@/lib/collections"
import { useTeamBoards, useTeamUsers } from "@/hooks/use-team-data"
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

export interface AgentSessionRow {
  session: CodingSession
  /** May be undefined while the issue row is still syncing. */
  issue: Issue | undefined
  board: Board | undefined
  /** May be undefined while the user row is still syncing — render via displayUserName. */
  user: User | undefined
  /** EXP-535: a batch session's resolved open PR, as a representative linked
   * issue (merging through it merges the ONE batch PR — Reviews pattern).
   * Set only on issueless batch rows in review whose OWN PR (matched by the
   * stamped session branch, EXP-545) is open and unambiguous. */
  batchPrIssue: Issue | undefined
  /** EXP-549/550: the host machine as the synced devices row knows it (the
   * RENAMED label, live online-ness) — falls back to the row's snapshot. */
  device: SessionDevice
  /** EXP-550: running/needs-input on an OFFLINE machine — the agent is
   * parked and resumes when the device returns; render grey, never live. */
  paused: boolean
  /** EXP-637: the run can be relaunched where it ran — its device is known,
   * online, and advertises `resume-run`. Only meaningful on ENDED rows. */
  canResume: boolean
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
export function useAgentsData(
  teamId: string | undefined,
  currentUserId: string | undefined
) {
  const { data: sessionRows, isReady } = useLiveQuery(
    (query) =>
      teamId && currentUserId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) =>
              and(
                eq(sessions.teamId, teamId),
                eq(sessions.userId, currentUserId)
              )
            )
        : undefined,
    [teamId, currentUserId]
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

  // EXP-535: batch sessions carry no issue linkage, so a batch row resolves
  // its open PR client-side: the team's open-PR issues on an `exp/batch-`
  // branch, collapsed by prUrl like Reviews, then matched to the branch the
  // pr_open flip stamped on the row (EXP-545). Team scoping rides the board
  // join below (the issues shape drops team_id). Queried only while an
  // issueless, actionless in-review batch row actually needs it — this hook
  // also backs the always-mounted dock.
  const needsBatchPr = useMemo(
    () =>
      sessions.some(
        (session) =>
          !session.issueId &&
          session.actionName == null &&
          session.status === `in_review`
      ),
    [sessions]
  )
  const { data: openPrIssueRows } = useLiveQuery(
    (query) =>
      teamId && needsBatchPr
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.prState, `open`))
        : undefined,
    [teamId, needsBatchPr]
  )

  // EXP-549/550: the caller's own + team-shared device rows (the devices
  // shape is already server-scoped) resolve each session's live label and
  // online-ness. Ticks every 30 s against the 90 s online window (the
  // use-remote-start idiom) — also fine-grained enough for the staleness
  // guard below.
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      teamId && currentUserId ? query.from({ d: deviceCollection }) : undefined,
    [teamId, currentUserId]
  )
  const devices = useMemo(
    () => (deviceRows ?? []) as Device[],
    [deviceRows]
  )

  const boards = useTeamBoards(teamId)
  const { userMap } = useTeamUsers(teamId)
  const now = useNow(30_000)

  return useMemo(() => {
    const issueMap = new Map(
      ((issueRows ?? []) as Issue[]).map((issue) => [issue.id, issue])
    )
    const boardMap = new Map(boards.map((board) => [board.id, board]))

    // The team's open batch PRs, one representative (newest) issue per
    // distinct prUrl. A session resolves ITS OWN PR by the branch the
    // pr_open batch flip stamped on the row (EXP-545) — matching by "the
    // team's sole open batch PR" alone could target a teammate's PR once
    // the session's own PR closed unmerged. Pre-stamp branchless rows have
    // drained (EXP-546), so a NULL branch resolves nothing and shows no
    // Merge shortcut — Reviews still lists every PR.
    const batchPrByUrl = new Map<string, Issue>()
    for (const issue of (openPrIssueRows ?? []) as Issue[]) {
      if (!issue.prUrl || !issue.branch?.startsWith(`exp/batch-`)) continue
      if (!boardMap.has(issue.boardId)) continue
      const current = batchPrByUrl.get(issue.prUrl)
      if (
        !current ||
        new Date(issue.createdAt).getTime() >
          new Date(current.createdAt).getTime()
      ) {
        batchPrByUrl.set(issue.prUrl, issue)
      }
    }
    const batchPrReps = [...batchPrByUrl.values()]
    const resolveBatchPr = (sessionBranch: string | null): Issue | undefined => {
      if (!sessionBranch) return undefined
      const matches = batchPrReps.filter(
        (issue) => issue.branch === sessionBranch
      )
      return matches.length === 1 ? matches[0] : undefined
    }

    const toRow = (session: CodingSession): AgentSessionRow => {
      // Batch-scoped sessions carry no issue — render issueless.
      const issue = session.issueId ? issueMap.get(session.issueId) : undefined
      // EXP-535: an issueless, actionless batch run whose PR is open
      // (status in_review — flipped in the pr_open transaction) gets the
      // resolved batch PR for its Merge button.
      const isBatch = !session.issueId && session.actionName == null
      const device = resolveSessionDevice(session, devices, now)
      return {
        session,
        issue,
        board: issue ? boardMap.get(issue.boardId) : undefined,
        user: userMap.get(session.userId),
        batchPrIssue:
          isBatch && session.status === `in_review`
            ? resolveBatchPr(session.branch)
            : undefined,
        device,
        paused: sessionIsPaused(
          sessionDisplayState(session, issue?.prState),
          device
        ),
        canResume: resolvesToResumableDevice(session),
      }
    }

    // EXP-637: Resume relaunches the run on the machine that still holds its
    // worktree, so the row's own device has to be online AND advertise
    // `resume-run` (steer.startSession refuses otherwise — hiding the button
    // beats failing after the tap).
    const resolvesToResumableDevice = (session: CodingSession): boolean => {
      if (!session.deviceId) return false
      const matches = devices.filter((d) => d.deviceId === session.deviceId)
      const row = matches.find((d) => d.userId === session.userId) ?? matches[0]
      if (!row) return false
      return (
        deviceRowIsOnline(row.lastSeenAt, now) &&
        deviceCanResumeRun({
          deviceId: row.deviceId,
          deviceLabel: row.label,
          caps: row.caps,
        })
      )
    }

    // Staleness guard (EXP-153): heartbeat-dead rows render as absent
    // (not "ended" — swept rows leave no recap entry either).
    const running = sessions
      .filter(
        (session) =>
          (session.status === `running` || session.status === `in_review`) &&
          !isCodingSessionStale(session.updatedAt, now)
      )
      .sort(
        (a, b) =>
          new Date(b.startedAt).getTime() - new Date(a.startedAt).getTime()
      )
      .map(toRow)

    // EXP-637: the caller's own runs that the AGENT closed out — the only
    // ended rows with a summary and an outcome to show. Rows killed, exited
    // or swept carry nothing worth a "Recent runs" entry, so they stay out.
    // Newest-ended first, capped at ten.
    const recent = sessions
      .filter(
        (session) =>
          session.status === `ended` && session.endedBy === `agent`
      )
      .sort(
        (a, b) =>
          new Date(b.endedAt ?? b.startedAt).getTime() -
          new Date(a.endedAt ?? a.startedAt).getTime()
      )
      .slice(0, 10)
      .map(toRow)

    return {
      running,
      recent,
      // Without a team id or a signed-in user the query is skipped and can
      // never deliver a snapshot — treat that as ready-empty instead of
      // loading forever.
      isLoading: !isReady && Boolean(teamId && currentUserId),
    }
  }, [
    sessions,
    issueRows,
    openPrIssueRows,
    boards,
    userMap,
    devices,
    isReady,
    teamId,
    currentUserId,
    now,
  ])
}
