import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react"
import { createFileRoute, Link, useNavigate } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { useBoardViewData } from "@/hooks/use-board-view-data"
import {
  ChangesFaceLabel,
  parseSessionResultGroups,
  PrGithubButton,
  useIsMobile,
} from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import {
  mergeTargetProps,
  useIssueRuns,
  type PastRunRow,
} from "@/hooks/use-agents-data"
import { useIsTeamMember } from "@/components/issue-coding-rows"
import { useReviewFiles, type ReviewFilesState } from "@/hooks/use-review-files"
import { usePrDescription } from "@/hooks/use-pr-description"
import { runFaceSearch, useLiveDiffHandoff } from "@/hooks/use-issue-face-nav"
import { totals, type DiffFile } from "@exp/domain-contract/diff"
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
import {
  IssueChangesBody,
  IssueChangesFace,
  MergeCapsule,
} from "@/components/issue-changes-face"
import { IssueDetailView } from "@/components/issue-detail-view"
import {
  IssueResultsBody,
  IssueResultsFace,
  prDescriptionGroups,
} from "@/components/issue-results-face"
import { MobileMergeCircle } from "@/components/mobile-merge-circle"
import { publishReviewFiles } from "@/lib/review-files-slot"
import { MobileFaceTabs, useFaceSwipe } from "@/components/mobile-face-tabs"
import { selectIssueRuns } from "@/lib/past-runs"
import {
  availableFaces,
  changesFaceCounts,
  codingTarget,
  faceDots,
  isSessionLive,
  issueResultsRun,
  toggleFaceDots,
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
import { runFaceMark } from "@/lib/coding-session-display"

type IssueSearch = { from?: string; view?: `diff` | `results`; file?: string }

/** One stable empty list, so a loading diff is not a new array per render. */
const EMPTY_FILES: DiffFile[] = []

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
  // EXP-893: `?view=diff` is the Changes face over the issue's PR / branch
  // files (a run's live diff lives on the session route instead). EXP-1154:
  // at every width — it IS the review of the PR (the Reviews detail page is
  // gone), always honoured ("No changes yet" when nothing was pushed);
  // `?file=` seeds the file in focus.
  // EXP-933: `?view=results` is the issue's Results face — the report of the
  // run `issueResultsRun` picks (any member's), drawn on the issue itself as
  // the Guide (EXP-1154); an agent message deep-links here. No report and no
  // open PR = the issue face.
  validateSearch: (search: Record<string, unknown>): IssueSearch => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
    view:
      search.view === `diff` || search.view === `results`
        ? search.view
        : undefined,
    file:
      typeof search.file === `string` && search.file ? search.file : undefined,
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
  const showChanges = search.view === `diff`
  // EXP-1154: a pushed branch with no PR yet counts too (the ×4 rule); the
  // branch diff answering `none` (never pushed) drops the face again.
  const branchOnly =
    issue?.prNumber == null && Boolean(issue?.branch?.trim())
  // EXP-952: source B — the issue's PR / branch files, read only when there
  // is no live diff (or the face is asked for). Resolved HERE, not inside the
  // Changes face, because the tabs have to count the very files that face
  // draws — otherwise its Changes row printed no `+N −M` until the face had
  // been opened once (Android's `WorkScreen` owns its `ChangesViewModel` the
  // same way, and the session route hoists `useReviewFiles` for the run's
  // Changes face). EXP-1154: every width — md+ draws the face too now.
  const { state: prFilesState, reload: reloadPrFiles } = useReviewFiles(
    issue ?? null,
    {
      enabled:
        showChanges ||
        (diffStats.fileCount === 0 &&
          (issue?.prState === `open` || branchOnly)),
    }
  )
  const hasChanges =
    diffStats.fileCount > 0 ||
    issue?.prState === `open` ||
    (branchOnly && prFilesState.kind !== `none`)
  const prFiles = prFilesState.kind === `files` ? prFilesState.files : EMPTY_FILES
  // EXP-1154: the file in focus on the Changes face — seeded by `?file=`
  // (a Results file row), then the tree / sheet / cards own it.
  const [selectedFile, setSelectedFile] = useState<string | null>(
    search.file ?? null
  )
  // The route is reused across issues (no remount on a param change): issue
  // B must never open on issue A's file. Reset during render, before use.
  const [selectedFor, setSelectedFor] = useState(issue?.id ?? null)
  if (selectedFor !== (issue?.id ?? null)) {
    setSelectedFor(issue?.id ?? null)
    setSelectedFile(search.file ?? null)
  }
  useEffect(() => {
    if (search.file) setSelectedFile(search.file)
  }, [search.file])
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
  const hasRunResults = resultsRun !== null && resultGroups.length > 0
  const resultsMine =
    hasRunResults &&
    Boolean(currentUserId) &&
    resultsRun.userId === currentUserId
  // EXP-1154: an open PR with no report still has a Results face — the
  // GitHub PR body as one unnumbered group.
  const hasGuide = hasRunResults || issue?.prState === `open`
  /** A stale or guide-less `?view=results` falls back to the issue face. */
  const showResults = search.view === `results` && hasGuide
  const { state: prDescriptionState } = usePrDescription(issue ?? null, {
    enabled: showResults && !hasRunResults,
  })
  const guideGroups = useMemo(
    () =>
      hasRunResults ? resultGroups : prDescriptionGroups(prDescriptionState),
    [hasRunResults, resultGroups, prDescriptionState]
  )

  const goRun = useCallback(
    (
      sessionId: string,
      view?: `diff` | `results`,
      /** EXP-1154: the file in focus on the run's Changes face. */
      file?: string,
      replace: boolean = isMobile
    ) => {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId },
        search: runFaceSearch({ from: search.from, view, file }),
        replace,
      })
    },
    [navigate, teamSlug, search.from, isMobile]
  )
  const goFace = useCallback(
    (view?: `diff` | `results`, file?: string) => {
      if (!board) return
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: { teamSlug, boardSlug: board.slug, issueIdentifier },
        search: {
          ...(search.from ? { from: search.from } : {}),
          ...(view ? { view } : {}),
          ...(file ? { file } : {}),
        },
        replace: isMobile,
      })
    },
    [navigate, teamSlug, board, issueIdentifier, search.from, isMobile]
  )
  /** EXP-1154: a Guide file row — the Changes face on that file. The run's
   *  live diff when there is one, else the PR's right here. */
  const liveDiff = diffStats.fileCount > 0
  const openFile = useCallback(
    (path: string) => {
      setSelectedFile(path)
      if (liveDiff && runTarget) goRun(runTarget.id, `diff`, path)
      else goFace(`diff`, path)
    },
    [liveDiff, runTarget, goRun, goFace]
  )
  /** The Guide's rows count off the files that tap opens: the live diff when
   *  the run has one, else the PR's. */
  const guideFiles = liveDiff ? diffStats.files : prFiles
  // EXP-1154: a `?view=diff` deep link while the run has a live diff hands
  // off to the run (it owns that diff), replacing the entry so Back skips it;
  // the issue's PR-files face is only for the no-live-diff case.
  const handoffToRun = useCallback(
    (sessionId: string, view: `diff`, file?: string) =>
      goRun(sessionId, view, file, true),
    [goRun]
  )
  useLiveDiffHandoff(
    { showChanges, liveDiff, runId: runTarget?.id, file: search.file },
    handoffToRun
  )
  const backToIssue = useCallback(() => goFace(), [goFace])

  // EXP-916/945/1154: on md+ the Changes face's file TREE is the sidebar's
  // panel (`ReviewFilesNav`), keyed by the identifier the occupant matches,
  // its back row the issue itself. Cleared whenever the face is not up.
  const publishTree = showChanges && !isMobile && Boolean(issue)
  useEffect(() => {
    if (!publishTree) return
    publishReviewFiles({
      subjectId: issueIdentifier,
      status: prFilesState.kind,
      files: prFiles,
      selected: selectedFile,
      onSelect: setSelectedFile,
      back: { label: issueIdentifier, onBack: backToIssue },
    })
  }, [publishTree, issueIdentifier, prFilesState.kind, prFiles, selectedFile, backToIssue])
  useEffect(() => {
    if (publishTree) return
    publishReviewFiles(null)
  }, [publishTree])
  useEffect(() => () => publishReviewFiles(null), [])

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
    if (resultsRun && resultsMine) goRun(resultsRun.id, `results`)
    else goFace(`results`)
  }
  const guideBody = (
    <IssueResultsBody
      groups={guideGroups}
      files={guideFiles}
      onOpenFile={hasChanges ? openFile : undefined}
      numbered={hasRunResults}
      loading={!hasRunResults && prDescriptionState.kind === `loading`}
      error={
        !hasRunResults && prDescriptionState.kind === `error`
          ? prDescriptionState.message
          : null
      }
    />
  )

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
        liveDiff={liveDiff}
        changesCounts={changesCounts}
        prFilesState={prFilesState}
        reloadPrFiles={reloadPrFiles}
        selectedFile={selectedFile}
        onSelectFile={setSelectedFile}
        hasResults={hasGuide}
        showResults={showResults}
        guideBody={guideBody}
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
        showChanges ? (
          <IssueChangesBody
            state={prFilesState}
            selected={selectedFile}
            onSelect={setSelectedFile}
            onRetry={reloadPrFiles}
          />
        ) : showResults ? (
          guideBody
        ) : undefined
      }
      /* EXP-949/1154: GitHub only while the Changes face shows. */
      headerAction={
        showChanges && issue.prUrl ? (
          <PrGithubButton prUrl={issue.prUrl} />
        ) : undefined
      }
      faceToggle={
        <WorkFaceToggle
          face={showChanges ? `diff` : showResults ? `results` : `issue`}
          /* EXP-1162: the tabs carry the state — live run, open PR. */
          run={runFaceMark(runTarget, issue.prState)}
          dots={toggleFaceDots(
            faceDots({
              faces: availableFaces({
                hasIssue: true,
                hasRun: Boolean(runTarget),
                hasChanges: hasChanges || showChanges,
                hasResults: hasGuide,
              }),
              runLive: runTarget ? isSessionLive(runTarget, now) : false,
              needsInput: runTarget?.needsInput === true,
              prOpen: issue.prState === `open`,
            })
          )}
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
                if (showResults || showChanges) goFace()
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
            // EXP-889: the run's live diff opens the run on its Changes
            // sub-face; EXP-1154: else the PR / branch files right here.
            ...(hasChanges || showChanges
              ? [
                  {
                    face: `diff` as const,
                    // EXP-1152: the counts recipe every face strip shares.
                    label: <ChangesFaceLabel counts={changesCounts} />,
                    onSelect: () => {
                      if (liveDiff && runTarget) goRun(runTarget.id, `diff`)
                      else if (!showChanges) goFace(`diff`)
                    },
                  },
                ]
              : []),
            // EXP-879/933: the report, last in the strip — mine opens on my
            // run, a teammate's (EXP-1154: or the PR body) right here.
            ...(hasGuide
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
 *  face on show (issue, its PR files, or the Guide), the face tabs every one
 *  of them wears under the header, and the swipe between them. EXP-1154: the
 *  ONE merge is the white capsule on the floating bar — in the Changes and
 *  Results bars' cluster, floating above the Issue face's composer bar. A
 *  child so the swipe hook sits past the page's early returns. */
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
  reloadPrFiles,
  selectedFile,
  onSelectFile,
  hasResults,
  showResults,
  guideBody,
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
  reloadPrFiles: () => void
  /** EXP-1154: the file in focus on the Changes face (`?file=`). */
  selectedFile: string | null
  onSelectFile: (path: string) => void
  /** EXP-1154: the Guide exists (a report, or an open PR's body). */
  hasResults: boolean
  showResults: boolean
  /** The Guide (`IssueResultsBody`), built by the page. */
  guideBody: ReactNode
  goFace: (view?: `diff` | `results`) => void
  goRun: (sessionId: string, view?: `diff` | `results`) => void
  openResults: () => void
}) {
  // EXP-1154: `?view=diff` is always honoured ("No changes yet").
  const showChanges = view === `diff`
  const faces = availableFaces({
    hasIssue: true,
    hasRun: Boolean(runTarget),
    hasChanges: hasChanges || showChanges,
    hasResults,
  })
  // EXP-1162: the state lives on the TABS, never on the title — the Run tab
  // says its run is live (amber while it waits on a person), the Results /
  // Changes tab that the pull request is open. The synced row is all the
  // issue face knows.
  const dots = faceDots({
    faces,
    runLive,
    needsInput: runTarget?.needsInput === true,
    prOpen: issue.prState === `open`,
  })
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
  // EXP-1154: the white Merge capsule — the tray's own gating (membership,
  // the relay); it self-hides unless the PR is open.
  const isMember = useIsTeamMember(team.id, currentUserId ?? ``)
  const steerConfig = useSteerConfig()
  const canMerge = Boolean(currentUserId) && isMember && issue.prState === `open`
  const mergeTarget = {
    ...mergeTargetProps({ kind: `issue`, issue }),
    steerEnabled: steerConfig?.enabled === true,
  }
  const mergeCapsule = canMerge ? <MergeCapsule {...mergeTarget} /> : undefined
  const tabs = (
    <MobileFaceTabs
      faces={faces}
      face={face}
      dots={dots}
      run={runFaceMark(runTarget, issue.prState)}
      runs={issueRuns}
      viewedRunId={runTarget?.id ?? null}
      changesCounts={changesCounts}
      onFace={onFace}
      onOpenRun={(target) => goRun(target.id)}
    />
  )
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
        onRetry={reloadPrFiles}
        selected={selectedFile}
        onSelect={onSelectFile}
        merge={mergeCapsule}
        tabs={tabs}
        swipe={swipe}
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
        body={guideBody}
        merge={mergeCapsule}
        tabs={tabs}
        swipe={swipe}
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
      mobileWork={{
        tabs,
        swipe,
        merge: canMerge ? <MobileMergeCircle {...mergeTarget} /> : undefined,
      }}
    />
  )
}
