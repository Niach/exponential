import { useCallback, useMemo } from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import {
  ChangesFaceLabel,
  parseSessionResultGroups,
  useIsMobile,
  type SessionDotTone,
  type SessionResultGroup,
} from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import {
  mergeTargetProps,
  useIssueRuns,
  type PastRunRow,
} from "@/hooks/use-agents-data"
import { useIsTeamMember } from "@/components/issue-coding-rows"
import { MergePrPill } from "@/components/run-action-pills"
import { useReviewFiles, type ReviewFilesState } from "@/hooks/use-review-files"
import { totals } from "@exp/domain-contract/diff"
import { useSession } from "@/hooks/use-session"
import {
  shouldConnectSessionDiff,
  useSessionDiffStats,
} from "@/hooks/use-session-diff-stats"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useWorkTabs } from "@/hooks/use-work-tabs"
import type { CodingSession, Issue } from "@/db/schema"
import { useSteerConfig } from "@/components/agent-session"
import { BoardNotFound } from "@/components/board-not-found"
import { IssueChangesFace } from "@/components/issue-changes-face"
import { IssueDetailView } from "@/components/issue-detail-view"
import {
  IssueResultsBody,
  IssueResultsFace,
} from "@/components/issue-results-face"
import { MobileFaceTabs, useFaceSwipe } from "@/components/mobile-face-tabs"
import { selectIssueRuns } from "@/lib/past-runs"
import {
  availableFaces,
  changesFaceCounts,
  codingTarget,
  isSessionLive,
  issueResultsRun,
  type ChangesFaceCounts,
  type WorkFaceKind,
} from "@/lib/work-faces"
import {
  ISSUE_FACE_LABEL,
  RESULTS_FACE_LABEL,
  runFaceLabel,
  WorkFaceToggle,
} from "@/components/team/work-face-toggle"
import { pageTitle, usePageTitle } from "@/lib/page-title"

type IssueSearch = { from?: string; view?: `diff` | `results` }

export const Route = createFileRoute(
  `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`
)({
  head: ({ params }) => ({
    meta: [{ title: pageTitle(params.issueIdentifier) }],
  }),
  // No route-level auth guard: the parent `/t/$teamSlug` layout route
  // (route.tsx) already gates access — anonymous or non-member requests are
  // redirected to login there (EXP-180: nothing is anonymously readable).
  // Mirroring the sibling board-view route, which likewise carries no
  // beforeLoad.
  //
  // EXP-851: `?from=` is the LIST this issue was opened from
  // (`lib/detail-origin.ts`) — the sidebar keeps it beside the issue, and
  // absent means the main menu stays.
  // EXP-893: `?view=diff` is the phone's Changes face over the issue's PR
  // files (a run's live diff lives on the session route instead).
  // EXP-933: `?view=results` is the issue's Results face — the report of the
  // run `issueResultsRun` picks (any member's), drawn on the issue itself;
  // an agent message deep-links here. No results = the issue face.
  validateSearch: (search: Record<string, unknown>): IssueSearch => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
    view:
      search.view === `diff` || search.view === `results`
        ? search.view
        : undefined,
  }),
  component: IssueDetailPage,
})

