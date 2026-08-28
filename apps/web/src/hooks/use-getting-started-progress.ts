import { createContext, useContext, useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  actionCollection,
  codingSessionCollection,
  deviceCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useSession } from "@/hooks/use-session"
import {
  useTeamBoards,
  useTeamInvites,
  useTeamUsers,
} from "@/hooks/use-team-data"
import {
  useTeamPermissions,
  type TeamPermissions,
} from "@/hooks/use-team-permissions"
import {
  deriveEntryStates,
  isGettingStartedComplete,
  type GettingStartedEntry,
  type GettingStartedSignals,
} from "@/components/getting-started/getting-started-model"
import type { Device, Team } from "@/db/schema"

export interface GettingStartedProgress {
  /** Some signal source has not answered yet — render neutral / stay hidden. */
  loading: boolean
  signals: GettingStartedSignals
  permissions: TeamPermissions
  entries: GettingStartedEntry[]
  done: number
  total: number
  /**
   * EXP-548: every visible entry is done (never true while loading). The
   * sidebar entry and the empty-board block hide on this — there is no
   * dismissal any more.
   */
  complete: boolean
}

// One instance per team layout (`GettingStartedProgressProvider` in
// `t/$teamSlug/route.tsx`): the sidebar entry, its sheet and the empty-board
// block all read the same answer, so the one-shots fire once per team, not
// once per consumer.
const GettingStartedProgressContext =
  createContext<GettingStartedProgress | null>(null)

export const GettingStartedProgressProvider =
  GettingStartedProgressContext.Provider

export function useGettingStartedProgressContext(): GettingStartedProgress {
  const value = useContext(GettingStartedProgressContext)
  if (!value) {
    throw new Error(
      `useGettingStartedProgressContext must be used inside GettingStartedProgressProvider`
    )
  }
  return value
}

