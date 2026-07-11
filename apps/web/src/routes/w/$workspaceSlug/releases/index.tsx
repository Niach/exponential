import { useMemo, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { CalendarDays, Plus, Rocket } from "lucide-react"
import type { Issue, Release } from "@/db/schema"
import { issueCollection, releaseCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { compareReleases, releaseProgress } from "@/lib/releases"
import { formatDate } from "@/lib/utils"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"
import { EmptyState } from "@/components/empty-state"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Progress } from "@/components/ui/progress"

// Workspace Releases list (EXP-56): every release in the workspace, unshipped
// first (by target date), then shipped (most recent first) — the shared
// compareReleases contract. Progress is pure client work over the already-
// synced issues shape (issues.release_id), matching the sidebar badge.
export const Route = createFileRoute(`/w/$workspaceSlug/releases/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: ReleasesPage,
})

function ReleaseStatePill({
  release,
  isComplete,
}: {
  release: Release
  isComplete: boolean
}) {
  if (release.shippedAt !== null) {
    return (
      <Badge
        variant="outline"
        className="border-emerald-500/40 bg-emerald-500/10 text-emerald-500"
      >
        Shipped {formatDate(release.shippedAt)}
      </Badge>
    )
  }
  if (isComplete) {
    return (
      <Badge
        variant="outline"
        className="border-emerald-500/40 text-emerald-500"
      >
        Ready
      </Badge>
    )
  }
  return null
}

function ReleaseRow({
  release,
  issues,
  onOpen,
}: {
  release: Release
  issues: Issue[]
  onOpen: () => void
}) {
  const progress = releaseProgress(issues)

  return (
    <div
      className="grid cursor-pointer grid-cols-[1fr_auto] items-center gap-3 border-b border-border/30 px-3 py-2.5 hover:bg-muted/50"
      onClick={onOpen}
      data-testid={`release-row-${release.id}`}
    >
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-medium">{release.name}</span>
          <ReleaseStatePill release={release} isComplete={progress.isComplete} />
        </div>
        <div className="mt-0.5 flex items-center gap-1.5 text-xs text-muted-foreground">
          {release.targetDate && (
            <span className="inline-flex items-center gap-1">
              <CalendarDays className="size-3" />
              {formatDate(release.targetDate)}
              <span aria-hidden>·</span>
            </span>
          )}
          <span>
            {progress.total === 0
              ? `No issues`
              : `${progress.done} of ${progress.denominator} done`}
          </span>
        </div>
      </div>
      <div className="flex w-32 items-center">
        <Progress value={progress.fraction * 100} className="h-1.5" />
      </div>
    </div>
  )
}

function ReleasesPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const projectIds = useMemo(
    () => projects.map((project) => project.id),
    [projects]
  )
  const [creating, setCreating] = useState(false)

  const { data: releaseRows, isReady: releasesReady } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ releases: releaseCollection })
            .where(({ releases }) => eq(releases.workspaceId, workspace.id))
        : undefined,
    [workspace?.id]
  )

  // All issues bundled into ANY release of this workspace, grouped by
  // release_id for the per-row progress. Workspace-scoped via project ids —
  // the issues shape spans every workspace the member can see.
  const { data: issueRows } = useLiveQuery(
    (query) =>
      projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.projectId, projectIds))
        : undefined,
    [projectIds.join(`,`)]
  )

  const releases = useMemo(
    () => [...((releaseRows ?? []) as Release[])].sort(compareReleases),
    [releaseRows]
  )

  const issuesByRelease = useMemo(() => {
    const map = new Map<string, Issue[]>()
    for (const issue of (issueRows ?? []) as Issue[]) {
      if (!issue.releaseId) continue
      const current = map.get(issue.releaseId) ?? []
      current.push(issue)
      map.set(issue.releaseId, current)
    }
    return map
  }, [issueRows])

  const openRelease = (releaseId: string) => {
    void navigate({
      to: `/w/$workspaceSlug/releases/$releaseId`,
      params: { workspaceSlug, releaseId },
    })
  }

  // One-click create: the server auto-names sequentially ("Release N"); the
  // detail page is where naming/dates happen (inline). Await the txId so the
  // navigation lands on a synced row, never a not-found flash.
  const handleCreate = async () => {
    if (!workspace || creating) return
    setCreating(true)
    try {
      const { txId, release } = await trpc.releases.create.mutate({
        workspaceId: workspace.id,
      })
      await releaseCollection.utils.awaitTxId(txId)
      openRelease(release.id)
    } finally {
      setCreating(false)
    }
  }

  if (!workspace) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Rocket className="h-4 w-4" />
          Releases
          {releases.length > 0 && (
            <span className="text-sm font-normal text-muted-foreground">
              · {releases.length}
            </span>
          )}
        </h1>
        <Button
          size="sm"
          onClick={() => void handleCreate()}
          disabled={creating}
        >
          <Plus className="size-4" />
          New release
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {!releasesReady ? (
          <div className="text-muted-foreground p-6 text-sm">Loading…</div>
        ) : releases.length === 0 ? (
          <EmptyState
            icon={Rocket}
            title="No releases yet"
            description="Bundle issues into a release to track what ships together and when."
          >
            <Button
              size="sm"
              onClick={() => void handleCreate()}
              disabled={creating}
            >
              <Plus className="mr-1.5 size-4" />
              New release
            </Button>
          </EmptyState>
        ) : (
          <div className="rounded-md border border-border/50">
            {releases.map((release) => (
              <ReleaseRow
                key={release.id}
                release={release}
                issues={issuesByRelease.get(release.id) ?? []}
                onOpen={() => openRelease(release.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
