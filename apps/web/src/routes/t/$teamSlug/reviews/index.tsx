import { useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { GitMerge, GitPullRequest, Loader2 } from "lucide-react"
import type { OpenPull } from "@/lib/integrations/github-pr"
import { EmptyState } from "@/components/empty-state"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useReviewsData, type ReviewEntry } from "@/hooks/use-reviews-data"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
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

// Cross-board review queue: every issue in the team with an open PR,
// grouped by board, with a one-click (confirmed) squash-merge that goes
// through the GitHub App server-side. Deliberately filter-free — the queue
// should be short. Open PRs WITHOUT any link (manual PRs, external
// contributors) are listed last, grouped by repository, straight from GitHub.
export const Route = createFileRoute(`/t/$teamSlug/reviews/`)({
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
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const {
    groups,
    externalGroups,
    count,
    isLoading,
    externalLoading,
    removeExternalPull,
  } = useReviewsData(team)

  // The entry whose confirm dialog is open, and the entries with an in-flight
  // merge (keyed by entry.key). A successful merge keeps its spinner until the
  // Electric echo flips prState and the entry leaves the list; external PRs
  // have no echo and are removed locally on success.
  // Closing without merging lives on the review-detail page (EXP-248) — list
  // rows offer merge only, matching the iOS/Android review rows.
  const [mergeTarget, setMergeTarget] = useState<ReviewEntry | null>(null)
  const [mergingIds, setMergingIds] = useState<Set<string>>(new Set())
  const [externalMergeTarget, setExternalMergeTarget] =
    useState<ExternalMergeTarget | null>(null)

  // The row opens the review-detail page (PR/branch diff + Merge/Close), not the
  // issue itself — a batch entry's representative identifier stands for the PR.
  const openReview = (issueIdentifier: string) => {
    void navigate({
      to: `/t/$teamSlug/reviews/$issueIdentifier`,
      params: { teamSlug, issueIdentifier },
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

  if (!team) {
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

      <div className={`flex-1 overflow-y-auto ${TAB_BAR_CLEARANCE}`}>
        {isLoading ? (
          <div className="text-muted-foreground p-6 text-sm">Loading…</div>
        ) : count === 0 ? (
          externalLoading ? (
            <div className="text-muted-foreground p-6 text-sm">Loading…</div>
          ) : (
            <EmptyState
              icon={GitPullRequest}
              title="No open pull requests"
              description="Open pull requests in this team's repositories land here for review."
            />
          )
        ) : (
          <>
          {groups.map((group) => (
            <div key={group.board.id} className="mb-4">
              <div
                className="flex items-center gap-1.5 rounded-t-md border-b border-border/50 px-3 py-1.5"
                style={{ backgroundColor: `rgba(113, 113, 122, 0.08)` }}
              >
                <div
                  className="h-2.5 w-2.5 shrink-0 rounded-full"
                  style={{ backgroundColor: group.board.color }}
                />
                <span className="text-sm font-medium">
                  {group.board.name}
                </span>
                <span className="text-xs text-muted-foreground">
                  {group.entries.length}
                </span>
              </div>

              {group.entries.map((entry) => {
                const issue = entry.issue
                const isBatch = entry.issues.length > 1
                const merging = mergingIds.has(entry.key)
                return (
                  <div
                    key={entry.key}
                    className="group/row grid cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 py-1.5 hover:bg-muted/50"
                    onClick={() => openReview(issue.identifier)}
                    data-testid={`review-row-${issue.identifier}`}
                  >
                    <GitPullRequest className="h-4 w-4 text-emerald-500" />
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      {isBatch && issue.prNumber
                        ? `#${issue.prNumber}`
                        : issue.identifier}
                    </span>
                    <div className="min-w-0 pr-2">
                      <div className="truncate text-sm">
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
                      </div>
                      {issue.branch && (
                        <div className="truncate font-mono text-xs text-muted-foreground">
                          {issue.branch}
                        </div>
                      )}
                    </div>
                    <Button
                      size="sm"
                      variant="outline"
                      disabled={merging}
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
                    className="group/row grid cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 py-1.5 hover:bg-muted/50"
                    onClick={() =>
                      window.open(pull.url, `_blank`, `noopener,noreferrer`)
                    }
                    data-testid={`review-pull-${group.fullName}-${pull.number}`}
                  >
                    <GitPullRequest className="h-4 w-4 text-emerald-500" />
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      #{pull.number}
                    </span>
                    <div className="min-w-0 pr-2">
                      <div className="flex min-w-0 items-center gap-2">
                        <span className="min-w-0 truncate text-sm">
                          {pull.title}
                        </span>
                        {pull.draft && <Badge variant="secondary">Draft</Badge>}
                      </div>
                      {pull.branch && (
                        <div className="truncate font-mono text-xs text-muted-foreground">
                          {pull.branch}
                        </div>
                      )}
                    </div>
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
