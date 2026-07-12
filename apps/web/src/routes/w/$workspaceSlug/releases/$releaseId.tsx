import { useEffect, useMemo, useRef, useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  CalendarDays,
  ChevronRight,
  GitMerge,
  GitPullRequest,
  ListTodo,
  MoreHorizontal,
  Plus,
  Rocket,
  Trash2,
  X,
} from "lucide-react"
import type { Issue, IssueLabel, Label, Release } from "@/db/schema"
import {
  issueCollection,
  issueLabelCollection,
  labelCollection,
  releaseCollection,
} from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { formatDateForMutation } from "@/lib/domain"
import { formatDate, parseLocalDate } from "@/lib/utils"
import { releaseProgress } from "@/lib/releases"
import {
  buildIssueLabelMap,
  buildVisibleIssueGroups,
} from "@/lib/project-board"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import { EmptyState } from "@/components/empty-state"
import { IssueList } from "@/components/issue-list"
import {
  ReleaseIssuePicker,
  releaseCandidateIssues,
} from "@/components/release-issue-picker"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Input } from "@/components/ui/input"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { Progress } from "@/components/ui/progress"
import { Textarea } from "@/components/ui/textarea"

// Release detail (EXP-56): inline-editable header (name/description save on
// blur, like the issue detail), target-date picker, ship/unship, the release
// PR pill, and the release's issues grouped by status via the shared
// project-board machinery. Issue rows navigate to their project detail route;
// the trailing X unbundles a row (setIssueRelease null — issues survive).
export const Route = createFileRoute(`/w/$workspaceSlug/releases/$releaseId`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: ReleaseDetailPage,
})

function ReleasePrPill({ release }: { release: Release }) {
  if (!release.prUrl) return null
  const merged = release.prState === `merged`
  return (
    <Button variant="outline" size="xs" asChild className="shrink-0">
      <a href={release.prUrl} target="_blank" rel="noreferrer">
        {merged ? (
          <GitMerge className="size-3.5 text-purple-400" />
        ) : (
          <GitPullRequest className="size-3.5 text-emerald-500" />
        )}
        <span className="font-mono">#{release.prNumber}</span>
        {release.prState && (
          <span className="text-muted-foreground">{release.prState}</span>
        )}
      </a>
    </Button>
  )
}

