import { useEffect, useMemo, useRef, useState } from "react"
import {
  mergeExternalPrPrompt,
  mergeIssuePrPrompt,
  mergeRunPrPrompt,
  promptActions,
} from "@/lib/prompts"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import type { OpenPull } from "@/lib/integrations/github-pr"
import {
  conceptIcon,
  EmptyState,
  Pill,
  Prompt,
  ListRow,
  GlassSectionHeader,
  BoardGlyph,
} from "@exp/ui"
import { useSteerConfig } from "@/components/agent-session"
import { useOpenComposerInTeam } from "@/hooks/use-open-composer"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import {
  useReviewsData,
  type ReviewEntry,
  type SessionReviewEntry,
} from "@/hooks/use-reviews-data"
import { ReviewPrRow } from "@/components/review-pr-row"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { BUILTIN_FIX_CONFLICTS_ID } from "@/lib/builtin-actions"
import { mergeFailure, type MergeFailure } from "@/lib/merge-failure"
import { stackMergeChoice, type StackMergeChoice } from "@/lib/pr-stack"
import {
  StackMergeChoiceDialog,
  type StackMergeInput,
} from "@/components/stack-merge-choice-dialog"
import { trpc } from "@/lib/trpc-client"
import { pageTitle } from "@/lib/page-title"
import { sessionIdentity } from "@/lib/session-identity"
import { REPO_BAND_CAPTION, RUN_BAND_CAPTION } from "@/lib/reviews-queue"