function IssueDetailPage() {
  const { teamSlug, boardSlug, issueIdentifier } = Route.useParams()
  const search = Route.useSearch()

  // The team/board/users lookups are the board view's, not a second copy
  // (EXP-791: the prev/next switcher that walked its sequence is gone).
  const { board, boardReady, team, users } = useBoardViewData({
    boardSlug,
    teamSlug,
  })

  const { data: issues, isReady: issuesQueryReady } = useLiveQuery(
    (query) =>
      board
        ? query
            .from({ issues: issueCollection })
            .where(({ issues }) =>
              and(
                eq(issues.boardId, board.id),
                eq(issues.identifier, issueIdentifier)
              )
            )
        : undefined,
    [board?.id, issueIdentifier]
  )
  const issue = (issues?.[0] ?? null) as Issue | null
  usePageTitle(
    issue ? pageTitle(`${issue.identifier} ${issue.title}`) : undefined
  )
  // A disabled live query reports `isReady: true`, so the board gate rides
  // along: on a cold deep link the issues snapshot always lands after the
  // boards one, and claiming "not found" in that window is a lie (REV2-32).
  const issueReady = Boolean(board) && issuesQueryReady

  const permissions = useTeamPermissions(team)

  // EXP-870: the Run face — the run this issue's work tab is bound to, else
  // the issue's newest run of mine (`codingTarget`: the bound run when live,
  // else the newest live own run, else the newest own run). No such run =
  // the toggle has one face and does not render (EXP-877).
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id
  const { data: runRows } = useLiveQuery(
    (query) =>
      issue
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => eq(s.issueId, issue.id))
        : undefined,
    [issue?.id]
  )
  const { tabs } = useWorkTabs(team?.id)
  const boundRunId = issue
    ? tabs.find((tab) => tab.kind === `issue` && tab.issueId === issue.id)
    : undefined
  const now = useNow(30_000)
  const runTarget = useMemo(
    () =>
      issue
        ? codingTarget(
            (runRows ?? []) as CodingSession[],
            issue.id,
            boundRunId?.kind === `issue` ? boundRunId.runId : undefined,
            currentUserId,
            now
          )
        : null,
    [runRows, issue, boundRunId, currentUserId, now]
  )
  // EXP-886: with MORE THAN ONE run of mine on the issue the segment reads
  // "Runs" — and (EXP-950) carries the caret to the menu between them.
  const multipleRuns = useMemo(
    () =>
      selectIssueRuns(
        (runRows ?? []) as CodingSession[],
        currentUserId,
        issue?.id
      ).length > 1,
    [runRows, currentUserId, issue?.id]
  )

  // EXP-893: the phone's Work screen — the issue's runs of mine for the
  // switcher's run rows (EXP-950: and the wide toggle's run menu), the target
  // run's live diff (so the Changes face is known without mounting the
  // view), and the faces on offer.
  const { runs: issueRuns } = useIssueRuns(issue?.id, team?.id, currentUserId)
  // EXP-889: every width — the wide toggle's diff item reads it too.
  // EXP-875: but it DIALS only for a run whose diff is still a live thing —
  // running, PR open, or freshly merged — and never with steering off. An
  // ended run used to have its ticket minted and its device asked to
  // republish its journal every time anyone opened the issue.
  const steerConfig = useSteerConfig()
  const diffStats = useSessionDiffStats(runTarget?.id, {
    connect: shouldConnectSessionDiff({
      session: runTarget,
      issuePrState: issue?.prState,
      steerEnabled: Boolean(steerConfig?.enabled),
      now,
    }),
    status: runTarget?.status,
  })
  const hasChanges = diffStats.fileCount > 0 || issue?.prState === `open`
  // EXP-952: source B — the issue's open PR files, read only when there is
  // no live diff. Resolved HERE, not inside the Changes face, because the
  // switcher has to count the very files that face draws — otherwise its
  // Changes row printed no `+N −M` until the face had been opened once
  // (Android's `WorkScreen` owns its `ChangesViewModel` the same way, and
  // the session route hoists `useReviewFiles` for the run's Changes face).
  // A phone thing: md+ has no PR-files face on this route.
  const { state: prFilesState } = useReviewFiles(issue ?? null, {
    enabled: isMobile && hasChanges && diffStats.fileCount === 0,
  })
  // EXP-1152: the phone's Changes tab wears the `+N −M` of the very files
  // that face draws — the run's live diff, else the PR's — the word until
  // either is known (the desktop `FaceToggle::diff` rule).
  const changesCounts = useMemo(
    () =>
      changesFaceCounts(
        diffStats.fileCount > 0
          ? {
              files: diffStats.fileCount,
              additions: diffStats.additions,
              deletions: diffStats.deletions,
            }
          : prFilesState.kind === `files`
            ? totals(prFilesState.files)
            : null
      ),
    [diffStats.fileCount, diffStats.additions, diffStats.deletions, prFilesState]
  )

  // EXP-879/933: the results belong to a RUN — `issueResultsRun` picks it
  // (my target run when it has results, else the newest run on the issue by
  // ANY member that has some). My run: the Results segment opens the run's
  // `?view=results`. A teammate's: the issue draws the report itself
  // (`?view=results` here), since their run is not mine to open.
  const resultsRun = useMemo(
    () =>
      issue
        ? issueResultsRun(
            (runRows ?? []) as CodingSession[],
            issue.id,
            boundRunId?.kind === `issue` ? boundRunId.runId : undefined,
            currentUserId,
            now
          )
        : null,
    [runRows, issue, boundRunId, currentUserId, now]
  )
  const resultGroups = useMemo(
    () => parseSessionResultGroups(resultsRun?.results),
    [resultsRun?.results]
  )
  const hasResults = resultsRun !== null && resultGroups.length > 0
  const resultsMine =
    hasResults && Boolean(currentUserId) && resultsRun.userId === currentUserId
  /** A stale or result-less `?view=results` falls back to the issue face. */
  const showResults = search.view === `results` && hasResults

  const goRun = useCallback(
    (sessionId: string, view?: `diff` | `results`) => {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId },
        search: {
          ...(search.from ? { from: search.from } : {}),
          ...(view ? { view } : {}),
        },
        replace: isMobile,
      })
    },
    [navigate, teamSlug, search.from, isMobile]
  )
  const goFace = useCallback(
    (view?: `diff` | `results`) => {
      if (!board) return
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: { teamSlug, boardSlug: board.slug, issueIdentifier },
        search: {
          ...(search.from ? { from: search.from } : {}),
          ...(view ? { view } : {}),
        },
        replace: isMobile,
      })
    },
    [navigate, teamSlug, board, issueIdentifier, search.from, isMobile]
  )

  if (!team || !board) {
    // Ready-and-empty boards means the slug is dead (trashed board, rename,
    // stale bookmark) — same recovery the board route offers (REV2-59).
    if (boardReady) {
      return (
        <BoardNotFound
          boardSlug={boardSlug}
          teamSlug={teamSlug}
        />
      )
    }
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  if (!issue) {
    // Absent-because-still-syncing, not absent-because-gone (REV2-32).
    if (!issueReady) {
      return <div className="text-muted-foreground text-sm p-6">Loading…</div>
    }
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm">
        <div className="text-muted-foreground">
          Issue <span className="font-mono">{issueIdentifier}</span> not found
          in this board.
        </div>
        <Link
          to="/t/$teamSlug/boards/$boardSlug"
          params={{ teamSlug, boardSlug }}
          className="text-foreground underline-offset-2 hover:underline"
        >
          ← Back to board
        </Link>
      </div>
    )
  }

  const readOnly = !permissions.canMutateIssue(issue)
  const openResults = () => {
    if (!resultsRun) return
    if (resultsMine) goRun(resultsRun.id, `results`)
    else goFace(`results`)
  }

  // EXP-893: the phone. Faces are screen state behind `replace` navigations,
  // so Back always leaves the issue. The Changes face: the run's live diff
  // (the session route's `?view=diff`) when there is one, else the issue's
  // PR files right here. EXP-1150: the faces are TABS under the header
  // (`MobileFaceTabs`) and the body swipes between them (`useFaceSwipe`);
  // the bar's right circle is the view's own Start coding.
  if (isMobile) {
    return (
      <MobileIssuePage
        issue={issue}
        board={board}
        team={team}
        users={users}
        teamSlug={teamSlug}
        currentUserId={currentUserId}
        readOnly={readOnly}
        origin={search.from}
        view={search.view}
        runTarget={runTarget}
        runLive={runTarget ? isSessionLive(runTarget, now) : false}
        issueRuns={issueRuns}
        hasChanges={hasChanges}
        liveDiff={diffStats.fileCount > 0}
        changesCounts={changesCounts}
        prFilesState={prFilesState}
        hasResults={hasResults}
        showResults={showResults}
        resultGroups={resultGroups}
        goFace={goFace}
        goRun={goRun}
        openResults={openResults}
      />
    )
  }

  // EXP-851: the issue IS the content panel — the board list that used to sit
  // on its left moved into the sidebar's list nav (one list, one place, every
  // detail), which is what gives the description and timeline the full width.
  return (
    <IssueDetailView
      issue={issue}
      users={users}
      board={board}
      teamSlug={teamSlug}
      teamId={team.id}
      readOnly={readOnly}
      origin={search.from}
      faceBody={
        showResults ? <IssueResultsBody groups={resultGroups} /> : undefined
      }
      faceToggle={
        <WorkFaceToggle
          face={showResults ? `results` : `issue`}
          runMenu={{
            runs: issueRuns,
            checkedRunId: runTarget?.id,
            onOpen: (target) => goRun(target.id),
          }}
          items={[
            {
              face: `issue`,
              label: ISSUE_FACE_LABEL,
              onSelect: () => {
                if (showResults) goFace()
              },
            },
            ...(runTarget
              ? [
                  {
                    face: `run` as const,
                    label: runFaceLabel(multipleRuns),
                    onSelect: () => goRun(runTarget.id),
                  },
                ]
              : []),
            // EXP-889: the run's diff, as on the run face (and the IDE's
            // issue toggle) — opens the run on its Changes sub-face.
            ...(runTarget && diffStats.fileCount > 0
              ? [
                  {
                    face: `diff` as const,
                    // EXP-1152: the counts recipe every face strip shares.
                    label: <ChangesFaceLabel counts={diffStats} />,
                    onSelect: () => goRun(runTarget.id, `diff`),
                  },
                ]
              : []),
            // EXP-879/933: the report, last in the strip — mine opens on my
            // run, a teammate's right here on the issue.
            ...(hasResults
              ? [
                  {
                    face: `results` as const,
                    label: RESULTS_FACE_LABEL,
                    onSelect: openResults,
                  },
                ]
              : []),
          ]}
        />
      }
    />
  )
}

