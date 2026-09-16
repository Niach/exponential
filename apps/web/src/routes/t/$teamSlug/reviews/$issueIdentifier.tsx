import { useCallback, useEffect, useMemo, useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import type { Issue } from "@/db/schema"
import { issueCollection } from "@/lib/collections"
import { useTeamBySlug, useTeamBoards } from "@/hooks/use-team-data"
import { useReviewFiles } from "@/hooks/use-review-files"
import {
  HEADER_BUTTON_CLASS,
  MobileDetailHeader,
} from "@/components/team/mobile-detail-header"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { mergeFailure } from "@/lib/merge-failure"
import { trpc } from "@/lib/trpc-client"
import {
  conceptIcon,
  Button,
  Pill,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@exp/ui"
import { cn } from "@/lib/utils"
import { ChangesFileSheet } from "@/components/changes-file-sheet"
import { ChangesTopBar } from "@/components/changes-top-bar"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import { ChangesView } from "@/components/changes-view"
import { GithubCircle, MergeCapsule } from "@/components/issue-changes-face"
import {
  MOBILE_WORK_BAR_CLEARANCE,
  MobileWorkBar,
} from "@/components/mobile-work-bar"
import { useSteerConfig } from "@/components/agent-session"
import { pageTitle, usePageTitle } from "@/lib/page-title"

// Review-detail (EXP-106): the PR/branch diff for one review, with Merge/Close
// actions moved off the issue detail. The representative issue carries the PR;
// merging/closing acts on the ONE PR, and the server completes every linked
// issue (a batch run's issues all share one prUrl).
//
// EXP-895: the page is now a `ChangesTopBar` over a `ChangesView` — the exact
// pair the run's Changes face draws. Merge is `SessionMergePill` (one control,
// with its own confirm and the Fix-conflicts swap), so the route keeps only the
// CLOSE mutation; on a phone the bar is the shared `MobileWorkBar` (file sheet ·
// Merge capsule · GitHub) and Close moves into the header's `…`.
export const Route = createFileRoute(
  `/t/$teamSlug/reviews/$issueIdentifier`
)({
  head: ({ params }) => ({
    meta: [{ title: pageTitle(params.issueIdentifier, `Reviews`) }],
  }),
  // EXP-851: the list this review was opened from (`lib/detail-origin.ts`) —
  // the sidebar keeps the queue beside it; absent means the main menu stays.
  validateSearch: (search: Record<string, unknown>): { from?: string } => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
  }),
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

const UiLoadingIcon = conceptIcon(`ui-loading`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiRefreshIcon = conceptIcon(`ui-refresh`)

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
  usePageTitle(
    issue
      ? pageTitle(`${issue.identifier} ${issue.title}`, `Reviews`)
      : undefined
  )

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

  // The diff itself, hoisted so the top bar can caption it (EXP-706) — the
  // SHARED hook since EXP-895, the same one the phone's Changes face uses.
  const { state: filesState, reload: reloadFiles } = useReviewFiles(issue)
  const files = filesState.kind === `files` ? filesState.files : []
  const [selected, setSelected] = useState<string | null>(null)

  // Close holds its spinner until the Electric echo flips prState away from
  // `open` (which hides the action), matching the Reviews list. Merge's own
  // spinner lives inside `SessionMergePill`.
  const [closing, setClosing] = useState(false)
  const [confirmCloseOpen, setConfirmCloseOpen] = useState(false)
  const [closeError, setCloseError] = useState<string | null>(null)

  // A refusal describes ONE snapshot of the pull request, so it must not outlive
  // that snapshot: a re-synced issue row (Electric echo) drops it.
  const issueUpdatedAt = issue?.updatedAt
  useEffect(() => {
    setCloseError(null)
  }, [issueUpdatedAt])
  const reloadReview = useCallback(() => {
    setCloseError(null)
    reloadFiles()
  }, [reloadFiles])

  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  const confirmClose = () => {
    if (!issue) return
    setConfirmCloseOpen(false)
    setClosing(true)
    setCloseError(null)
    trpc.issues.closePr
      .mutate({ issueId: issue.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        setCloseError(
          mergeFailure(error, `The pull request could not be closed`).message
        )
        setClosing(false)
      })
  }

  // EXP-897: merge the WHOLE stack, from its top — the server merges every
  // unmerged member below it. A refusal captions the bar, like a refused close.
  const mergeStack = (topIssueId: string) => {
    setCloseError(null)
    trpc.issues.mergePr
      .mutate(
        { issueId: topIssueId, mergeStack: true },
        { context: { skipErrorToast: true } }
      )
      .catch((error: unknown) => {
        setCloseError(
          mergeFailure(error, `The stack could not be merged`).message
        )
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
  const mergeTarget = {
    issueId: issue.id,
    prState: issue.prState,
    prNumber: issue.prNumber,
    branch: issue.branch,
    updatedAt: issue.updatedAt,
    steerEnabled,
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* EXP-851: the shared detail header on phones — round back, the
          identifier centred. EXP-895: Close PR lives in its `…` now; the bottom
          bar is the shared three-slot work bar, which has no room for a fourth
          control. */}
      <MobileDetailHeader
        className="md:hidden"
        title={<span className="font-mono">{issue.identifier}</span>}
        backLabel="Back to reviews"
        onBack={() =>
          void navigate({ to: `/t/$teamSlug/reviews`, params: { teamSlug } })
        }
        menu={
          isOpen ? (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className={HEADER_BUTTON_CLASS}
                  aria-label="Review actions"
                  data-testid="review-actions-menu"
                >
                  <UiMoreIcon className="size-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem
                  disabled={closing}
                  onSelect={() => setConfirmCloseOpen(true)}
                >
                  Close PR without merging
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          ) : undefined
        }
      />

      {/* The md+ header: the ONE Changes bar (EXP-895) — it owns the merge
          control, Close and the GitHub link. */}
      <ChangesTopBar
        files={files}
        branch={issue.branch}
        prState={issue.prNumber == null ? null : (issue.prState ?? `open`)}
        prUrl={issue.prUrl}
        merge={isOpen ? mergeTarget : null}
        onClosePr={() => setConfirmCloseOpen(true)}
        closing={closing}
        trailing={
          /* EXP-897: the review page IS the Changes face — the same stack /
             batch pill the run's header wears, with `Merge stack` on the
             bottom entry of the stack. */
          <PrGraphBadge
            teamId={issue.teamId}
            teamSlug={teamSlug}
            face="changes"
            issue={issue}
            onMergeStack={mergeStack}
          />
        }
      />

      {/* A refused CLOSE captions the bar that produced it (EXP-333). A refused
          MERGE captions itself, inside `SessionMergePill`. */}
      {closeError && (
        <div className="flex flex-wrap items-center gap-2 border-b border-border px-4 py-2">
          <span className="text-destructive text-xs">{closeError}</span>
        </div>
      )}

      {/* Linked-issue chips (batch PRs) */}
      {isBatch && (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-border px-4 py-2">
          <span className="text-xs text-muted-foreground">
            {linked.length} linked issues
          </span>
          {linked.map((linkedIssue) => (
            <Pill
              key={linkedIssue.id}
              mode="action"
              className="font-mono"
              onClick={() => openIssue(linkedIssue)}
            >
              #{linkedIssue.identifier}
            </Pill>
          ))}
        </div>
      )}

      {/* EXP-771: the scroller is the FULL-width flex child; the max-w column
          is its child. The scrollbar then rides the panel's right edge instead
          of appearing mid-page beside the centred diff. EXP-895: the phone's
          clearance is the shared work-bar constant — no measured `--reviewbar-h`
          any more, because the bar is the standard 52px one. */}
      <div className="flex-1 overflow-x-auto overflow-y-auto">
        <div
          className={cn(`mx-auto w-full max-w-5xl`, MOBILE_WORK_BAR_CLEARANCE, `md:pb-4`)}
        >
          {filesState.kind === `loading` ? (
            <div className="flex items-center gap-2 px-4 py-3 text-xs text-muted-foreground">
              <UiLoadingIcon className="size-3.5 animate-spin" /> Loading
              changes…
            </div>
          ) : filesState.kind === `error` ? (
            <div className="flex flex-wrap items-center gap-2 px-4 py-3 text-xs text-destructive">
              {`Couldn’t load changes: ${filesState.message}`}
              <Pill mode="action" onClick={() => reloadReview()}>
                <UiRefreshIcon className="size-3" />
                Retry
              </Pill>
            </div>
          ) : files.length > 0 ? (
            <ChangesView
              files={files}
              nav="auto"
              defaultCollapsed
              selected={selected}
              onSelect={setSelected}
            />
          ) : (
            <div className="px-4 py-6 text-xs text-muted-foreground">
              No changes yet. A pushed branch or pull request will appear here.
            </div>
          )}
        </div>
      </div>

      {/* The phone bar (EXP-895): the same three-slot work bar every Work-screen
          face wears — the file sheet, the Merge capsule, GitHub. */}
      {(files.length > 0 || isOpen || issue.prUrl) && (
        <MobileWorkBar
          leading={
            files.length > 0 ? (
              <ChangesFileSheet
                files={files}
                selected={selected}
                onSelect={setSelected}
              />
            ) : undefined
          }
          capsule={isOpen ? <MergeCapsule {...mergeTarget} /> : undefined}
          trailing={
            issue.prUrl ? <GithubCircle prUrl={issue.prUrl} /> : undefined
          }
        />
      )}

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