// Signal gathering for the getting-started checklist (EXP-141). Live signals
// come from Electric (boards, coding sessions, actions); the rest are one-shot
// tRPC queries fired on mount (httpBatchLink — imperative .query(), the
// repositories-section convention). Deliberately NEVER calls
// repositories.list here: that procedure heals default branches against
// GitHub per call — far too heavy for a checklist.
export function useGettingStartedProgress(
  team: Team | null | undefined
): GettingStartedProgress {
  const { data: session } = useSession()
  const { members } = useTeamUsers(team?.id)
  const invites = useTeamInvites(team?.id)
  const permissions = useTeamPermissions(team)
  const teamId = team?.id

  // Same contract as useSettingsPage (settings/-shared.tsx): permissions are
  // transiently all-false until the user's own member row has synced, so
  // nothing permission-gated may fire before `resolved`.
  const currentUserId = session?.user?.id
  const resolved = Boolean(
    team &&
      currentUserId &&
      members.some((member) => member.userId === currentUserId)
  )

  const boards = useTeamBoards(teamId)
  const liveBoards = useMemo(
    () => boards.filter((board) => !board.deletedAt),
    [boards]
  )

  // Lean coding-sessions existence query (the useAgentsData pattern without
  // its issue/board/user joins).
  const { data: sessionRows, isReady: sessionsReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) => eq(sessions.teamId, teamId))
        : undefined,
    [teamId]
  )

  // EXP-548: any synced action row in the team (the two builtins are
  // constructed client-side, never rows — so this is exactly "an action was
  // authored").
  const { data: actionRows, isReady: actionsReady } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ actions: actionCollection })
            .where(({ actions }) => eq(actions.teamId, teamId))
        : undefined,
    [teamId]
  )

  // One-shot answers; null = not asked / not answered yet. Failures resolve
  // to false — these drive a checklist hint, not access control.
  const [githubInstalled, setGithubInstalled] = useState<boolean | null>(null)
  const [hasWidget, setHasWidget] = useState<boolean | null>(null)
  const [mcpConnected, setMcpConnected] = useState<boolean | null>(null)

  // Team-scoped answers must not leak across a team switch — the
  // sidebar keeps this hook mounted, and a stale `true` would flash the new
  // team's steps as done.
  useEffect(() => {
    setGithubInstalled(null)
    setHasWidget(null)
  }, [teamId])

  const isMember = permissions.isMember
  useEffect(() => {
    if (!resolved || !isMember || !teamId) return
    let cancelled = false
    const check = () => {
      trpc.integrations.github.status
        .query({ teamId })
        .then((status) => {
          if (cancelled) return
          setGithubInstalled(status.installed)
          // The listener only exists to catch not-installed → installed
          // (returning from the GitHub install tab); once installed there is
          // nothing left to detect, so stop re-querying on every focus.
          if (status.installed) window.removeEventListener(`focus`, check)
        })
        .catch(() => {
          if (!cancelled) setGithubInstalled(false)
        })
    }
    check()
    // Re-detect when the user returns from the GitHub install/connect tab —
    // same window-focus convention as the repositories settings section.
    window.addEventListener(`focus`, check)
    return () => {
      cancelled = true
      window.removeEventListener(`focus`, check)
    }
  }, [resolved, isMember, teamId])

  // widgets.list is owner-only on the server — never fire it for members.
  const canManageWidgets = permissions.canManageWidgets
  useEffect(() => {
    if (!resolved || !canManageWidgets || !teamId) return
    let cancelled = false
    trpc.widgets.list
      .query({ teamId })
      .then((rows) => {
        if (!cancelled) setHasWidget(rows.length > 0)
      })
      .catch(() => {
        if (!cancelled) setHasWidget(false)
      })
    return () => {
      cancelled = true
    }
  }, [resolved, canManageWidgets, teamId])

  // User-level, team-independent. The synced devices shape already carries
  // the caller's own rows (EXP-481) — "installed the app / ran the server
  // one-liner in another window" arrives as a live insert, so the old
  // one-shot tRPC fetch + focus listener is gone (EXP-485).
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const deviceKinds = useMemo(() => {
    if (deviceRows === undefined || !currentUserId) return null
    const own = (deviceRows as Device[]).filter(
      (device) => device.userId === currentUserId
    )
    return {
      desktop: own.some((device) => device.kind === `desktop`),
      server: own.some((device) => device.kind === `server`),
    }
  }, [deviceRows, currentUserId])

  // User-level, team-independent — fire immediately.
  useEffect(() => {
    let cancelled = false
    void Promise.allSettled([
      trpc.mcpGrants.hasAny.query(),
      trpc.users.listPersonalApiKeys.query(),
    ]).then(([grants, apiKeys]) => {
      if (cancelled) return
      const hasGrant = grants.status === `fulfilled` && grants.value.hasAny
      const hasKey =
        apiKeys.status === `fulfilled` && apiKeys.value.keys.length > 0
      setMcpConnected(hasGrant || hasKey)
    })
    return () => {
      cancelled = true
    }
  }, [])

  const signals: GettingStartedSignals = useMemo(
    () => ({
      hasDesktopDevice: deviceKinds?.desktop === true,
      hasServerDevice: deviceKinds?.server === true,
      githubInstalled: githubInstalled === true,
      hasInvitedTeam: members.length > 1 || invites.length > 0,
      hasBoard: liveBoards.length > 0,
      hasRepoBoard: liveBoards.some(
        (board) => board.repositoryId != null
      ),
      hasCodingSession: (sessionRows ?? []).length > 0,
      hasAction: (actionRows ?? []).length > 0,
      helpdeskEnabled: team?.helpdeskEnabled === true,
      hasWidget: hasWidget === true,
      mcpConnected: mcpConnected === true,
    }),
    [
      deviceKinds,
      githubInstalled,
      members,
      invites,
      liveBoards,
      sessionRows,
      actionRows,
      team?.helpdeskEnabled,
      hasWidget,
      mcpConnected,
    ]
  )

  // Neutral until every signal source has answered — checks/locks that pop in
  // one by one read as state changes, not loading.
  const loading =
    !resolved ||
    !sessionsReady ||
    !actionsReady ||
    githubInstalled === null ||
    deviceKinds === null ||
    (canManageWidgets && hasWidget === null) ||
    mcpConnected === null

  const { entries, done, total } = useMemo(
    () =>
      deriveEntryStates(signals, {
        canManageWidgets: permissions.canManageWidgets,
        isOwner: permissions.isOwner,
        canManageMembers: permissions.canManageMembers,
      }),
    [
      signals,
      permissions.canManageWidgets,
      permissions.isOwner,
      permissions.canManageMembers,
    ]
  )

  return {
    loading,
    signals,
    permissions,
    entries,
    done,
    total,
    complete: !loading && isGettingStartedComplete({ done, total }),
  }
}
