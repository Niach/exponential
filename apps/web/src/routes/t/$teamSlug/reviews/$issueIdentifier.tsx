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
  useTeamBySlug,
  useTeamBoards,
} from "@/hooks/use-team-data"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { BUILTIN_FIX_CONFLICTS_ID } from "@/lib/builtin-actions"
import { trpcErrorMessage } from "@/lib/trpc-error"
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
import { useSteerConfig } from "@/components/agent-session"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"

// Review-detail (EXP-106): the PR/branch diff for one review, with Merge/Close
// actions moved off the issue detail. The representative issue carries the PR;
// merging/closing acts on the ONE PR, and the server completes every linked
// issue (a batch run's issues all share one prUrl).
export const Route = createFileRoute(
  `/t/$teamSlug/reviews/$issueIdentifier`
)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
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
        <FileDiffList files={branch.files} showFileNav={false} defaultCollapsed />
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
  const { teamSlug, issueIdentifier } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const boards = useTeamBoards(team?.id)

  const boardIds = useMemo(() => {
    const ids = boards.map((p) => p.id)
    ids.sort()
    return ids
  }, [boards])
  const boardSlugById = useMemo(
    () => new Map(boards.map((p) => [p.id, p.slug])),
    [boards]
  )

  const { data: issueRows } = useLiveQuery(
    (query) =>
      boardIds.length > 0
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                inArray(issues.boardId, boardIds),
                eq(issues.identifier, issueIdentifier)
              )
            )
        : undefined,
    [boardIds.join(`,`), issueIdentifier]
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
      ((linkedRows ?? []) as Issue[]).sort(
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
  // A refused merge/close captions the action bar that produced it (EXP-323)
  // instead of only flashing a toast — the reason has to stay next to the
  // conflict-recovery button.
  const [actionError, setActionError] = useState<string | null>(null)

  // "Fix conflicts" (EXP-323, desktop parity). Presence is fetched only once
  // an action has actually failed — opening a review must not poll for
  // desktops, but waiting for the click would open the dialog on a momentary
  // "no desktop online".
  const [fixOpen, setFixOpen] = useState(false)
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const remote = useRemoteStart({
    enabled: steerEnabled && actionError !== null,
    currentUserId,
  })

  const confirmMerge = () => {
    if (!issue) return
    setConfirmMergeOpen(false)
    setMerging(true)
    setActionError(null)
    trpc.issues.mergePr
      .mutate({ issueId: issue.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        setActionError(
          trpcErrorMessage(error, `The pull request could not be merged`)
        )
        setMerging(false)
      })
  }

  const confirmClose = () => {
    if (!issue) return
    setConfirmCloseOpen(false)
    setClosing(true)
    setActionError(null)
    trpc.issues.closePr
      .mutate({ issueId: issue.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        setActionError(
          trpcErrorMessage(error, `The pull request could not be closed`)
        )
        setClosing(false)
      })
  }

  const openIssue = (linkedIssue: Issue) => {
    const boardSlug = boardSlugById.get(linkedIssue.boardId)
    if (!boardSlug) return
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug,
        issueIdentifier: linkedIssue.identifier,
      },
    })
  }

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  if (!issue) {
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm">
        <div className="text-muted-foreground">
          Review <span className="font-mono">{issueIdentifier}</span> not found.
        </div>
        <Link
          to="/t/$teamSlug/reviews"
          params={{ teamSlug }}
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
          to="/t/$teamSlug/reviews"
          params={{ teamSlug }}
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

      {/* Diff body — bottom padding clears the floating action bar */}
      <div className="mx-auto w-full max-w-5xl flex-1 overflow-y-auto pb-24">
        {issue.prNumber != null ? (
          <DiffView issueId={issue.id} showFileNav={false} defaultCollapsed />
        ) : (
          <BranchDiffSection issueId={issue.id} identifier={issue.identifier} />
        )}
      </div>

      {/* Floating action bar (EXP-248) — dismiss · Merge · GitHub, matching the
          mobile clients' review-detail bar and the app's glass-pill chrome. */}
      {(isOpen || issue.prUrl) && (
        <div className="pointer-events-none fixed inset-x-0 bottom-0 z-40 flex flex-col items-center gap-2 px-4 pb-[max(1.25rem,env(safe-area-inset-bottom))]">
          {actionError && (
            <div className="pointer-events-auto flex max-w-lg flex-wrap items-center justify-center gap-2 rounded-lg border border-glass-stroke-card bg-popover/85 px-3 py-2 shadow-lg shadow-black/40 backdrop-blur-xl">
              <span className="text-destructive text-xs">{actionError}</span>
              {/* The recovery run rebases the PR's branch, so it needs one
                  recorded — the same guard the desktop applies. */}
              {isOpen && issue.branch && steerEnabled && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => setFixOpen(true)}
                >
                  <GitBranch className="size-3.5" />
                  Fix conflicts
                </Button>
              )}
            </div>
          )}
          <div className="flex items-center justify-center gap-3">
            {isOpen && (
              <Button
                variant="ghost"
                size="icon"
                className="pointer-events-auto size-11 rounded-full border border-glass-stroke-card bg-popover/85 text-muted-foreground shadow-lg shadow-black/40 backdrop-blur-xl hover:bg-muted/85 hover:text-foreground"
                aria-label="Close pull request without merging"
                title="Close PR without merging"
                disabled={merging || closing}
                onClick={() => setConfirmCloseOpen(true)}
              >
                {closing ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <X className="size-4" />
                )}
              </Button>
            )}
            {isOpen && (
              <Button
                className="pointer-events-auto h-12 rounded-full px-6 shadow-lg shadow-black/40"
                disabled={merging || closing}
                onClick={() => setConfirmMergeOpen(true)}
              >
                {merging ? (
                  <>
                    <Loader2 className="size-4 animate-spin" />
                    Merging…
                  </>
                ) : (
                  <>
                    <GitMerge className="size-4" />
                    Merge
                  </>
                )}
              </Button>
            )}
            {issue.prUrl && (
              <Button
                variant="ghost"
                size="icon"
                className="pointer-events-auto size-11 rounded-full border border-glass-stroke-card bg-popover/85 text-muted-foreground shadow-lg shadow-black/40 backdrop-blur-xl hover:bg-muted/85 hover:text-foreground"
                aria-label="Open pull request on GitHub"
                title="Open PR on GitHub"
                onClick={() =>
                  window.open(
                    issue.prUrl ?? ``,
                    `_blank`,
                    `noopener,noreferrer`
                  )
                }
              >
                <ExternalLink className="size-4" />
              </Button>
            )}
          </div>
        </div>
      )}

      {fixOpen && (
        <LaunchDialog
          open
          onOpenChange={(next) => {
            if (!next) setFixOpen(false)
          }}
          devices={remote.devices ?? []}
          starting={remote.starting}
          teamId={team.id}
          initialTab="actions"
          initialActionId={BUILTIN_FIX_CONFLICTS_ID}
          initialPrIssueId={issue.id}
          onStartIssues={(device, options, issueIds) => {
            remote
              .startIssues(device, options, issueIds)
              .then(() => setFixOpen(false))
              .catch(() => {})
          }}
          onRunAction={(device, action, options, inputs) => {
            remote
              .runAction(device, action, options, inputs)
              .then(() => setFixOpen(false))
              .catch(() => {})
          }}
        />
      )}

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
