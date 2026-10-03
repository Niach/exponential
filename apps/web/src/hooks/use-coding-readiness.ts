// EXP-1121: the INPUTS of the "Ready to code?" checklist, gathered from what
// the web already has — membership + boards + devices off Electric, the
// relay switch, the team's repositories and GitHub connection over tRPC
// (`repositories` is server-only, never a shape) — and handed to the pure,
// fixture-locked `codingReadiness` (`lib/coding-readiness.ts`). Every Start
// coding surface (issue capsule, phone circle, bulk bar, Agent composer,
// Getting started) reads this ONE hook, so they can never disagree about
// what is missing.
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Board, Device, Issue, User } from "@/db/schema"
import {
  deviceCollection,
  issueCollection,
  teamMemberCollection,
  userCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import {
  codingReadiness,
  type CodingReadiness,
  type ReadinessDevice,
} from "@/lib/coding-readiness"
import {
  composeDeviceList,
  deviceIsMine,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { useNow } from "@/hooks/use-now"
import { useTeamBoards, useTeamById } from "@/hooks/use-team-data"
import { useSteerConfig } from "@/components/agent-session"

export type ReadinessRepoList = Awaited<
  ReturnType<typeof trpc.repositories.list.query>
>
type GithubStatus = Awaited<
  ReturnType<typeof trpc.integrations.github.status.query>
>

// Membership gate shared by every coding surface (the server enforces it
// regardless; this only decides what renders). Re-exported by
// `issue-coding-rows.tsx` for its existing importers.
export function useIsTeamMember(teamId: string, currentUserId: string) {
  return useTeamMembership(teamId, currentUserId).isMember
}

function useTeamMembership(teamId: string, currentUserId: string) {
  const { data: memberRows } = useLiveQuery(
    (query) =>
      query
        .from({ m: teamMemberCollection })
        .where(({ m }) =>
          and(eq(m.teamId, teamId), eq(m.userId, currentUserId))
        ),
    [teamId, currentUserId]
  )
  const row = (memberRows?.[0] ?? null) as { role?: string } | null
  return { isMember: row !== null, isOwner: row?.role === `owner` }
}

/** The board a selection's Start coding is judged by: the first one that
 * HAS a repository (a mixed selection starts its repo-backed issues, the
 * bulk bar's rule since EXP-642), else the first one at all. */
export function readinessBoard(
  boards: Board[],
  boardIds: readonly string[]
): Board | null {
  const byId = new Map(boards.map((board) => [board.id, board]))
  const picked = boardIds
    .map((id) => byId.get(id))
    .filter((board): board is Board => Boolean(board))
  return picked.find((board) => board.repositoryId) ?? picked[0] ?? null
}

/** `octocat`: the viewer's linked GitHub login, else the first installed
 * account, else the owner half of the first team repository. */
function githubLabel(
  status: GithubStatus | null,
  repos: ReadinessRepoList
): string | null {
  if (status?.login) return status.login
  const login = status?.installations.find((inst) => inst.accountLogin)
    ?.accountLogin
  if (login) return login
  const owner = repos[0]?.fullName.split(`/`)[0]
  return owner || null
}

export interface CodingReadinessState {
  readiness: CodingReadiness
  /** The board the checklist is about (null while unresolved). */
  board: Board | null
  teamId: string
  /** Owner-only fixes (Board settings) hide from everyone else. */
  isOwner: boolean
  /** The team's repositories (null until the first answer) — the inline
   * repository picker lists them. */
  repos: ReadinessRepoList | null
  /** The viewer has GitHub linked (SLOP-7); null while unknown. The
   * "Connect GitHub" fix opens the guided page either way. */
  githubLinked: boolean | null
  /** The caller's OWN machines (the add-device dialog watches them for the
   * new one); null while loading. */
  ownDevices: SteerDevice[] | null
  /** Re-list repositories + GitHub state (after an add). */
  reload: () => void
}

export function useCodingReadiness({
  teamId,
  boardIds,
  currentUserId,
  enabled = true,
}: {
  teamId: string
  /** The issue's board, or every selected issue's board. */
  boardIds: readonly string[]
  currentUserId: string
  /** The host's own gate (e.g. only while an issue subject is picked). */
  enabled?: boolean
}): CodingReadinessState {
  const { isMember, isOwner } = useTeamMembership(teamId, currentUserId)
  const config = useSteerConfig()
  const remoteStartEnabled = config ? config.enabled : null
  // Nothing is fetched for a surface that renders nothing (non-member, a
  // relay-less instance): `visible` is false there before any of it lands.
  const active = enabled && isMember && remoteStartEnabled !== false

  const team = useTeamById(active ? teamId : null)
  const boards = useTeamBoards(active ? teamId : undefined)
  const boardKey = boardIds.join(`,`)
  const board = useMemo(
    () => readinessBoard(boards, boardIds),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [boards, boardKey]
  )
  const needsGithub = active && board !== null && !board.repositoryId

  // ── repositories + GitHub (tRPC) ──────────────────────────────────────────
  const [repos, setRepos] = useState<ReadinessRepoList | null>(null)
  const [reposFailed, setReposFailed] = useState(false)
  const [github, setGithub] = useState<GithubStatus | null>(null)
  const [githubFailed, setGithubFailed] = useState(false)
  const [tick, setTick] = useState(0)
  const reload = useCallback(() => setTick((value) => value + 1), [])

  // A team change drops the previous team's answers (and failure flags) in
  // the same render, so a mounted surface never judges the new team by them.
  const [stateTeamId, setStateTeamId] = useState(teamId)
  if (stateTeamId !== teamId) {
    setStateTeamId(teamId)
    setRepos(null)
    setReposFailed(false)
    setGithub(null)
    setGithubFailed(false)
  }

  // A late answer for a previous team must not land on the current one.
  const teamRef = useRef(teamId)
  teamRef.current = teamId

  useEffect(() => {
    if (!active) return
    const forTeam = teamId
    let live = true
    trpc.repositories.list
      .query({ teamId: forTeam })
      .then((rows) => {
        if (!live || teamRef.current !== forTeam) return
        setRepos(rows)
        setReposFailed(false)
      })
      .catch(() => {
        if (!live || teamRef.current !== forTeam) return
        setReposFailed(true)
      })
    return () => {
      live = false
    }
  }, [active, teamId, tick])

  useEffect(() => {
    if (!needsGithub) return
    const forTeam = teamId
    let live = true
    trpc.integrations.github.status
      .query({ teamId: forTeam })
      .then((status) => {
        if (!live || teamRef.current !== forTeam) return
        setGithub(status)
        setGithubFailed(false)
      })
      .catch(() => {
        if (!live || teamRef.current !== forTeam) return
        setGithubFailed(true)
      })
    return () => {
      live = false
    }
  }, [needsGithub, teamId, tick])

  // Back from the GitHub popup or a settings tab: re-probe, the rows tick.
  useEffect(() => {
    if (!active) return
    const onFocus = () => reload()
    window.addEventListener(`focus`, onFocus)
    return () => window.removeEventListener(`focus`, onFocus)
  }, [active, reload])

  // A board pointed at a repository this copy of the list has never seen
  // (added a moment ago from the picker or settings): re-list once per id.
  const relistedFor = useRef<string | null>(null)
  const repositoryId = board?.repositoryId ?? null
  useEffect(() => {
    if (!repositoryId || !repos) return
    if (repos.some((repo) => repo.id === repositoryId)) return
    if (relistedFor.current === repositoryId) return
    relistedFor.current = repositoryId
    reload()
  }, [repositoryId, repos, reload])

  // ── devices (Electric) ────────────────────────────────────────────────────
  const { data: deviceRows } = useLiveQuery(
    (query) => (active ? query.from({ d: deviceCollection }) : undefined),
    [active]
  )
  const { data: userRows } = useLiveQuery(
    (query) => (active ? query.from({ u: userCollection }) : undefined),
    [active]
  )
  // 30s against the contract's online window, like `useRemoteStart`.
  const now = useNow(30_000)
  const composed = useMemo<SteerDevice[] | null>(() => {
    if (!active || deviceRows === undefined) return null
    const usersById = new Map(
      ((userRows ?? []) as User[]).map((user) => [user.id, user])
    )
    return composeDeviceList(
      deviceRows as Device[],
      usersById,
      now,
      currentUserId,
      teamId
    )
  }, [active, deviceRows, userRows, now, currentUserId, teamId])
  const devices = useMemo<ReadinessDevice[] | null>(
    () =>
      composed?.map((device) => ({
        label: device.deviceLabel || device.deviceId,
        own: deviceIsMine(device),
        online: deviceIsOnline(device),
        lastSeenAtMs: device.lastSeenAt
          ? new Date(device.lastSeenAt).getTime()
          : null,
      })) ?? null,
    [composed]
  )
  const ownDevices = useMemo(
    () => composed?.filter(deviceIsMine) ?? null,
    [composed]
  )

  // ── the model ─────────────────────────────────────────────────────────────
  const boardRepository = !board?.repositoryId
    ? null
    : (repos?.find((repo) => repo.id === board.repositoryId)?.fullName ?? ``)
  // Asked only while the board has none; an unresolved board keeps the
  // whole checklist loading (null repository + null GitHub).
  // SLOP-7: "GitHub connected" = the viewer's own GitHub account is linked
  // with a live token, or the team already has repositories (a teammate
  // connected them — this person needs no GitHub of their own to pick one).
  const githubInput = useMemo(() => {
    if (!board || board.repositoryId) return null
    const reposKnown = repos !== null || reposFailed
    const statusKnown = github !== null || githubFailed
    if (!reposKnown || !statusKnown) return null
    const teamRepos = repos ?? []
    const linked =
      github?.linked === true && github.needsReconnect !== true
    return {
      connected: teamRepos.length > 0 || linked,
      label: githubLabel(github, teamRepos),
    }
  }, [board, repos, reposFailed, github, githubFailed])

  const readiness = useMemo(
    () =>
      codingReadiness({
        isMember,
        remoteStartEnabled,
        teamName: team?.name ?? ``,
        boardName: board?.name ?? ``,
        boardRepository,
        github: githubInput,
        devices,
        nowMs: now.getTime(),
      }),
    [
      isMember,
      remoteStartEnabled,
      team?.name,
      board?.name,
      boardRepository,
      githubInput,
      devices,
      now,
    ]
  )

  return {
    readiness,
    board,
    teamId,
    isOwner,
    repos,
    githubLinked: github ? github.linked && !github.needsReconnect : null,
    ownDevices,
    reload,
  }
}

/** The readiness of starting a set of ISSUES (the Agent composer's chips):
 * resolves each id's board off the synced issues — `known` rows first (the
 * composer's own pool), a live lookup for the rest (a seeded issue on a
 * repo-less board is never in that pool). Inert with no ids. */
export function useIssuesCodingReadiness({
  teamId,
  issueIds,
  known,
  currentUserId,
}: {
  teamId: string
  issueIds: readonly string[]
  known: readonly Pick<Issue, `id` | `boardId`>[]
  currentUserId: string
}): CodingReadinessState {
  const knownBoard = useMemo(
    () => new Map(known.map((issue) => [issue.id, issue.boardId])),
    [known]
  )
  const missing = useMemo(
    () => issueIds.filter((id) => !knownBoard.has(id)).sort(),
    [issueIds, knownBoard]
  )
  const { data: missingRows } = useLiveQuery(
    (query) =>
      missing.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, missing))
        : undefined,
    [missing.join(`,`)]
  )
  const boardIds = useMemo(() => {
    const byId = new Map(knownBoard)
    for (const row of (missingRows ?? []) as Issue[]) {
      byId.set(row.id, row.boardId)
    }
    const ids = issueIds
      .map((id) => byId.get(id))
      .filter((id): id is string => Boolean(id))
    return [...new Set(ids)]
  }, [issueIds, knownBoard, missingRows])
  return useCodingReadiness({
    teamId,
    boardIds,
    currentUserId,
    enabled: issueIds.length > 0 && Boolean(currentUserId),
  })
}
