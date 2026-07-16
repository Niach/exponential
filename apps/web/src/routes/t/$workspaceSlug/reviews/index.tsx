import { useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  ExternalLink,
  GitMerge,
  GitPullRequest,
  Loader2,
  X,
} from "lucide-react"
import type { OpenPull } from "@/lib/integrations/github-pr"
import { EmptyState } from "@/components/empty-state"
import { useReviewsData, type ReviewEntry } from "@/hooks/use-reviews-data"
import { useWorkspaceBySlug } from "@/hooks/use-workspace-data"
import { trpc } from "@/lib/trpc-client"
import { getInitials } from "@/lib/utils"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

// Cross-project review queue: every issue in the workspace with an open PR,
// grouped by project, with a one-click (confirmed) squash-merge that goes
// through the GitHub App server-side. Deliberately filter-free — the queue
// should be short. Open PRs WITHOUT any link (manual PRs, external
// contributors) are listed last, grouped by repository, straight from GitHub.
export const Route = createFileRoute(`/t/$workspaceSlug/reviews/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: ReviewsPage,
})

interface ExternalMergeTarget {
  repositoryId: string
  fullName: string
  pull: OpenPull
}

function ReviewsPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const {
    groups,
    externalGroups,
    count,
    isLoading,
    externalLoading,
    userMap,
    removeExternalPull,
  } = useReviewsData(workspace)

  // The entry whose confirm dialog is open, and the entries with an in-flight
  // merge (keyed by entry.key). A successful merge keeps its spinner until the
  // Electric echo flips prState and the entry leaves the list; external PRs
  // have no echo and are removed locally on success.
  const [mergeTarget, setMergeTarget] = useState<ReviewEntry | null>(null)
  const [mergingIds, setMergingIds] = useState<Set<string>>(new Set())
  const [externalMergeTarget, setExternalMergeTarget] =
    useState<ExternalMergeTarget | null>(null)
  // The reject path (EXP-100): close WITHOUT merging — deliberately subtle
  // (hover-revealed ghost ×); merge stays the primary action. Same spinner
  // semantics as merge: success waits for the Electric echo to drop the entry.
  const [closeTarget, setCloseTarget] = useState<ReviewEntry | null>(null)
  const [closingIds, setClosingIds] = useState<Set<string>>(new Set())

  const openIssue = (projectSlug: string, issueIdentifier: string) => {
    void navigate({
      to: `/t/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
      params: { workspaceSlug, projectSlug, issueIdentifier },
    })
  }

  const confirmMerge = () => {
    const entry = mergeTarget
    if (!entry) return
    setMergeTarget(null)
    setMergingIds((prev) => new Set(prev).add(entry.key))
    // Merging through the representative issue merges the ONE PR — the server
    // then completes every linked issue.
    trpc.issues.mergePr.mutate({ issueId: entry.issue.id }).catch(() => {
      // The global mutation toast already surfaced the error; just unstick
      // the row spinner so the merge can be retried.
      setMergingIds((prev) => {
        const next = new Set(prev)
        next.delete(entry.key)
        return next
      })
    })
  }

  const confirmClose = () => {
    const entry = closeTarget
    if (!entry) return
    setCloseTarget(null)
    setClosingIds((prev) => new Set(prev).add(entry.key))
    trpc.issues.closePr.mutate({ issueId: entry.issue.id }).catch(() => {
      // The global mutation toast already surfaced the error; just unstick
      // the row spinner so the close can be retried.
      setClosingIds((prev) => {
        const next = new Set(prev)
        next.delete(entry.key)
        return next
      })
    })
  }

  const externalPullKey = (repositoryId: string, prNumber: number) =>
    `${repositoryId}#${prNumber}`

  const confirmExternalMerge = () => {
    const target = externalMergeTarget
    if (!target) return
    setExternalMergeTarget(null)
    const key = externalPullKey(target.repositoryId, target.pull.number)
    setMergingIds((prev) => new Set(prev).add(key))
    trpc.repositories.mergePull
      .mutate({
        repositoryId: target.repositoryId,
        prNumber: target.pull.number,
      })
      .then(() => {
        removeExternalPull(target.repositoryId, target.pull.number)
      })
      .catch(() => {
        // Toast already shown — unstick the spinner for a retry.
      })
      .finally(() => {
        setMergingIds((prev) => {
          const next = new Set(prev)
          next.delete(key)
          return next
        })
      })
  }

  if (!workspace) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <GitPullRequest className="h-4 w-4" />
          Reviews
          {count > 0 && (
            <span className="text-sm font-normal text-muted-foreground">
              · {count} open
            </span>
          )}
        </h1>
      </div>

      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="text-muted-foreground p-6 text-sm">Loading…</div>
        ) : count === 0 ? (
          externalLoading ? (
            <div className="text-muted-foreground p-6 text-sm">Loading…</div>
          ) : (
            <EmptyState
              icon={GitPullRequest}
              title="No open pull requests"
              description="Open pull requests in this workspace's repositories land here for review."
            />
          )
        ) : (
          <>
          {groups.map((group) => (
            <div key={group.project.id} className="mb-4">
              <div
                className="flex items-center gap-1.5 rounded-t-md border-b border-border/50 px-3 py-1.5"
                style={{ backgroundColor: `rgba(113, 113, 122, 0.08)` }}
              >
                <div
                  className="h-2.5 w-2.5 shrink-0 rounded-full"
                  style={{ backgroundColor: group.project.color }}
                />
                <span className="text-sm font-medium">
                  {group.project.name}
                </span>
                <span className="text-xs text-muted-foreground">
                  {group.entries.length}
                </span>
              </div>

              {group.entries.map((entry) => {
                const issue = entry.issue
                const isBatch = entry.issues.length > 1
                const assignee = issue.assigneeId
                  ? userMap.get(issue.assigneeId)
                  : undefined
                const merging = mergingIds.has(entry.key)
                const closing = closingIds.has(entry.key)
                return (
                  <div
                    key={entry.key}
                    className="group/row grid h-11 cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 hover:bg-muted/50"
                    onClick={() =>
                      openIssue(group.project.slug, issue.identifier)
                    }
                    data-testid={`review-row-${issue.identifier}`}
                  >
                    <GitPullRequest className="h-4 w-4 text-emerald-500" />
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      {isBatch && issue.prNumber
                        ? `#${issue.prNumber}`
                        : issue.identifier}
                    </span>
                    <span className="min-w-0 truncate pr-2 text-sm">
                      {isBatch ? (
                        <>
                          {`${entry.issues.length} issues`}
                          <span className="ml-2 font-mono text-xs text-muted-foreground">
                            {entry.issues
                              .map((linked) => linked.identifier)
                              .join(`, `)}
                          </span>
                        </>
                      ) : (
                        issue.title
                      )}
                    </span>
                    <div className="flex items-center gap-2">
                      {issue.branch && (
                        <Badge
                          variant="outline"
                          className="hidden font-mono text-xs md:inline-flex"
                        >
                          {issue.branch}
                        </Badge>
                      )}
                      {issue.prUrl && (
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          aria-label="Open pull request on GitHub"
                          onClick={(e) => {
                            e.stopPropagation()
                            window.open(
                              issue.prUrl ?? ``,
                              `_blank`,
                              `noopener,noreferrer`
                            )
                          }}
                        >
                          <ExternalLink className="h-3.5 w-3.5" />
                        </Button>
                      )}
                      {assignee && (
                        <Avatar className="size-5">
                          {assignee.image && (
                            <AvatarImage
                              src={assignee.image}
                              alt={assignee.name}
                            />
                          )}
                          <AvatarFallback className="text-[0.625rem]">
                            {getInitials(assignee.name)}
                          </AvatarFallback>
                        </Avatar>
                      )}
                      <Button
                        variant="ghost"
                        size="icon"
                        className={`h-7 w-7 text-muted-foreground ${
                          closing
                            ? ``
                            : `md:opacity-0 md:group-hover/row:opacity-100 md:focus-visible:opacity-100`
                        }`}
                        aria-label="Close pull request without merging"
                        title="Close PR without merging"
                        disabled={merging || closing}
                        onClick={(e) => {
                          e.stopPropagation()
                          setCloseTarget(entry)
                        }}
                      >
                        {closing ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <X className="h-3.5 w-3.5" />
                        )}
                      </Button>
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={merging || closing}
                        onClick={(e) => {
                          e.stopPropagation()
                          setMergeTarget(entry)
                        }}
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
                    </div>
                  </div>
                )
              })}
            </div>
          ))}

          {externalGroups.map((group) => (
            <div key={group.repositoryId} className="mb-4">
              <div
                className="flex items-center gap-1.5 rounded-t-md border-b border-border/50 px-3 py-1.5"
                style={{ backgroundColor: `rgba(113, 113, 122, 0.08)` }}
              >
                <GitPullRequest className="h-2.5 w-2.5 shrink-0 text-muted-foreground" />
                <span className="text-sm font-medium">{group.fullName}</span>
                <span className="text-xs text-muted-foreground">
                  not linked to an issue · {group.pulls.length}
                </span>
              </div>

              {group.pulls.map((pull) => {
                const key = externalPullKey(group.repositoryId, pull.number)
                const merging = mergingIds.has(key)
                return (
                  <div
                    key={pull.number}
                    className="group/row grid h-11 cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 hover:bg-muted/50"
                    onClick={() =>
                      window.open(pull.url, `_blank`, `noopener,noreferrer`)
                    }
                    data-testid={`review-pull-${group.fullName}-${pull.number}`}
                  >
                    <GitPullRequest className="h-4 w-4 text-emerald-500" />
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      #{pull.number}
                    </span>
                    <span className="min-w-0 truncate pr-2 text-sm">
                      {pull.title}
                    </span>
                    <div className="flex items-center gap-2">
                      {pull.draft && <Badge variant="secondary">Draft</Badge>}
                      {pull.branch && (
                        <Badge
                          variant="outline"
                          className="hidden font-mono text-xs md:inline-flex"
                        >
                          {pull.branch}
                        </Badge>
                      )}
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        aria-label="Open pull request on GitHub"
                        onClick={(e) => {
                          e.stopPropagation()
                          window.open(pull.url, `_blank`, `noopener,noreferrer`)
                        }}
                      >
                        <ExternalLink className="h-3.5 w-3.5" />
                      </Button>
                      {pull.authorAvatarUrl && (
                        <Avatar className="size-5">
                          <AvatarImage
                            src={pull.authorAvatarUrl}
                            alt={pull.authorLogin ?? `PR author`}
                          />
                          <AvatarFallback className="text-[0.625rem]">
                            {getInitials(pull.authorLogin ?? `?`)}
                          </AvatarFallback>
                        </Avatar>
                      )}
                      <Button
                        size="sm"
                        variant="outline"
                        disabled={merging || pull.draft}
                        onClick={(e) => {
                          e.stopPropagation()
                          setExternalMergeTarget({
                            repositoryId: group.repositoryId,
                            fullName: group.fullName,
                            pull,
                          })
                        }}
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
                    </div>
                  </div>
                )
              })}
            </div>
          ))}
          </>
        )}
      </div>

      <Dialog
        open={mergeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setMergeTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {mergeTarget && mergeTarget.issues.length > 1
                ? `Merge PR #${mergeTarget.issue.prNumber}?`
                : `Merge ${mergeTarget?.issue.identifier}?`}
            </DialogTitle>
            <DialogDescription>
              {`Squash-merges pull request #${mergeTarget?.issue.prNumber} (${mergeTarget?.issue.branch}) into the repository's default branch via the GitHub App.`}
              {mergeTarget && mergeTarget.issues.length > 1
                ? ` Completes all ${mergeTarget.issues.length} linked issues: ${mergeTarget.issues
                    .map((linked) => linked.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setMergeTarget(null)}>
              Cancel
            </Button>
            <Button onClick={confirmMerge}>Merge pull request</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={closeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setCloseTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>
              {closeTarget && closeTarget.issues.length > 1
                ? `Close PR #${closeTarget.issue.prNumber}?`
                : `Close ${closeTarget?.issue.identifier}'s pull request?`}
            </DialogTitle>
            <DialogDescription>
              {`Closes pull request #${closeTarget?.issue.prNumber} (${closeTarget?.issue.branch}) on GitHub WITHOUT merging — use this when the issue was dropped even though the work exists. The branch is kept; the PR can be reopened on GitHub.`}
              {closeTarget && closeTarget.issues.length > 1
                ? ` The PR is linked to ${closeTarget.issues.length} issues: ${closeTarget.issues
                    .map((linked) => linked.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setCloseTarget(null)}>
              Cancel
            </Button>
            <Button variant="destructive" onClick={confirmClose}>
              Close pull request
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={externalMergeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setExternalMergeTarget(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{`Merge ${externalMergeTarget?.fullName}#${externalMergeTarget?.pull.number}?`}</DialogTitle>
            <DialogDescription>
              {`Squash-merges "${externalMergeTarget?.pull.title}" (${externalMergeTarget?.pull.branch} → ${externalMergeTarget?.pull.baseBranch}) via the GitHub App. This pull request is not linked to an issue.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" onClick={() => setExternalMergeTarget(null)}>
              Cancel
            </Button>
            <Button onClick={confirmExternalMerge}>Merge pull request</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