/** EXP-893 / EXP-1150: the phone's Work screen over an ISSUE subject — the
 *  face on show (issue, its PR files, or a teammate's results), the face
 *  tabs every one of them wears under the header, and the swipe between
 *  them. A child so the swipe hook sits past the page's early returns. */
function MobileIssuePage({
  issue,
  board,
  team,
  users,
  teamSlug,
  currentUserId,
  readOnly,
  origin,
  view,
  runTarget,
  runLive,
  issueRuns,
  hasChanges,
  liveDiff,
  changesCounts,
  prFilesState,
  hasResults,
  showResults,
  resultGroups,
  goFace,
  goRun,
  openResults,
}: {
  issue: Issue
  board: NonNullable<ReturnType<typeof useBoardViewData>[`board`]>
  team: NonNullable<ReturnType<typeof useBoardViewData>[`team`]>
  users: ReturnType<typeof useBoardViewData>[`users`]
  teamSlug: string
  currentUserId: string | undefined
  readOnly: boolean
  origin?: string
  view?: `diff` | `results`
  runTarget: CodingSession | null
  runLive: boolean
  issueRuns: readonly PastRunRow[]
  hasChanges: boolean
  /** The target run has a live diff: Changes opens on the run. */
  liveDiff: boolean
  /** EXP-1152: the Changes tab's `+N −M` (null = the word). */
  changesCounts: ChangesFaceCounts | null
  prFilesState: ReviewFilesState
  hasResults: boolean
  showResults: boolean
  resultGroups: readonly SessionResultGroup[]
  goFace: (view?: `diff` | `results`) => void
  goRun: (sessionId: string, view?: `diff` | `results`) => void
  openResults: () => void
}) {
  const faces = availableFaces({
    hasIssue: true,
    hasRun: Boolean(runTarget),
    hasChanges,
    hasResults,
  })
  // The synced row is all the issue face knows: live → the running dot
  // (amber while it waits on a person); no live run → no dot.
  const sessionTone: SessionDotTone | null = runLive
    ? runTarget?.needsInput
      ? `needs_input`
      : `running`
    : null
  const showChanges = view === `diff` && hasChanges
  const face: WorkFaceKind = showChanges
    ? `changes`
    : showResults
      ? `results`
      : `issue`
  const onFace = (next: WorkFaceKind) => {
    if (next === `issue`) goFace()
    else if (next === `run` && runTarget) goRun(runTarget.id)
    else if (next === `changes`) {
      if (liveDiff && runTarget) goRun(runTarget.id, `diff`)
      else goFace(`diff`)
    } else if (next === `results`) {
      openResults()
    }
  }
  const swipe = useFaceSwipe(faces, face, onFace)
  // EXP-1150: the ONE merge of the phone Work screen rides the header band
  // beside the tabs — the tray's own gating (membership, the relay), the
  // md+ header's own pill.
  const isMember = useIsTeamMember(team.id, currentUserId ?? ``)
  const steerConfig = useSteerConfig()
  const mergePill =
    currentUserId && isMember && issue.prState === `open` ? (
      <MergePrPill
        {...mergeTargetProps({ kind: `issue`, issue })}
        steerEnabled={steerConfig?.enabled === true}
        placement="header"
      />
    ) : undefined
  const tabs = (
    <MobileFaceTabs
      faces={faces}
      face={face}
      runs={issueRuns}
      viewedRunId={runTarget?.id ?? null}
      changesCounts={changesCounts}
      onFace={onFace}
      onOpenRun={(target) => goRun(target.id)}
      trailing={mergePill}
    />
  )
  const dot = sessionTone ? { tone: sessionTone } : null
  if (showChanges) {
    return (
      <IssueChangesFace
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={team.id}
        readOnly={readOnly}
        origin={origin}
        filesState={prFilesState}
        tabs={tabs}
        swipe={swipe}
        dot={dot}
      />
    )
  }
  if (showResults) {
    return (
      <IssueResultsFace
        issue={issue}
        board={board}
        teamSlug={teamSlug}
        teamId={team.id}
        readOnly={readOnly}
        origin={origin}
        groups={resultGroups}
        tabs={tabs}
        swipe={swipe}
        dot={dot}
      />
    )
  }
  return (
    <IssueDetailView
      issue={issue}
      users={users}
      board={board}
      teamSlug={teamSlug}
      teamId={team.id}
      readOnly={readOnly}
      origin={origin}
      mobileWork={{ tabs, swipe, dot }}
    />
  )
}
