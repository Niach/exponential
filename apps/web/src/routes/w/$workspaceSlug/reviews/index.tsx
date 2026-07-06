import { useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { ExternalLink, GitMerge, GitPullRequest, Loader2 } from "lucide-react"
import type { Issue } from "@/db/schema"
import { EmptyState } from "@/components/empty-state"
import { useReviewsData } from "@/hooks/use-reviews-data"
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
// should be short.
export const Route = createFileRoute(`/w/$workspaceSlug/reviews/`)({
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

function ReviewsPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { groups, count, isLoading, userMap } = useReviewsData(workspace)

  // The row whose confirm dialog is open, and the rows with an in-flight
  // merge. A successful merge keeps its spinner until the Electric echo flips
  // prState and the row leaves the list.
  const [mergeTarget, setMergeTarget] = useState<Issue | null>(null)
  const [mergingIds, setMergingIds] = useState<Set<string>>(new Set())

  const openIssue = (projectSlug: string, issueIdentifier: string) => {
    void navigate({
      to: `/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
      params: { workspaceSlug, projectSlug, issueIdentifier },
    })
  }

  const confirmMerge = () => {
    const issue = mergeTarget
    if (!issue) return
    setMergeTarget(null)
    setMergingIds((prev) => new Set(prev).add(issue.id))
    trpc.issues.mergePr.mutate({ issueId: issue.id }).catch(() => {
      // The global mutation toast already surfaced the error; just unstick
      // the row spinner so the merge can be retried.
      setMergingIds((prev) => {
        const next = new Set(prev)
        next.delete(issue.id)
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
          <EmptyState
            icon={GitPullRequest}
            title="No open pull requests"
            description="When Claude opens a pull request for an issue, it lands here for review."
          />
        ) : (
          groups.map((group) => (
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
                  {group.issues.length}
                </span>
              </div>

              {group.issues.map((issue) => {
                const assignee = issue.assigneeId
                  ? userMap.get(issue.assigneeId)
                  : undefined
                const merging = mergingIds.has(issue.id)
                return (
                  <div
                    key={issue.id}
                    className="group/row grid h-11 cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 hover:bg-muted/50"
                    onClick={() =>
                      openIssue(group.project.slug, issue.identifier)
                    }
                    data-testid={`review-row-${issue.identifier}`}
                  >
                    <GitPullRequest className="h-4 w-4 text-emerald-500" />
                    <span className="truncate font-mono text-xs text-muted-foreground">
                      {issue.identifier}
                    </span>
                    <span className="min-w-0 truncate pr-2 text-sm">
                      {issue.title}
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
                        size="sm"
                        variant="outline"
                        disabled={merging}
                        onClick={(e) => {
                          e.stopPropagation()
                          setMergeTarget(issue)
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
          ))
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
            <DialogTitle>{`Merge ${mergeTarget?.identifier}?`}</DialogTitle>
            <DialogDescription>
              {`Squash-merges pull request #${mergeTarget?.prNumber} (${mergeTarget?.branch}) into the repository's default branch via the GitHub App.`}
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
    </div>
  )
}