// Cross-board review queue: every issue in the team with an open PR,
// grouped by board, with a one-click (confirmed) squash-merge that goes
// through the GitHub App server-side. Deliberately filter-free — the queue
// should be short. Open PRs WITHOUT any link (manual PRs, external
// contributors) are listed last, grouped by repository, straight from GitHub.
export const Route = createFileRoute(`/t/$teamSlug/reviews/`)({
  head: () => ({ meta: [{ title: pageTitle(`Reviews`) }] }),
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

// EXP-916: Reviews is a ×4 surface, so its glyphs are CONCEPTS — the natives
// and the desktop read the same names out of `packages/icons/icons.json`.
const PrOpenIcon = conceptIcon(`pr-open`)
const PrMergedIcon = conceptIcon(`pr-merged`)
const UiLoadingIcon = conceptIcon(`ui-loading`)
const BatchIcon = conceptIcon(`pr-batch`)

/**
 * The rows one stack-dialog merge walks: the chain the choice already named
 * (`choice.members`, bottom first, one label per pull request), through the
 * member the call names. "Merge stack" takes the top, so the whole chain;
 * "Merge this pull request" on a member above the bottom lands it and
 * everything below. A plain merge walks nothing. Exported for its test.
 */
export function stackMergeEntries(
  choice: StackMergeChoice,
  input: StackMergeInput,
  entries: readonly ReviewEntry[]
): ReviewEntry[] {
  if (!input.mergeStack) return []
  const through =
    input.issueId === choice.topIssueId ? choice.members.length : choice.position
  // A label is the identifier, or `EXP-874 +2` for a batch pull request.
  const named = new Set(
    choice.members.slice(0, through).map((label) => label.split(` `)[0])
  )
  return entries.filter((entry) =>
    entry.issues.some((issue) => named.has(issue.identifier))
  )
}

interface ExternalMergeTarget {
  repositoryId: string
  fullName: string
  pull: OpenPull
}

function ReviewsPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  // EXP-1186: the phone's Reviews reads EVERY member team, like the inbox;
  // with more than one, each band names its team. md+ = the active team.
  const scope = useCrossTeamScope(team)
  const {
    groups,
    sessionEntries,
    sessionGroups,
    externalGroups,
    count,
    isLoading,
    externalLoading,
    removeExternalPull,
    openIssues,
  } = useReviewsData(team, scope.teams)
  const teamById = useMemo(
    () => new Map(scope.teams.map((row) => [row.id, row])),
    [scope.teams]
  )
  // A band's quiet team caption — only while the list spans several teams.
  const teamCaption = (teamId: string | undefined) => {
    const name = scope.grouped && teamId ? teamById.get(teamId)?.name : undefined
    return name ? (
      <span className="truncate text-xs text-muted-foreground">{name}</span>
    ) : undefined
  }

  // The entry whose confirm dialog is open, and the entries with an in-flight
  // merge (keyed by entry.key). A successful merge keeps its spinner until the
  // Electric echo flips prState and the entry leaves the list; external PRs
  // have no echo and are removed locally on success.
  // Closing without merging lives in the issue's actions menu (EXP-248,
  // EXP-1154) — list rows offer merge only, matching the iOS/Android rows.
  const [mergeTarget, setMergeTarget] = useState<ReviewEntry | null>(null)
  // EXP-1145: a row whose PR is a member of an open stack asks first (the
  // list itself stays FLAT). Holds the entry and what the dialog says.
  const [stackTarget, setStackTarget] = useState<{
    entry: ReviewEntry
    choice: StackMergeChoice
  } | null>(null)
  const [mergingIds, setMergingIds] = useState<Set<string>>(new Set())
  const [externalMergeTarget, setExternalMergeTarget] =
    useState<ExternalMergeTarget | null>(null)
  // EXP-734: the run PR whose confirm dialog is open. Its spinner is held by
  // the same `mergingIds` set, keyed on the entry key, and released by the
  // Electric echo (the session row's prState leaves `open`).
  const [sessionMergeTarget, setSessionMergeTarget] =
    useState<SessionReviewEntry | null>(null)
  // A refused merge (conflicts, branch protection, GitHub App errors) captions
  // ITS row, keyed by entry.key (EXP-323) — the global toast is transient and
  // gave the conflict-recovery run nowhere to live.
  const [mergeErrors, setMergeErrors] = useState<
    Record<string, MergeFailure>
  >({})

  // A refusal describes ONE snapshot of the PR. Every entry stamps the issue
  // row its caption was taken from; when Electric echoes a newer `updatedAt`
  // for that row the caption is stale, so it clears itself.
  const stamps = useMemo(() => {
    const map: Record<string, string> = {}
    for (const group of groups) {
      for (const entry of group.entries) {
        map[entry.key] = String(entry.issue.updatedAt ?? ``)
      }
    }
    // Run PRs stamp their own session row (EXP-734).
    for (const entry of sessionEntries) {
      map[entry.key] = String(entry.session.updatedAt ?? ``)
    }
    return map
  }, [groups, sessionEntries])
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

  // The conflict recovery run (EXP-323, desktop parity): the composer opened
  // on the builtin action with THIS pull request already picked. EXP-1233:
  // a merge refused by a REAL conflict opens it at once — no "Fix conflicts"
  // button in the row's slot, no caption — in the PR's OWN team (EXP-1186).
  // Only where a run can start: a member, with the relay configured.
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const openComposerInTeam = useOpenComposerInTeam()
  const openFixConflicts = (entry: ReviewEntry, rowTeamSlug: string) =>
    openComposerInTeam(rowTeamSlug, {
      actionId: BUILTIN_FIX_CONFLICTS_ID,
      prIssueId: entry.issue.id,
      conflict: true,
    })
  // EXP-1154: the row opens the ISSUE on its Changes face (the PR's diff,
  // Merge in the tray, Close PR in the `…`); the review-detail page is gone.
  // A batch entry's representative issue stands for the PR.
  // EXP-1186: under the row's OWN team's slug (the inbox rule).
  const openReview = (
    rowTeamSlug: string,
    boardSlug: string,
    issueIdentifier: string,
  ) => {
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: { teamSlug: rowTeamSlug, boardSlug, issueIdentifier },
      // EXP-851: the queue stays the Back target beside the review.
      search: { from: `reviews`, view: `diff` },
    })
  }

  // EXP-1194: a run's own PR opens the same diff UI, on the RUN's Changes
  // face (`codingSessions.prFiles`) — there is no issue to open. GitHub stays
  // one click away in the face's header.
  const openRunReview = (rowTeamSlug: string, sessionId: string) => {
    void navigate({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug: rowTeamSlug, sessionId },
      search: { from: `reviews`, view: `diff` },
    })
  }

  // A row's Merge: a stack member opens the stack dialog, anything else the
  // plain confirm. The rows are already the team's
  // open pull requests, exactly what the chain is read from.
  const askMerge = (entry: ReviewEntry) => {
    const choice = stackMergeChoice(entry.issue, openIssues)
    if (choice) setStackTarget({ entry, choice })
    else setMergeTarget(entry)
  }

  const confirmMerge = () => {
    const entry = mergeTarget
    if (!entry) return
    setMergeTarget(null)
    runMerge(entry, { issueId: entry.issue.id })
  }

  // EXP-1145: the stack dialog's two merges. A stack merge lands several pull
  // requests; its refusal captions the row with the server's message but
  // never opens the recovery run (that run rebases ONE pull request).
  const confirmStackMerge = (input: StackMergeInput) => {
    const target = stackTarget
    if (!target) return
    setStackTarget(null)
    runMerge(target.entry, input, target.choice)
  }

  function runMerge(
    entry: ReviewEntry,
    input: StackMergeInput,
    choice?: StackMergeChoice
  ) {
    // A stack merge lands several rows while the server walks the chain:
    // every one of them spins, not only the pressed row, so none keeps a
    // live Merge pill meanwhile.
    const walked = choice
      ? stackMergeEntries(
          choice,
          input,
          groups.flatMap((group) => group.entries)
        )
      : []
    const spinning = [entry, ...walked.filter((row) => row.key !== entry.key)]
    const release = (rows: readonly ReviewEntry[]) =>
      setMergingIds((prev) => {
        const next = new Set(prev)
        for (const row of rows) next.delete(row.key)
        return next
      })
    setMergingIds((prev) => {
      const next = new Set(prev)
      for (const row of spinning) next.add(row.key)
      return next
    })
    setMergeErrors((prev) => {
      const next = { ...prev }
      delete next[entry.key]
      return next
    })
    // Merging through the representative issue merges the ONE PR — the server
    // then completes every linked issue and ends its live coding sessions
    // (EXP-498).
    trpc.issues.mergePr
      .mutate(input, { context: { skipErrorToast: true } })
      .then((result) => {
        // A stack merge can settle with members still open (a queued merge
        // holds everything above it). The rows it landed keep their spinner
        // until the echo removes them, like any merge; the rest are released.
        if (!input.mergeStack) return
        const landed = new Set((result.stack ?? []).map((pr) => pr.identifier))
        release(
          spinning.filter(
            (row) => !row.issues.some((issue) => landed.has(issue.identifier))
          )
        )
      })
      .catch((error: unknown) => {
        const failure = input.mergeStack
          ? {
              ...mergeFailure(error, `The stack could not be merged`),
              conflict: false,
            }
          : mergeFailure(error, `The pull request could not be merged`)
        release(spinning)
        // EXP-1233: a REAL conflict (EXP-533) on a PR with a recorded branch
        // (the run rebases it) opens the recovery run's composer at once —
        // the composer says why. A stale base, a branch-protection refusal
        // or an unreachable server fail the merge too, and a
        // rebase-and-resolve run fixes none of them: those caption the row
        // (GitHub's verbatim reason stays readable, and the spinner is
        // unstuck for a retry).
        if (failure.conflict && entry.issue.branch && steerEnabled) {
          const rowTeam = groups.find((group) =>
            group.entries.some((row) => row.key === entry.key)
          )?.team
          openFixConflicts(entry, rowTeam?.slug ?? team?.slug ?? teamSlug)
          return
        }
        setMergeErrors((prev) => ({ ...prev, [entry.key]: failure }))
      })
  }

  // EXP-734: merging a run's OWN pull request. No issue completes; the run's
  // session closes unless the team keeps sessions on merge.
  const confirmSessionMerge = () => {
    const entry = sessionMergeTarget
    if (!entry) return
    setSessionMergeTarget(null)
    setMergingIds((prev) => new Set(prev).add(entry.key))
    setMergeErrors((prev) => {
      const next = { ...prev }
      delete next[entry.key]
      return next
    })
    trpc.codingSessions.mergePr
      .mutate(
        { sessionId: entry.session.id },
        { context: { skipErrorToast: true } }
      )
      .catch((error: unknown) => {
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
      .then((result) => {
        // EXP-1165: a queued merge has not landed; the row stays until the
        // pull request actually closes.
        if (result.merged) removeExternalPull(target.repositoryId, target.pull.number)
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

  const mergeCopy = mergeIssuePrPrompt({
    number: mergeTarget?.issue.prNumber,
    count: mergeTarget?.issues.length ?? 1,
  })
  const sessionMergeCopy = mergeRunPrPrompt(
    sessionMergeTarget?.session.prNumber
  )
  const externalMergeCopy = mergeExternalPrPrompt(
    externalMergeTarget?.fullName ?? ``,
    externalMergeTarget?.pull.number ?? 0,
    externalMergeTarget?.pull.baseBranch ?? ``
  )

  return (
    // EXP-771: the SCROLLER is full width, the reading column lives inside it
    // — so the scrollbar rides the panel's right edge instead of appearing
    // mid-page beside a centred list (the actions.tsx pattern). The dialogs
    // below are portalled, so sitting in the scrollport costs them nothing.
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 ${TAB_BAR_CLEARANCE}`}
      >
        {isLoading ? (
          <div className="text-muted-foreground px-1 py-6 text-sm">Loading…</div>
        ) : count === 0 ? (
          externalLoading ? (
            <div className="text-muted-foreground px-1 py-6 text-sm">
              Loading…
            </div>
          ) : (
            <EmptyState
              icon={PrOpenIcon}
              title="No open pull requests"
              description={
                scope.teams.length > 1
                  ? `Open pull requests in your teams' repositories land here for review.`
                  : `Open pull requests in this team's repositories land here for review.`
              }
            />
          )
        ) : (
          <>
            {groups.map((group) => {
              const rowTeam = group.team ?? team
              return (
              <div key={group.board.id} className="mb-6">
                <GlassSectionHeader
                  leading={
                    <BoardGlyph board={group.board} className="size-3.5" />
                  }
                  label={group.board.name}
                  trailing={teamCaption(group.board.teamId)}
                />

                <div className="flex flex-col gap-0">
                  {group.entries.map((entry) => {
                    const issue = entry.issue
                    const isBatch = entry.issues.length > 1
                    const merging = mergingIds.has(entry.key)
                    const mergeError = mergeErrors[entry.key]
                    return (
                      <ReviewPrRow
                        key={entry.key}
                        issue={issue}
                        issues={entry.issues}
                        onOpen={() =>
                          openReview(
                            rowTeam.slug,
                            group.board.slug,
                            issue.identifier,
                          )
                        }
                        /* A PR linking several issues wears the batch glyph;
                            the overlay on it lists the issues it closes
                            (EXP-897 Part 4). EXP-916: the lead cell is ALWAYS
                            drawn: a badge that renders nothing (its siblings
                            have not synced) falls back to the plain glyph. */
                        lead={
                          isBatch ? (
                            <PrGraphBadge
                              teamId={rowTeam.id}
                              teamSlug={rowTeam.slug}
                              issue={issue}
                              variant="glyph"
                              fallback={
                                <BatchIcon className="size-4 text-muted-foreground" />
                              }
                            />
                          ) : (
                            <PrOpenIcon className="h-4 w-4 text-emerald-500" />
                          )
                        }
                        /* EXP-698: the row's Merge and the review detail's
                            header Merge are ONE control at ONE weight —
                            `Pill size="md" mode="action"`. EXP-1233: a real
                            conflict opens the recovery composer instead of
                            swapping this slot. */
                        trailing={
                          <Pill
                            size="md"
                            mode="action"
                            disabled={merging}
                            onClick={(e) => {
                              e.stopPropagation()
                              askMerge(entry)
                            }}
                          >
                            {merging ? (
                              <>
                                <UiLoadingIcon className="h-3.5 w-3.5 animate-spin" />
                                Merging…
                              </>
                            ) : (
                              <>
                                <PrMergedIcon className="h-3.5 w-3.5" />
                                Merge
                              </>
                            )}
                          </Pill>
                        }
                        /* A non-conflict refusal captions its own row
                            (EXP-323) — spanning the grid so the full GitHub
                            message stays readable. */
                        footer={
                          mergeError && (
                            <div className="col-span-4 flex flex-wrap items-center gap-2 pt-2">
                              <span className="text-destructive text-xs">
                                {mergeError.message}
                              </span>
                            </div>
                          )
                        }
                      />
                    )
                  })}
                </div>
              </div>
              )
            })}

            {/* EXP-734: pull requests a coding run opened for itself — an
                action or chat run with no linked issue. They merge through
                the run, not an issue, so they group on their own. */}
            {/* EXP-1186: one band per team (a single team = one band). */}
            {sessionEntries.length > 0 && sessionGroups.map((sessionGroup) => (
              <div key={sessionGroup.team?.id ?? `runs`} className="mb-6">
                <GlassSectionHeader
                  leading={
                    <PrOpenIcon className="h-2.5 w-2.5 shrink-0 text-foreground/50" />
                  }
                  label="Agent runs"
                  trailing={
                    teamCaption(sessionGroup.team?.id) ?? (
                      <span className="text-xs text-foreground/50">
                        {RUN_BAND_CAPTION}
                      </span>
                    )
                  }
                />

                <div className="flex flex-col gap-0">
                  {sessionGroup.entries.map((entry) => {
                    const session = entry.session
                    const merging = mergingIds.has(entry.key)
                    const mergeError = mergeErrors[entry.key]
                    return (
                      <ListRow
                        key={entry.key}
                        interactive
                        className="group/row grid grid-cols-[1.5rem_4.5rem_1fr_auto] gap-0"
                        onClick={() =>
                          openRunReview(
                            sessionGroup.team?.slug ?? teamSlug,
                            session.id
                          )
                        }
                        data-testid={`review-run-${session.id}`}
                      >
                        <PrOpenIcon className="h-4 w-4 text-emerald-500" />
                        <span className="truncate font-mono text-xs text-muted-foreground">
                          #{session.prNumber}
                        </span>
                        <div className="min-w-0 pr-3">
                          <div className="truncate text-sm">
                            {/* EXP-1204: the run's own name, ×4 with the
                                natives' rows — a chat's agent-named subject,
                                an action's name snapshot. */}
                            {sessionIdentity({ session, issue: undefined }).subject}
                          </div>
                          {session.branch && (
                            <div className="truncate font-mono text-xs text-muted-foreground">
                              {session.branch}
                            </div>
                          )}
                        </div>
                        {/* No recovery run here: the builtin takes a
                            representative ISSUE, and a run PR has none. */}
                        <Pill
                          size="md"
                          mode="action"
                          disabled={merging}
                          onClick={(e) => {
                            e.stopPropagation()
                            setSessionMergeTarget(entry)
                          }}
                        >
                          {merging ? (
                            <>
                              <UiLoadingIcon className="h-3.5 w-3.5 animate-spin" />
                              Merging…
                            </>
                          ) : (
                            <>
                              <PrMergedIcon className="h-3.5 w-3.5" />
                              Merge
                            </>
                          )}
                        </Pill>
                        {mergeError && (
                          <div className="col-span-4 pt-2">
                            <span className="text-destructive text-xs">
                              {mergeError.message}
                            </span>
                          </div>
                        )}
                      </ListRow>
                    )
                  })}
                </div>
              </div>
            ))}

            {externalGroups.map((group) => (
              <div key={`${group.teamId}:${group.repositoryId}`} className="mb-6">
                <GlassSectionHeader
                  leading={
                    <PrOpenIcon className="h-2.5 w-2.5 shrink-0 text-foreground/50" />
                  }
                  label={group.fullName}
                  trailing={
                    teamCaption(group.teamId) ?? (
                      <span className="text-xs text-foreground/50">{REPO_BAND_CAPTION}</span>
                    )
                  }
                />

                <div className="flex flex-col gap-0">
                  {group.pulls.map((pull) => {
                    const key = externalPullKey(group.repositoryId, pull.number)
                    const merging = mergingIds.has(key)
                    return (
                      <ListRow
                        key={pull.number}
                        interactive
                        className="group/row grid grid-cols-[1.5rem_4.5rem_1fr_auto] gap-0"
                        onClick={() =>
                          window.open(pull.url, `_blank`, `noopener,noreferrer`)
                        }
                        data-testid={`review-pull-${group.fullName}-${pull.number}`}
                      >
                        <PrOpenIcon className="h-4 w-4 text-emerald-500" />
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
                              <UiLoadingIcon className="h-3.5 w-3.5 animate-spin" />
                              Merging…
                            </>
                          ) : (
                            <>
                              <PrMergedIcon className="h-3.5 w-3.5" />
                              Merge
                            </>
                          )}
                        </Pill>
                      </ListRow>
                    )
                  })}
                </div>
              </div>
            ))}
          </>
        )}
      </div>

      <Prompt
        open={mergeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setMergeTarget(null)
        }}
        title={mergeCopy.title}
        body={mergeCopy.body}
        actions={promptActions(mergeCopy, {
          merge: { onSelect: confirmMerge },
        })}
      />

      <StackMergeChoiceDialog
        choice={stackTarget?.choice ?? null}
        issueId={stackTarget?.entry.issue.id ?? ``}
        onCancel={() => setStackTarget(null)}
        onMerge={(input) => confirmStackMerge(input)}
      />

      <Prompt
        open={sessionMergeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setSessionMergeTarget(null)
        }}
        title={sessionMergeCopy.title}
        body={sessionMergeCopy.body}
        actions={promptActions(sessionMergeCopy, {
          merge: { onSelect: confirmSessionMerge },
        })}
      />

      <Prompt
        open={externalMergeTarget !== null}
        onOpenChange={(open) => {
          if (!open) setExternalMergeTarget(null)
        }}
        title={externalMergeCopy.title}
        body={externalMergeCopy.body}
        actions={promptActions(externalMergeCopy, {
          merge: { onSelect: confirmExternalMerge },
        })}
      />
    </div>
  )
}
