import { useCallback, useEffect, useMemo, useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  ExternalLink,
  GitBranch,
  GitMerge,
  GitPullRequest,
  Loader2,
  RotateCw,
  X,
} from "lucide-react"
import type { Issue } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import {
  useWorkspaceBySlug,
  useWorkspaceProjects,
} from "@/hooks/use-workspace-data"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DiffView,
  FileDiffList,
  type PullFile,
} from "@/components/diff-view"
import { PrStateBadge } from "@/components/issue-coding-rows"

// Review-detail (EXP-106): the PR/branch diff for one review, with Merge/Close
// actions moved off the issue detail. The representative issue carries the PR;
// merging/closing acts on the ONE PR, and the server completes every linked
// issue (a batch run's issues all share one prUrl).
export const Route = createFileRoute(
  `/t/$workspaceSlug/reviews/$issueIdentifier`
)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: ReviewDetailPage,
})

type BranchState =
  | { kind: `loading` }
  | { kind: `files`; files: PullFile[] }
  | { kind: `none` } // branch was never pushed (GitHub 404)
  | { kind: `error`; message: string }

// Tier-3 diff: a pushed branch with no PR yet (ported from the retired
// issue-changes-tab). branchDiff returns null when the branch was never pushed.
function BranchDiffSection({
  issueId,
  identifier,
}: {
  issueId: string
  identifier: string
}) {
  const [branch, setBranch] = useState<BranchState>({ kind: `loading` })

  const load = useCallback(() => {
    let cancelled = false
    setBranch({ kind: `loading` })
    trpc.repositories.branchDiff
      .query({ issueId })
      .then((res) => {
        if (cancelled) return
        if (!res) {
          setBranch({ kind: `none` })
          return
        }
        setBranch({ kind: `files`, files: res.files })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setBranch({
          kind: `error`,
          message: err instanceof Error ? err.message : `Failed to load branch`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [issueId])

  useEffect(() => load(), [load])

  if (branch.kind === `loading`) {
    return (
      <div className="flex items-center gap-2 px-4 py-3 text-xs text-muted-foreground">
        <Loader2 className="size-3.5 animate-spin" /> Loading changes…
      </div>
    )
  }

  if (branch.kind === `files` && branch.files.length > 0) {
    return (
      <div>
        <div className="flex min-w-0 items-center gap-2 border-b border-border px-4 py-2 text-sm">
          <GitBranch className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">
            Branch <span className="font-mono">exp/{identifier}</span>
            {` — no PR yet`}
          </span>
          <Button
            variant="ghost"
            size="icon-xs"
            className="ml-auto shrink-0 text-muted-foreground"
            aria-label="Refresh changes"
            onClick={() => load()}
          >
            <RotateCw className="size-3.5" />
          </Button>
        </div>
        <FileDiffList files={branch.files} />
      </div>
    )
  }

  return (
    <div className="px-4 py-6 text-xs text-muted-foreground">
      {branch.kind === `error`
        ? `Couldn’t load branch changes: ${branch.message}`
        : `No changes yet. A pushed branch or pull request will appear here.`}
    </div>
  )
}

function ReviewDetailPage() {
  const { workspaceSlug, issueIdentifier } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const projects = useWorkspaceProjects(workspace?.id)

  const projectIds = useMemo(() => {
    const ids = projects.map((p) => p.id)
    ids.sort()
    return ids
  }, [projects])
  const projectSlugById = useMemo(
    () => new Map(projects.map((p) => [p.id, p.slug])),
    [projects]
  )

  const { data: issueRows } = useLiveQuery(
    (query) =>
      projectIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.projectId, projectIds),
                eq(issues.identifier, issueIdentifier)
              )
            )
        : undefined,
    [projectIds.join(`,`), issueIdentifier]
  )
  const issue = (issueRows?.[0] ?? null) as Issue | null

  // Every issue sharing this PR (a batch run links several) — newest first.
  const { data: linkedRows } = useLiveQuery(
    (query) =>
      issue?.prUrl
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) => eq(issues.prUrl, issue.prUrl))
        : undefined,
    [issue?.prUrl]
  )
  const linked = useMemo(
    () =>
      ((linkedRows ?? []) as Issue[])
        .filter((i) => i.archivedAt == null)
        .sort(
          (a, b) =>
            new Date(b.createdAt).getTime() - new Date(a.createdAt).getTime()
        ),
    [linkedRows]
  )

  // Merge / close hold their spinner until the Electric echo flips prState away
  // from `open` (which hides the actions), matching the Reviews list.
  const [merging, setMerging] = useState(false)
  const [closing, setClosing] = useState(false)
  const [confirmMergeOpen, setConfirmMergeOpen] = useState(false)
  const [confirmCloseOpen, setConfirmCloseOpen] = useState(false)

  const confirmMerge = () => {
    if (!issue) return
    setConfirmMergeOpen(false)
    setMerging(true)
    trpc.issues.mergePr.mutate({ issueId: issue.id }).catch(() => {
      setMerging(false)
    })
  }

  const confirmClose = () => {
    if (!issue) return
    setConfirmCloseOpen(false)
    setClosing(true)
    trpc.issues.closePr.mutate({ issueId: issue.id }).catch(() => {
      setClosing(false)
    })
  }

  const openIssue = (linkedIssue: Issue) => {
    const projectSlug = projectSlugById.get(linkedIssue.projectId)
    if (!projectSlug) return
    void navigate({
      to: `/t/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
      params: {
        workspaceSlug,
        projectSlug,
        issueIdentifier: linkedIssue.identifier,
      },
    })
  }

  if (!workspace) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  if (!issue) {
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm">
        <div className="text-muted-foreground">
          Review <span className="font-mono">{issueIdentifier}</span> not found.
        </div>
        <Link
          to="/t/$workspaceSlug/reviews"
          params={{ workspaceSlug }}
          className="text-foreground underline-offset-2 hover:underline"
        >
          ← Back to reviews
        </Link>
      </div>
    )
  }

  const isOpen = issue.prState === `open`
  const isBatch = linked.length > 1

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Breadcrumb */}
      <div className="flex items-center gap-1.5 border-b border-border px-4 py-2 text-xs text-muted-foreground">
        <Link
          to="/t/$workspaceSlug/reviews"
          params={{ workspaceSlug }}
          className="hover:text-foreground"
        >
          Reviews
        </Link>
        <span className="text-muted-foreground/50">/</span>
        <span className="font-mono text-foreground">{issue.identifier}</span>
      </div>

      {/* Header: PR identity + actions */}
      <div className="flex min-w-0 items-center gap-2 border-b border-border px-4 py-2 text-sm">
        <GitPullRequest className="size-4 shrink-0 text-muted-foreground" />
        {issue.prNumber != null ? (
          issue.prUrl ? (
            <a
              href={issue.prUrl}
              target="_blank"
              rel="noreferrer"
              className="inline-flex shrink-0 items-center gap-1 font-mono hover:underline"
            >
              PR #{issue.prNumber}
              <ExternalLink className="size-3.5" />
            </a>
          ) : (
            <span className="shrink-0 font-mono">PR #{issue.prNumber}</span>
          )
        ) : (
          <span className="shrink-0 text-muted-foreground">No pull request</span>
        )}
        <PrStateBadge state={issue.prState} />
        {issue.branch && (
          <span className="hidden truncate font-mono text-xs text-muted-foreground md:inline">
            {issue.branch}
          </span>
        )}
        <div className="ml-auto flex shrink-0 items-center gap-2">
          {isOpen && (
            <>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7 text-muted-foreground"
                aria-label="Close pull request without merging"
                title="Close PR without merging"
                disabled={merging || closing}
                onClick={() => setConfirmCloseOpen(true)}
              >
                {closing ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <X className="h-3.5 w-3.5" />
                )}
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={merging || closing}
                onClick={() => setConfirmMergeOpen(true)}
              >
                {merging ? (
                  <>
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    Merging…
                  </>
                ) : (
                  <>
                    <GitMerge className="h-3.5 w-3.5" />
                    Merge
                  </>
                )}
              </Button>
            </>
          )}
        </div>
      </div>

      {/* Linked-issue chips (batch PRs) */}
      {isBatch && (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-border px-4 py-2">
          <span className="text-xs text-muted-foreground">
            {linked.length} linked issues
          </span>
          {linked.map((linkedIssue) => (
            <Button
              key={linkedIssue.id}
              variant="outline"
              size="xs"
              className="h-5 rounded-full px-2 font-mono text-xs"
              onClick={() => openIssue(linkedIssue)}
            >
              #{linkedIssue.identifier}
            </Button>
          ))}
        </div>
      )}

      {/* Diff body */}
      <div className="mx-auto w-full max-w-5xl flex-1 overflow-y-auto">
        {issue.prNumber != null ? (
          <DiffView issueId={issue.id} />
        ) : (
          <BranchDiffSection issueId={issue.id} identifier={issue.identifier} />
        )}
      </div>

      <Dialog open={confirmMergeOpen} onOpenChange={setConfirmMergeOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {isBatch
                ? `Merge PR #${issue.prNumber}?`
                : `Merge ${issue.identifier}?`}
            </DialogTitle>
            <DialogDescription>
              {`Squash-merges pull request #${issue.prNumber}${issue.branch ? ` (${issue.branch})` : ``} into the repository's default branch via the GitHub App.`}
              {isBatch
                ? ` Completes all ${linked.length} linked issues: ${linked
                    .map((i) => i.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setConfirmMergeOpen(false)}
            >
              Cancel
            </Button>
            <Button onClick={confirmMerge}>Merge pull request</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmCloseOpen} onOpenChange={setConfirmCloseOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {isBatch
                ? `Close PR #${issue.prNumber}?`
                : `Close ${issue.identifier}'s pull request?`}
            </DialogTitle>
            <DialogDescription>
              {`Closes pull request #${issue.prNumber}${issue.branch ? ` (${issue.branch})` : ``} on GitHub WITHOUT merging — use this when the issue was dropped even though the work exists. The branch is kept; the PR can be reopened on GitHub.`}
              {isBatch
                ? ` The PR is linked to ${linked.length} issues: ${linked
                    .map((i) => i.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setConfirmCloseOpen(false)}
            >
              Cancel
            </Button>
            <Button variant="destructive" onClick={confirmClose}>
              Close pull request
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
