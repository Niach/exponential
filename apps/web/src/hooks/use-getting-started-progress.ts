import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { useSession } from "@/hooks/use-session"
import {
  useWorkspaceProjects,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import {
  useWorkspacePermissions,
  type WorkspacePermissions,
} from "@/hooks/use-workspace-permissions"
import type { GettingStartedSignals } from "@/components/getting-started/getting-started-model"
import type { Workspace } from "@/db/schema"

// Signal gathering for the getting-started checklist (EXP-141). Live signals
// come from Electric (projects, coding sessions); the rest are one-shot tRPC
// queries fired on mount (httpBatchLink — imperative .query(), the
// repositories-section convention). Deliberately NEVER calls
// repositories.list here: that procedure heals default branches against
// GitHub per call — far too heavy for a checklist.
export function useGettingStartedProgress(
  workspace: Workspace | null | undefined
): {
  loading: boolean
  signals: GettingStartedSignals
  permissions: WorkspacePermissions
} {
  const { data: session } = useSession()
  const { members } = useWorkspaceUsers(workspace?.id)
  const permissions = useWorkspacePermissions(workspace)
  const workspaceId = workspace?.id

  // Same contract as useSettingsPage (settings/-shared.tsx): permissions are
  // transiently all-false until the user's own member row has synced, so
  // nothing permission-gated may fire before `resolved`.
  const currentUserId = session?.user?.id
  const resolved = Boolean(
    workspace &&
      currentUserId &&
      members.some((member) => member.userId === currentUserId)
  )

  const projects = useWorkspaceProjects(workspaceId)
  const liveProjects = useMemo(
    () => projects.filter((project) => !project.archivedAt && !project.deletedAt),
    [projects]
  )

  // Lean coding-sessions existence query (the useAgentsData pattern without
  // its issue/project/user joins).
  const { data: sessionRows, isReady: sessionsReady } = useLiveQuery(
    (query) =>
      workspaceId
        ? query
            .from({ sessions: codingSessionCollection })
            .where(({ sessions }) => eq(sessions.workspaceId, workspaceId))
        : undefined,
    [workspaceId]
  )

  // One-shot answers; null = not asked / not answered yet. Failures resolve
  // to false — these drive a checklist hint, not access control.
  const [githubInstalled, setGithubInstalled] = useState<boolean | null>(null)
  const [hasWidget, setHasWidget] = useState<boolean | null>(null)
  const [mcpConnected, setMcpConnected] = useState<boolean | null>(null)

  // Workspace-scoped answers must not leak across a workspace switch — the
  // sidebar keeps this hook mounted, and a stale `true` would flash the new
  // workspace's steps as done.
  useEffect(() => {
    setGithubInstalled(null)
    setHasWidget(null)
  }, [workspaceId])

  const isMember = permissions.isMember
  useEffect(() => {
    if (!resolved || !isMember || !workspaceId) return
    let cancelled = false
    const check = () => {
      trpc.integrations.github.status
        .query({ workspaceId })
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
  }, [resolved, isMember, workspaceId])

  // widgets.list is owner-only on the server — never fire it for members.
  const canManageWidgets = permissions.canManageWidgets
  useEffect(() => {
    if (!resolved || !canManageWidgets || !workspaceId) return
    let cancelled = false
    trpc.widgets.list
      .query({ workspaceId })
      .then((rows) => {
        if (!cancelled) setHasWidget(rows.length > 0)
      })
      .catch(() => {
        if (!cancelled) setHasWidget(false)
      })
    return () => {
      cancelled = true
    }
  }, [resolved, canManageWidgets, workspaceId])

  // User-level, workspace-independent — fire immediately.
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
      githubInstalled: githubInstalled === true,
      hasProject: liveProjects.length > 0,
      hasRepoProject: liveProjects.some(
        (project) => project.repositoryId != null
      ),
      hasCodingSession: (sessionRows ?? []).length > 0,
      hasPublicProject: liveProjects.some((project) => project.isPublic),
      hasHelpdeskProject: liveProjects.some(
        (project) => project.isPublic && project.helpdeskEnabled
      ),
      hasWidget: hasWidget === true,
      mcpConnected: mcpConnected === true,
    }),
    [githubInstalled, liveProjects, sessionRows, hasWidget, mcpConnected]
  )

  // Neutral until every signal source has answered — checks/locks that pop in
  // one by one read as state changes, not loading.
  const loading =
    !resolved ||
    !sessionsReady ||
    githubInstalled === null ||
    (canManageWidgets && hasWidget === null) ||
    mcpConnected === null

  return { loading, signals, permissions }
}
