import { useEffect, useMemo, useRef, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { GitBranch, GitMerge, GitPullRequest, LoaderCircle } from "lucide-react"
import type { OpenPull } from "@/lib/integrations/github-pr"
import { EmptyState } from "@/components/empty-state"
import { useSteerConfig } from "@/components/agent-session"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useReviewsData, type ReviewEntry } from "@/hooks/use-reviews-data"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { BUILTIN_FIX_CONFLICTS_ID } from "@/lib/builtin-actions"
import { mergeFailure, type MergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import { Pill } from "@/components/ui/pill"
import { Button } from "@/components/ui/button"
import { BoardGlyph } from "@/components/board-glyph"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  GlassRow,
  GlassSectionHeader,
} from "@/components/ui/glass-rows"

// Cross-board review queue: every issue in the team with an open PR,
// grouped by board, with a one-click (confirmed) squash-merge that goes
// through the GitHub App server-side. Deliberately filter-free — the queue
// should be short. Open PRs WITHOUT any link (manual PRs, external
// contributors) are listed last, grouped by repository, straight from GitHub.
export const Route = createFileRoute(`/t/$teamSlug/reviews/`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
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
  // A refused merge (conflicts, branch protection, GitHub App errors) captions
  // ITS row, keyed by entry.key (EXP-323) — the global toast is transient and
  // gave the conflict-recovery run nowhere to live.
  const [mergeErrors, setMergeErrors] = useState<
    Record<string, MergeFailure>
  >({})

  // A refusal describes ONE snapshot of the PR. Every entry stamps the issue
  // row its caption was taken from; when Electric echoes a newer `updatedAt`
  // for that row the caption — and with it the "Fix conflicts" swap — is
  // stale, so it clears itself and the plain Merge button comes back. Without
  // this a conflict resolved OUTSIDE the recovery run (a teammate rebases and
  // pushes) would hide Merge for the life of the open PR.
  const stamps = useMemo(() => {
    const map: Record<string, string> = {}
    for (const group of groups) {
      for (const entry of group.entries) {
        map[entry.key] = String(entry.issue.updatedAt ?? ``)
      }
    }
    return map
  }, [groups])
  const stampSignature = Object.entries(stamps)
    .map(([key, value]) => `${key}=${value}`)
    .join(`|`)
  const seenStamps = useRef<Record<string, string>>({})
  useEffect(() => {
    const seen = seenStamps.current
    const refreshed = Object.keys(stamps).filter(
      (key) => key in seen && seen[key] !== stamps[key]
    )
    seenStamps.current = stamps
    if (refreshed.length === 0) return
    setMergeErrors((prev) => {
      if (!refreshed.some((key) => key in prev)) return prev
      const next = { ...prev }
      for (const key of refreshed) delete next[key]
      return next
    })
    // `stamps` is derived from the signature; depending on it directly would
    // re-run on every render of a freshly-built object.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stampSignature])

  // "Fix conflicts" (EXP-323, desktop parity): the launch dialog opened on the
  // builtin action with THIS pull request already picked. Presence is fetched
  // only once a merge has actually failed — a plain Reviews visit must not
  // poll for desktops, but waiting for the click would open the dialog on a
  // momentary "no desktop online".
  const [fixTarget, setFixTarget] = useState<ReviewEntry | null>(null)
  const { data: session } = useSession()
  const currentUserId = session?.user?.id
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  // Only a REAL conflict can be fixed by the recovery run (EXP-533), so only a
  // real conflict is worth polling for an online desktop.
  const hasConflict = Object.values(mergeErrors).some(
    (failure) => failure.conflict
  )
  const remote = useRemoteStart({
    enabled: steerEnabled && hasConflict,
    currentUserId,
    teamId: team?.id,
  })

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
    setMergeErrors((prev) => {
      const next = { ...prev }
      delete next[entry.key]
      return next
    })
    // Merging through the representative issue merges the ONE PR — the server
    // then completes every linked issue and ends its live coding sessions
    // (EXP-498).
    trpc.issues.mergePr
      .mutate({ issueId: entry.issue.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        // Captioned on the row instead of toasted: the reason (GitHub's
        // verbatim "not mergeable") has to stay next to the recovery button,
        // and unstick the spinner so the merge can be retried.
        setMergeErrors((prev) => ({
          ...prev,
          [entry.key]: mergeFailure(
            error,
            `The pull request could not be merged`
          ),
        }))
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
      <div className={`flex-1 overflow-y-auto ${TAB_BAR_CLEARANCE}`}>
        {isLoading ? (
          <div className="text-muted-foreground px-1 py-6 text-sm">Loading…</div>
        ) : count === 0 ? (
          externalLoading ? (
            <div className="text-muted-foreground px-1 py-6 text-sm">
              Loading…
            </div>
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
              <div key={group.board.id} className="mb-6">
                <GlassSectionHeader
                  leading={
                    <BoardGlyph board={group.board} className="size-3.5" />
                  }
                  label={group.board.name}
                />

                <div className="flex flex-col gap-2">
                  {group.entries.map((entry) => {
                    const issue = entry.issue
                    const isBatch = entry.issues.length > 1
                    const merging = mergingIds.has(entry.key)
                    const mergeError = mergeErrors[entry.key]
                    // The recovery run rebases the PR's branch, so it needs one
                    // recorded — the same guard the desktop applies.
                    // EXP-533: only for a real content conflict. A stale base,
                    // a branch-protection refusal or an unreachable server all
                    // fail the merge too, and a rebase-and-resolve run fixes
                    // none of them.
                    const canFixConflicts = Boolean(
                      mergeError?.conflict && issue.branch && steerEnabled
                    )
                    return (
                      <GlassRow
                        key={entry.key}
                        interactive
                        className="group/row grid grid-cols-[1.5rem_4.5rem_1fr_auto] gap-0"
                        onClick={() => openReview(issue.identifier)}
                        data-testid={`review-row-${issue.identifier}`}
                      >
                        <GitPullRequest className="h-4 w-4 text-emerald-500" />
                        <span className="truncate font-mono text-xs text-muted-foreground">
                          {isBatch && issue.prNumber
                            ? `#${issue.prNumber}`
                            : issue.identifier}
                        </span>
                        {/* EXP-698: pr-3 IS the gap to the trailing Merge
                            control — on a phone the two used to sit 8px
                            apart, which read as one blob. */}
                        <div className="min-w-0 pr-3">
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
                        {/* EXP-706: the recovery run takes the Merge button's
                            OWN slot on a real conflict — one trailing action
                            per row, never two.
                            EXP-698: the row's Merge and the review detail's
                            header Merge are ONE control at ONE weight —
                            `Pill size="md" mode="action"`. */}
                        {canFixConflicts ? (
                          <Pill
                            size="md"
                            mode="action"
                            onClick={(e) => {
                              e.stopPropagation()
                              setFixTarget(entry)
                            }}
                          >
                            <GitBranch className="h-3.5 w-3.5" />
                            Fix conflicts
                          </Pill>
                        ) : (
                          <Pill
                            size="md"
                            mode="action"
                            disabled={merging}
                            onClick={(e) => {
                              e.stopPropagation()
                              setMergeTarget(entry)
                            }}
                          >
                            {merging ? (
                              <>
                                <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                                Merging…
                              </>
                            ) : (
                              <>
                                <GitMerge className="h-3.5 w-3.5" />
                                Merge
                              </>
                            )}
                          </Pill>
                        )}
                        {/* The refusal captions its own row (EXP-323) —
                            spanning the grid so the full GitHub message stays
                            readable. Message only since EXP-706, except while
                            the recovery run holds the trailing slot: Merge
                            then rides the caption as a quiet secondary so the
                            swap is never a dead end (the conflict may have
                            been resolved outside the recovery run). */}
                        {mergeError && (
                          <div className="col-span-4 flex flex-wrap items-center gap-2 pt-2">
                            <span className="text-destructive text-xs">
                              {mergeError.message}
                            </span>
                            {canFixConflicts && (
                              <Pill
                                mode="action"
                                disabled={merging}
                                onClick={(e) => {
                                  e.stopPropagation()
                                  setMergeTarget(entry)
                                }}
                              >
                                <GitMerge className="size-3" />
                                Retry merge
                              </Pill>
                            )}
                          </div>
                        )}
                      </GlassRow>
                    )
                  })}
                </div>
              </div>
            ))}

            {externalGroups.map((group) => (
              <div key={group.repositoryId} className="mb-6">
                <GlassSectionHeader
                  leading={
                    <GitPullRequest className="h-2.5 w-2.5 shrink-0 text-foreground/50" />
                  }
                  label={group.fullName}
                  trailing={
                    <span className="text-xs text-foreground/50">not linked to an issue</span>
                  }
                />

                <div className="flex flex-col gap-2">
                  {group.pulls.map((pull) => {
                    const key = externalPullKey(group.repositoryId, pull.number)
                    const merging = mergingIds.has(key)
                    return (
                      <GlassRow
                        key={pull.number}
                        interactive
                        className="group/row grid grid-cols-[1.5rem_4.5rem_1fr_auto] gap-0"
                        onClick={() =>
                          window.open(pull.url, `_blank`, `noopener,noreferrer`)
                        }
                        data-testid={`review-pull-${group.fullName}-${pull.number}`}
                      >
                        <GitPullRequest className="h-4 w-4 text-emerald-500" />
                        <span className="truncate font-mono text-xs text-muted-foreground">
                          #{pull.number}
                        </span>
                        <div className="min-w-0 pr-3">
                          <div className="flex min-w-0 items-center gap-2">
                            <span className="min-w-0 truncate text-sm">
                              {pull.title}
                            </span>
                            {pull.draft && <Pill>Draft</Pill>}
                          </div>
                          {pull.branch && (
                            <div className="truncate font-mono text-xs text-muted-foreground">
                              {pull.branch}
                            </div>
                          )}
                        </div>
                        <Pill
                          size="md"
                          mode="action"
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
                              <LoaderCircle className="h-3.5 w-3.5 animate-spin" />
                              Merging…
                            </>
                          ) : (
                            <>
                              <GitMerge className="h-3.5 w-3.5" />
                              Merge
                            </>
                          )}
                        </Pill>
                      </GlassRow>
                    )
                  })}
                </div>
              </div>
            ))}
          </>
        )}
      </div>

      {/* "Fix conflicts" (EXP-323): the launcher on the builtin action with
          this PR pre-picked. */}
      {fixTarget && (
        <LaunchDialog
          open
          onOpenChange={(next) => {
            if (!next) setFixTarget(null)
          }}
          devices={remote.devices ?? []}
          starting={remote.starting}
          teamId={team.id}
          initialTab="actions"
          initialActionId={BUILTIN_FIX_CONFLICTS_ID}
          initialPrIssueId={fixTarget.issue.id}
          onStartIssues={(device, options, issueIds) => {
            remote
              .startIssues(device, options, issueIds)
              .then(() => setFixTarget(null))
              .catch(() => {})
          }}
          onRunAction={(device, action, options, inputs) => {
            remote
              .runAction(device, action, options, inputs)
              .then(() => setFixTarget(null))
              .catch(() => {})
          }}
        />
      )}

      <Dialog
        open={mergeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setMergeTarget(null)
        }}
      >
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>
              {mergeTarget && mergeTarget.issues.length > 1
                ? `Merge PR #${mergeTarget.issue.prNumber}?`
                : `Merge ${mergeTarget?.issue.identifier}?`}
            </DialogTitle>
            <DialogDescription>
              {`Squash-merges pull request #${mergeTarget?.issue.prNumber} (${mergeTarget?.issue.branch}) into the repository's default branch via the GitHub App. Any live coding session for it closes.`}
              {mergeTarget && mergeTarget.issues.length > 1
                ? ` Completes all ${mergeTarget.issues.length} linked issues: ${mergeTarget.issues
                    .map((linked) => linked.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel onClick={() => setMergeTarget(null)} />
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
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>{`Merge ${externalMergeTarget?.fullName}#${externalMergeTarget?.pull.number}?`}</DialogTitle>
            <DialogDescription>
              {`Squash-merges "${externalMergeTarget?.pull.title}" (${externalMergeTarget?.pull.branch} → ${externalMergeTarget?.pull.baseBranch}) via the GitHub App. This pull request is not linked to an issue.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel onClick={() => setExternalMergeTarget(null)} />
            <Button onClick={confirmExternalMerge}>Merge pull request</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
