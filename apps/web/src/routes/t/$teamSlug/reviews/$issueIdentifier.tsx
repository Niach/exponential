import { useCallback, useEffect, useMemo, useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import {
  ArrowLeft,
  ExternalLink,
  GitBranch,
  GitMerge,
  LoaderCircle,
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
import { mergeFailure, type MergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import { Button } from "@/components/ui/button"
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
  AddDelCounts,
  FileDiffList,
  type PullFile,
} from "@/components/diff-view"
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

type FilesState =
  | { kind: `loading` }
  | { kind: `files`; files: PullFile[] }
  | { kind: `none` } // no PR and the branch was never pushed (GitHub 404)
  | { kind: `error`; message: string }

// EXP-706: the file list is fetched by the ROUTE, not by the diff component —
// the header prints the file count and the +/- totals, so it needs the files
// before they are rendered. Two tiers behind one state machine: the PR diff
// (`issues.prFiles`) and, for a pushed branch with no PR yet,
// `repositories.branchDiff` (which answers null when nothing was ever pushed).
function useReviewFiles(issue: Issue | null) {
  const issueId = issue?.id ?? null
  const hasPr = issue?.prNumber != null
  const [state, setState] = useState<FilesState>({ kind: `loading` })

  const load = useCallback(() => {
    if (!issueId) return
    let cancelled = false
    setState({ kind: `loading` })
    const request: Promise<PullFile[] | null> = hasPr
      ? trpc.issues.prFiles.query({ issueId }).then((res) => res.files)
      : trpc.repositories.branchDiff
          .query({ issueId })
          .then((res) => res?.files ?? null)
    request
      .then((files) => {
        if (cancelled) return
        setState(files ? { kind: `files`, files } : { kind: `none` })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setState({
          kind: `error`,
          message: err instanceof Error ? err.message : `Failed to load changes`,
        })
      })
    return () => {
      cancelled = true
    }
  }, [issueId, hasPr])

  useEffect(() => load(), [load])

  return { state, reload: load }
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

  // The diff itself, hoisted so the header can caption it (EXP-706).
  const { state: filesState, reload: reloadFiles } = useReviewFiles(issue)
  const loadedFiles = filesState.kind === `files` ? filesState.files : null
  const totals = useMemo(() => {
    const list = loadedFiles ?? []
    return {
      additions: list.reduce((n, f) => n + f.additions, 0),
      deletions: list.reduce((n, f) => n + f.deletions, 0),
    }
  }, [loadedFiles])

  // Merge / close hold their spinner until the Electric echo flips prState away
  // from `open` (which hides the actions), matching the Reviews list.
  const [merging, setMerging] = useState(false)
  const [closing, setClosing] = useState(false)
  const [confirmMergeOpen, setConfirmMergeOpen] = useState(false)
  const [confirmCloseOpen, setConfirmCloseOpen] = useState(false)
  // A refused merge/close captions the action bar that produced it (EXP-323)
  // instead of only flashing a toast — the reason has to stay next to the
  // conflict-recovery button. WHICH action failed rides along: the recovery
  // run rebases, force-pushes and then MERGES the PR, so it may only be
  // offered after a failed MERGE — a user who asked to CLOSE a PR must never
  // be handed a button that merges it.
  const [actionError, setActionError] = useState<
    ({ action: `merge` | `close` } & MergeFailure) | null
  >(null)

  // A refusal describes ONE snapshot of the pull request, so it must not
  // outlive that snapshot: a re-synced issue row (Electric echo) or a fresh
  // file fetch drops it, and with it the "Fix conflicts" swap below. Without
  // this a conflict resolved OUTSIDE the recovery run (a teammate rebases and
  // pushes, GitHub recomputes mergeability) would hide Merge for the life of
  // the open PR. A refused merge writes nothing server-side, so the echo can
  // never race the failure that was just stored.
  const issueUpdatedAt = issue?.updatedAt
  useEffect(() => {
    setActionError(null)
  }, [issueUpdatedAt])
  const reloadReview = useCallback(() => {
    setActionError(null)
    reloadFiles()
  }, [reloadFiles])

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
  // Only a REAL conflict can be fixed by the recovery run (EXP-533), so only a
  // real conflict is worth polling for an online desktop.
  const remote = useRemoteStart({
    enabled: steerEnabled && actionError?.conflict === true,
    currentUserId,
    teamId: team?.id,
  })

  const confirmMerge = () => {
    if (!issue) return
    setConfirmMergeOpen(false)
    setMerging(true)
    setActionError(null)
    // Merge always closes the live coding sessions (EXP-498).
    trpc.issues.mergePr
      .mutate({ issueId: issue.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        setActionError({
          action: `merge`,
          ...mergeFailure(error, `The pull request could not be merged`),
        })
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
        setActionError({
          action: `close`,
          ...mergeFailure(error, `The pull request could not be closed`),
        })
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
  // EXP-706: a real merge conflict REPLACES the Merge control in its own slot
  // (desktop header and mobile bar alike) instead of adding a second button
  // next to the refusal caption — one action per slot, on every client.
  const canFixConflicts = Boolean(
    actionError?.action === `merge` &&
      actionError.conflict &&
      isOpen &&
      issue.branch &&
      steerEnabled
  )
  const prStateLabel =
    issue.prNumber == null ? `No pull request` : (issue.prState ?? `open`)

  const mergeControl = canFixConflicts ? (
    <Button className="rounded-full" onClick={() => setFixOpen(true)}>
      <GitBranch className="size-3.5" />
      Fix conflicts
    </Button>
  ) : (
    <Button
      className="rounded-full"
      disabled={merging || closing}
      onClick={() => setConfirmMergeOpen(true)}
    >
      {merging ? (
        <>
          <LoaderCircle className="size-3.5 animate-spin" />
          Merging…
        </>
      ) : (
        <>
          <GitMerge className="size-3.5" />
          Merge
        </>
      )}
    </Button>
  )

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Mobile header (EXP-706) — a round back button and the section title,
          no actions (those live in the floating bar). A plain flex sibling
          ABOVE the scroller, not a sticky child of it: the route's own column
          already pins it, and the scrollport stays free of overlay chrome. */}
      <div className="flex items-center gap-2 border-b border-border bg-background/80 px-3 py-2 backdrop-blur-xl md:hidden">
        <Button
          variant="ghost"
          size="icon"
          className="size-9 shrink-0 rounded-full border border-glass-stroke-card bg-popover/85 text-muted-foreground backdrop-blur-xl hover:bg-muted/85 hover:text-foreground"
          aria-label="Back to reviews"
          onClick={() =>
            void navigate({ to: `/t/$teamSlug/reviews`, params: { teamSlug } })
          }
        >
          <ArrowLeft className="size-4" />
        </Button>
        <span className="flex-1 truncate text-center text-sm font-semibold">
          Review
        </span>
        {/* Balances the back button so the title stays optically centred. */}
        <span className="size-9 shrink-0" />
      </div>

      {/* Breadcrumb (desktop only — mobile has the back button above) */}
      <div className="hidden items-center gap-1.5 border-b border-border px-4 py-2 text-xs text-muted-foreground md:flex">
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

      {/* Desktop header (EXP-706) — deliberately NOT a card: the branch over a
          quiet state · files · ±totals line, with the actions on the right.
          The PR number is gone from this page entirely; GitHub is one round
          glass button away. */}
      <div className="hidden min-w-0 items-center gap-2 border-b border-border px-4 py-2 md:flex">
        <div className="min-w-0">
          {issue.branch && (
            <div className="truncate font-mono text-xs text-muted-foreground">
              {issue.branch}
            </div>
          )}
          <div className="flex items-center gap-3 text-xs">
            <span className="capitalize text-muted-foreground">
              {prStateLabel}
            </span>
            {loadedFiles && (
              <>
                <span className="text-muted-foreground">
                  {loadedFiles.length === 1
                    ? `1 file`
                    : `${loadedFiles.length} files`}
                </span>
                <AddDelCounts
                  additions={totals.additions}
                  deletions={totals.deletions}
                />
              </>
            )}
          </div>
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-2">
          {isOpen && (
            <Button
              variant="glass"
              size="icon-sm"
              aria-label="Close pull request without merging"
              title="Close PR without merging"
              disabled={merging || closing}
              onClick={() => setConfirmCloseOpen(true)}
            >
              {closing ? (
                <LoaderCircle className="size-4 animate-spin" />
              ) : (
                <X className="size-4" />
              )}
            </Button>
          )}
          {isOpen && mergeControl}
          {issue.prUrl && (
            <Button
              variant="glass"
              size="icon-sm"
              aria-label="Open pull request on GitHub"
              title="Open PR on GitHub"
              onClick={() =>
                window.open(issue.prUrl ?? ``, `_blank`, `noopener,noreferrer`)
              }
            >
              <ExternalLink className="size-4" />
            </Button>
          )}
        </div>
      </div>

      {/* Desktop refusal caption (EXP-333) — right under the header actions
          that produced it. EXP-706: message only; the recovery run has taken
          the Merge button's slot above — except that the swap must never be a
          dead end, so Merge rides the caption as a quiet secondary while it
          holds that slot. */}
      {actionError && (
        <div className="hidden flex-wrap items-center gap-2 border-b border-border px-4 py-2 md:flex">
          <span className="text-destructive text-xs">{actionError.message}</span>
          {canFixConflicts && (
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              disabled={merging || closing}
              onClick={() => setConfirmMergeOpen(true)}
            >
              <GitMerge className="size-3.5" />
              Retry merge
            </Button>
          )}
        </div>
      )}

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

      {/* Diff body — bottom padding clears the mobile floating action bar */}
      <div className="mx-auto w-full max-w-5xl flex-1 overflow-y-auto pb-24 md:pb-4">
        {filesState.kind === `loading` ? (
          <div className="flex items-center gap-2 px-4 py-3 text-xs text-muted-foreground">
            <LoaderCircle className="size-3.5 animate-spin" /> Loading changes…
          </div>
        ) : filesState.kind === `error` ? (
          <div className="flex flex-wrap items-center gap-2 px-4 py-3 text-xs text-rose-300">
            {`Couldn’t load changes: ${filesState.message}`}
            <Button
              variant="ghost"
              size="xs"
              className="text-muted-foreground"
              onClick={() => reloadReview()}
            >
              <RotateCw className="size-3.5" />
              Retry
            </Button>
          </div>
        ) : filesState.kind === `files` && filesState.files.length > 0 ? (
          <FileDiffList
            files={filesState.files}
            showFileNav={false}
            defaultCollapsed
          />
        ) : (
          <div className="px-4 py-6 text-xs text-muted-foreground">
            No changes yet. A pushed branch or pull request will appear here.
          </div>
        )}
      </div>

      {/* Floating action bar (EXP-248) — dismiss · Merge · GitHub, matching the
          mobile clients' review-detail bar and the app's glass-pill chrome.
          Mobile-only since EXP-333 — desktop gets the inline header actions
          above, like the IDE. */}
      {(isOpen || issue.prUrl) && (
        <div className="pointer-events-none fixed inset-x-0 bottom-0 z-40 flex flex-col items-center gap-2 px-4 pb-[max(1.25rem,env(safe-area-inset-bottom))] md:hidden">
          {actionError && (
            <div className="pointer-events-auto flex max-w-lg flex-wrap items-center justify-center gap-2 rounded-lg border border-glass-stroke-card bg-popover/85 px-3 py-2 shadow-lg shadow-black/40 backdrop-blur-xl">
              {/* EXP-706: message only — the recovery run (merge failures
                  only, and only REAL conflicts: EXP-533) has replaced the
                  Merge pill below instead of doubling up here. Merge is still
                  reachable from the caption while that swap stands: the
                  conflict may have been resolved outside the recovery run. */}
              <span className="text-destructive text-xs">
                {actionError.message}
              </span>
              {canFixConflicts && (
                <Button
                  variant="ghost"
                  size="xs"
                  className="text-muted-foreground"
                  disabled={merging || closing}
                  onClick={() => setConfirmMergeOpen(true)}
                >
                  <GitMerge className="size-3.5" />
                  Retry merge
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
                  <LoaderCircle className="size-4 animate-spin" />
                ) : (
                  <X className="size-4" />
                )}
              </Button>
            )}
            {/* EXP-706: same slot, same pill — a real conflict swaps Merge
                for the recovery run rather than adding a button. */}
            {isOpen &&
              (canFixConflicts ? (
                <Button
                  className="pointer-events-auto h-12 rounded-full px-6 shadow-lg shadow-black/40"
                  onClick={() => setFixOpen(true)}
                >
                  <GitBranch className="size-4" />
                  Fix conflicts
                </Button>
              ) : (
                <Button
                  className="pointer-events-auto h-12 rounded-full px-6 shadow-lg shadow-black/40"
                  disabled={merging || closing}
                  onClick={() => setConfirmMergeOpen(true)}
                >
                  {merging ? (
                    <>
                      <LoaderCircle className="size-4 animate-spin" />
                      Merging…
                    </>
                  ) : (
                    <>
                      <GitMerge className="size-4" />
                      Merge
                    </>
                  )}
                </Button>
              ))}
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
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>
              {isBatch
                ? `Merge PR #${issue.prNumber}?`
                : `Merge ${issue.identifier}?`}
            </DialogTitle>
            <DialogDescription>
              {`Squash-merges pull request #${issue.prNumber}${issue.branch ? ` (${issue.branch})` : ``} into the repository's default branch via the GitHub App. Any live coding session for it closes.`}
              {isBatch
                ? ` Completes all ${linked.length} linked issues: ${linked
                    .map((i) => i.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel onClick={() => setConfirmMergeOpen(false)} />
            <Button onClick={confirmMerge}>Merge pull request</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={confirmCloseOpen} onOpenChange={setConfirmCloseOpen}>
        <DialogContent mobile="alert">
          <DialogHeader>
            <DialogTitle>
              {isBatch
                ? `Close PR #${issue.prNumber}?`
                : `Close ${issue.identifier}'s pull request?`}
            </DialogTitle>
            <DialogDescription>
              {`Closes pull request #${issue.prNumber}${issue.branch ? ` (${issue.branch})` : ``} on GitHub WITHOUT merging. Use this when the issue was dropped even though the work exists. The branch is kept; the PR can be reopened on GitHub.`}
              {isBatch
                ? ` The PR is linked to ${linked.length} issues: ${linked
                    .map((i) => i.identifier)
                    .join(`, `)}.`
                : ``}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel onClick={() => setConfirmCloseOpen(false)} />
            <Button variant="destructive" onClick={confirmClose}>
              Close pull request
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