// "Add issues" picker: the shared ReleaseIssuePicker (EXP-62) over the
// workspace's issues that are not already in THIS release. Multi-select, one
// bulk addIssues call.
function AddIssuesDialog({
  open,
  onOpenChange,
  release,
  projectIds,
  releaseNameById,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  release: Release
  projectIds: string[]
  releaseNameById: Map<string, string>
}) {
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set())
  const [submitting, setSubmitting] = useState(false)

  const { data: issueRows } = useLiveQuery(
    (query) =>
      open && projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => inArray(issues.projectId, projectIds))
        : undefined,
    [open, projectIds.join(`,`)]
  )

  const candidates = useMemo(
    () => releaseCandidateIssues((issueRows ?? []) as Issue[], release.id),
    [issueRows, release.id]
  )

  const handleOpenChange = (next: boolean) => {
    onOpenChange(next)
    if (!next) setSelectedIds(new Set())
  }

  const toggle = (issueId: string) => {
    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(issueId)) {
        next.delete(issueId)
      } else {
        next.add(issueId)
      }
      return next
    })
  }

  const handleAdd = async () => {
    if (selectedIds.size === 0 || submitting) return
    setSubmitting(true)
    try {
      const { txId } = await trpc.releases.addIssues.mutate({
        releaseId: release.id,
        issueIds: [...selectedIds],
      })
      await issueCollection.utils.awaitTxId(txId)
      handleOpenChange(false)
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent className="flex max-h-[70vh] flex-col gap-0 overflow-hidden p-0 sm:max-w-lg">
        <DialogHeader className="border-b border-border/50 px-4 py-3">
          <DialogTitle className="text-sm">
            Add issues to {release.name}
          </DialogTitle>
        </DialogHeader>
        <ReleaseIssuePicker
          candidates={candidates}
          selectedIds={selectedIds}
          onToggle={toggle}
          releaseNameById={releaseNameById}
        />
        <DialogFooter className="border-t border-border/50 px-4 py-3">
          <Button
            variant="outline"
            size="sm"
            onClick={() => handleOpenChange(false)}
            disabled={submitting}
          >
            Cancel
          </Button>
          <Button
            size="sm"
            onClick={() => void handleAdd()}
            disabled={selectedIds.size === 0 || submitting}
          >
            {submitting
              ? `Adding…`
              : selectedIds.size === 1
                ? `Add 1 issue`
                : `Add ${selectedIds.size} issues`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function ReleaseDetailPage() {
  const { workspaceSlug, releaseId } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)
  const permissions = useWorkspacePermissions(workspace)
  const projectIds = useMemo(
    () => projects.map((project) => project.id),
    [projects]
  )
  const projectMap = useMemo(
    () => new Map(projects.map((project) => [project.id, project])),
    [projects]
  )

  const { data: releaseRows, isReady: releaseReady } = useLiveQuery(
    (query) =>
      query
        .from({ releases: releaseCollection })
        .where(({ releases }) => eq(releases.id, releaseId)),
    [releaseId]
  )
  const release = (releaseRows?.[0] ?? null) as Release | null

  // Every workspace release, for the add-dialog's "lives in another release"
  // badges (tiny list, already synced).
  const { data: workspaceReleaseRows } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ releases: releaseCollection })
            .where(({ releases }) => eq(releases.workspaceId, workspace.id))
        : undefined,
    [workspace?.id]
  )
  const releaseNameById = useMemo(
    () =>
      new Map(
        ((workspaceReleaseRows ?? []) as Release[]).map((row) => [
          row.id,
          row.name,
        ])
      ),
    [workspaceReleaseRows]
  )

  const { data: issueRows, isReady: issuesReady } = useLiveQuery(
    (query) =>
      query
        .from({ issues: issueCollection })
        .where(({ issues }) => eq(issues.releaseId, releaseId)),
    [releaseId]
  )
  const issues = (issueRows ?? []) as Issue[]

  const { data: labelRows } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ labels: labelCollection })
            .where(({ labels }) => eq(labels.workspaceId, workspace.id))
        : undefined,
    [workspace?.id]
  )
  const { data: issueLabelRows } = useLiveQuery(
    (query) =>
      workspace
        ? query
            .from({ issueLabels: issueLabelCollection })
            .where(({ issueLabels }) =>
              eq(issueLabels.workspaceId, workspace.id)
            )
        : undefined,
    [workspace?.id]
  )
  const labelList = (labelRows ?? []) as Label[]
  const issueLabelMap = useMemo(
    () =>
      buildIssueLabelMap((issueLabelRows ?? []) as IssueLabel[], labelList),
    [issueLabelRows, labelList]
  )
  const { userMap, users } = useWorkspaceUsers(workspace?.id)

  const groups = useMemo(() => buildVisibleIssueGroups(issues, []), [issues])
  const progress = releaseProgress(issues)

  // Inline name/description editing: local state, save on blur, resync from
  // Electric only when the incoming value differs from what we last saved
  // (mirrors the issue detail's title handling).
  const [name, setName] = useState(release?.name ?? ``)
  const [description, setDescription] = useState(release?.description ?? ``)
  const [addOpen, setAddOpen] = useState(false)
  const [deleteOpen, setDeleteOpen] = useState(false)
  const [deleting, setDeleting] = useState(false)

  // Last synced values, so each field resyncs ONLY when ITS OWN synced value
  // changed — a name echo (or a teammate's rename) must never clobber an
  // in-progress description draft, and vice versa.
  const lastSyncedName = useRef(release?.name ?? ``)
  const lastSyncedDescription = useRef(release?.description ?? ``)

  useEffect(() => {
    setName(release?.name ?? ``)
    setDescription(release?.description ?? ``)
    lastSyncedName.current = release?.name ?? ``
    lastSyncedDescription.current = release?.description ?? ``
  }, [release?.id])

  useEffect(() => {
    if (!release) return
    if (release.name !== lastSyncedName.current) {
      lastSyncedName.current = release.name
      if (release.name !== name && release.name !== name.trim()) {
        setName(release.name)
      }
    }
    const incoming = release.description ?? ``
    if (incoming !== lastSyncedDescription.current) {
      lastSyncedDescription.current = incoming
      if (incoming !== description.trim()) {
        setDescription(incoming)
      }
    }
  }, [release?.name, release?.description])

  const updateRelease = async (patch: {
    name?: string
    description?: string | null
    targetDate?: string | null
  }) => {
    if (!release) return
    const { txId } = await trpc.releases.update.mutate({
      id: release.id,
      ...patch,
    })
    await releaseCollection.utils.awaitTxId(txId)
  }

  const handleNameBlur = async () => {
    if (!release) return
    const trimmed = name.trim()
    if (trimmed && trimmed !== release.name) {
      await updateRelease({ name: trimmed })
    }
  }

  const handleDescriptionBlur = async () => {
    if (!release) return
    const trimmed = description.trim()
    if (trimmed !== (release.description ?? ``)) {
      await updateRelease({ description: trimmed ? trimmed : null })
    }
  }

  const handleTargetDateSelect = async (date: Date | undefined) => {
    await updateRelease({ targetDate: formatDateForMutation(date) })
  }

  const handleToggleShipped = async () => {
    if (!release) return
    const { txId } = await trpc.releases.markShipped.mutate({
      id: release.id,
      shipped: release.shippedAt === null,
    })
    await releaseCollection.utils.awaitTxId(txId)
  }

  const handleDelete = async () => {
    if (!release || deleting) return
    setDeleting(true)
    try {
      const { txId } = await trpc.releases.delete.mutate({ id: release.id })
      await releaseCollection.utils.awaitTxId(txId)
      setDeleteOpen(false)
      void navigate({
        to: `/w/$workspaceSlug/releases`,
        params: { workspaceSlug },
      })
    } finally {
      setDeleting(false)
    }
  }

  const handleRemoveIssue = async (issue: Issue) => {
    const { txId } = await trpc.releases.setIssueRelease.mutate({
      issueId: issue.id,
      releaseId: null,
    })
    await issueCollection.utils.awaitTxId(txId)
  }

  if (!workspace || (!release && !releaseReady)) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  if (!release || release.workspaceId !== workspace.id) {
    return (
      <EmptyState
        icon={Rocket}
        title="Release not found"
        description="This release may have been deleted."
      >
        <Button size="sm" variant="outline" asChild>
          <Link to="/w/$workspaceSlug/releases" params={{ workspaceSlug }}>
            Back to releases
          </Link>
        </Button>
      </EmptyState>
    )
  }

  const isShipped = release.shippedAt !== null
  const canMutate = permissions.isMember
  const targetDate = release.targetDate
    ? parseLocalDate(release.targetDate)
    : undefined

  return (
    <div className="flex h-full flex-col min-h-0">
      {/* Breadcrumb + header actions */}
      <div className="flex items-center gap-1.5 border-b border-border px-4 py-2 text-xs text-muted-foreground min-w-0">
        <Link
          to="/w/$workspaceSlug/releases"
          params={{ workspaceSlug }}
          className="inline-flex shrink-0 items-center gap-1.5 hover:text-foreground"
        >
          <Rocket className="size-3" />
          Releases
        </Link>
        <ChevronRight className="size-3 shrink-0 text-muted-foreground/50" />
        <span className="truncate text-foreground">{release.name}</span>
        <div className="ml-auto flex shrink-0 items-center gap-2">
          <ReleasePrPill release={release} />
          {canMutate && (
            <Button
              variant={isShipped ? `outline` : `default`}
              size="xs"
              onClick={() => void handleToggleShipped()}
            >
              {isShipped ? `Unship` : `Mark shipped`}
            </Button>
          )}
          {canMutate && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon-xs"
                  className="text-muted-foreground"
                  aria-label="Release actions"
                >
                  <MoreHorizontal className="size-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  variant="destructive"
                  onSelect={() => setDeleteOpen(true)}
                >
                  <Trash2 className="size-4" />
                  Delete release
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-3xl">
          <Input
            value={name}
            onBlur={() => void handleNameBlur()}
            onChange={(e) => setName(e.target.value)}
            placeholder="Release name"
            disabled={!canMutate}
            className="bg-transparent dark:bg-transparent border-none shadow-none text-2xl font-semibold px-5 pt-4 pb-1 focus-visible:ring-0 placeholder:text-muted-foreground/50"
          />
          <Textarea
            value={description}
            onBlur={() => void handleDescriptionBlur()}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Add description..."
            disabled={!canMutate}
            className="min-h-12 resize-none border-none bg-transparent px-5 py-1 shadow-none focus-visible:ring-0 dark:bg-transparent placeholder:text-muted-foreground/50"
          />

          {/* Meta row: target date, shipped state, progress */}
          <div className="flex flex-wrap items-center gap-3 px-5 py-3">
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  variant="ghost"
                  size="xs"
                  className="text-muted-foreground"
                  disabled={!canMutate}
                >
                  <CalendarDays className="size-3" />
                  {release.targetDate
                    ? formatDate(release.targetDate)
                    : `Target date`}
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-auto p-0" align="start">
                <Calendar
                  mode="single"
                  selected={targetDate}
                  onSelect={(date) => void handleTargetDateSelect(date)}
                />
              </PopoverContent>
            </Popover>

            {isShipped ? (
              <Badge
                variant="outline"
                className="border-emerald-500/40 bg-emerald-500/10 text-emerald-500"
              >
                Shipped {formatDate(release.shippedAt!)}
              </Badge>
            ) : progress.isComplete ? (
              <Badge
                variant="outline"
                className="border-emerald-500/40 text-emerald-500"
              >
                Ready
              </Badge>
            ) : null}

            <div className="ml-auto flex items-center gap-2">
              <span className="text-xs text-muted-foreground">
                {progress.total === 0
                  ? `No issues`
                  : `${progress.done} of ${progress.denominator} done`}
              </span>
              <Progress
                value={progress.fraction * 100}
                className="h-1.5 w-32"
              />
            </div>
          </div>

          {/* Issues section — the header button yields to the empty state's
              own (exactly one add affordance visible at any time) */}
          <div className="flex items-center justify-between border-b border-border px-5 py-2">
            <span className="text-sm font-medium">Issues</span>
            {canMutate && issues.length > 0 && (
              <Button
                variant="outline"
                size="xs"
                onClick={() => setAddOpen(true)}
              >
                <Plus className="size-3.5" />
                Add issues
              </Button>
            )}
          </div>

          {issuesReady && issues.length === 0 ? (
            <EmptyState
              icon={ListTodo}
              title="No issues in this release"
              description="Add issues to bundle them into this release."
            >
              {canMutate && (
                <Button size="sm" onClick={() => setAddOpen(true)}>
                  <Plus className="mr-1.5 size-4" />
                  Add issues
                </Button>
              )}
            </EmptyState>
          ) : (
            <IssueList
              groups={groups}
              issueLabelMap={issueLabelMap}
              labels={labelList}
              users={users}
              userMap={userMap}
              onNewIssue={() => {}}
              onIssueClick={(issue) => {
                const project = projectMap.get(issue.projectId)
                if (!project) return
                void navigate({
                  to: `/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
                  params: {
                    workspaceSlug,
                    projectSlug: project.slug,
                    issueIdentifier: issue.identifier,
                  },
                })
              }}
              canCreate={false}
              canMutateIssue={permissions.canMutateIssue}
              canModerate={permissions.isModerator}
              bulkWorkspaceId={workspace.id}
              isLoading={!issuesReady}
              hasAnyIssues={issues.length > 0}
              renderRowAction={
                canMutate
                  ? (issue) => (
                      <Button
                        variant="ghost"
                        size="icon-xs"
                        className="text-muted-foreground md:opacity-0 md:group-hover/row:opacity-100"
                        aria-label="Remove from release"
                        title="Remove from release"
                        onClick={() => void handleRemoveIssue(issue)}
                      >
                        <X className="size-3.5" />
                      </Button>
                    )
                  : undefined
              }
            />
          )}
        </div>
      </div>

      <AddIssuesDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        release={release}
        projectIds={projectIds}
        releaseNameById={releaseNameById}
      />

      {/* Delete confirm (repo convention: Dialog confirm, like project trash) */}
      <Dialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Delete release</DialogTitle>
            <DialogDescription>
              Delete{` `}
              <span className="font-semibold text-foreground">
                {release.name}
              </span>
              ? Its issues are kept — they just leave the release. This cannot
              be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setDeleteOpen(false)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => void handleDelete()}
              disabled={deleting}
            >
              {deleting ? `Deleting…` : `Delete release`}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
