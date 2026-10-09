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
  ChangesFileSheet,
  parseSessionResultGroups,
  PrGithubButton,
  sessionResultsForPr,
  useIsMobile,
  type GuideChangesTarget,
} from "@exp/ui"
import { useNow } from "@/hooks/use-now"
import {
  mergeTargetProps,
  useIssueRuns,
  type PastRunRow,
} from "@/hooks/use-agents-data"
import { useIsTeamMember } from "@/components/issue-coding-rows"
import { useReviewFiles } from "@/hooks/use-review-files"
import { usePrDescription } from "@/hooks/use-pr-description"
import { runFaceSearch } from "@/hooks/use-issue-face-nav"
import type { DiffFile } from "@exp/domain-contract/diff"
import { useSession } from "@/hooks/use-session"
import {
  shouldConnectSessionDiff,
  useSessionDiffStats,
} from "@/hooks/use-session-diff-stats"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { useWorkTabs } from "@/hooks/use-work-tabs"
import type { CodingSession, Issue } from "@/db/schema"
import { renderResultText, useSteerConfig } from "@/components/agent-session"
import { BoardNotFound } from "@/components/board-not-found"
import {
  GuideBody,
  GuideFace,
  GuideStackCard,
  guideFilesOf,
  prDescriptionGroups,
  useIssueStack,
} from "@/components/guide-face"
import { GuideSectionDiff } from "@/components/guide-section-diff"
import { IssueDetailView } from "@/components/issue-detail-view"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import {
  MergeCapsule,
  MobileMergeCircle,
  useMergeThrough,
} from "@/components/session-merge-button"
import {
  ISSUE_FACE_LABEL,
  GUIDE_FACE_LABEL,
  MobileFaceTabs,
  runFaceLabel,
  useFaceSwipe,
  WorkFaceToggle,
} from "@/components/work-faces"
import { selectIssueRuns } from "@/lib/past-runs"
import {
  availableFaces,
  codingTarget,
  faceDots,
  guideSearch,
  guideSectionPage,
  isSessionLive,
  issueResultsRun,
  parseGuideSearch,
  type GuideSearch,
  type GuideSectionKey,
  type WorkFaceKind,
} from "@/lib/work-faces"
import { pageTitle, usePageTitle } from "@/lib/page-title"
import { runFaceMark } from "@/lib/coding-session-display"

type IssueSearch = { from?: string } & GuideSearch

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
  // EXP-1251: `?view=guide` is the issue's GUIDE face — the report of the
  // run `issueResultsRun` picks (any member's; an agent message deep-links
  // here) over the diff (my run's live diff, else the PR / branch files);
  // `&section=N|lead|other|all&file=` opens a section's changes as a page
  // under it. The legacy `?view=results` lands on the Guide, `?view=diff` on
  // its complete diff (`parseGuideSearch`).
  validateSearch: (search: Record<string, unknown>): IssueSearch => ({
    from:
      typeof search.from === `string` && search.from ? search.from : undefined,
    ...parseGuideSearch(search),
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
  // no Run face (EXP-877).
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
  const { runs: issueRuns } = useIssueRuns(issue?.id, team?.id, currentUserId)
  // EXP-875: the run's live diff DIALS only for a run whose diff is still a
  // live thing — running, PR open, or freshly merged — and never with
  // steering off.
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
  const liveDiff = diffStats.fileCount > 0
  // EXP-1154: a pushed branch with no PR yet counts too (the ×4 rule); the
  // branch diff answering `none` (never pushed) drops it again.
  const branchOnly =
    issue?.prNumber == null && Boolean(issue?.branch?.trim())
  const askedGuide = search.view === `guide`
  // EXP-952: the PR / branch files, read only when there is no live diff —
  // resolved HERE so the Guide's rows count the very files its pages draw.
  const { state: prFilesState, reload: reloadPrFiles } = useReviewFiles(
    issue ?? null,
    {
      enabled:
        !liveDiff &&
        (askedGuide || issue?.prState === `open` || branchOnly),
    }
  )
  const hasDiff =
    liveDiff ||
    issue?.prState === `open` ||
    (branchOnly && prFilesState.kind !== `none`)
  /** The diff the Guide counts (null = not loaded): the live diff when my
   *  run has one, else the PR's. */
  const guideFiles: readonly DiffFile[] | null = liveDiff
    ? diffStats.files
    : guideFilesOf(prFilesState)

  // EXP-933/1251: the report belongs to a RUN — `issueResultsRun` picks it
  // (my target run when it has results, else the newest run on the issue by
  // ANY member, else a run that tagged a topic with this issue's PR), and the
  // issue shows only its own PR's topics (`sessionResultsForPr`).
  const resultsRun = useMemo(
    () =>
      issue
        ? issueResultsRun(
            (runRows ?? []) as CodingSession[],
            issue.id,
            boundRunId?.kind === `issue` ? boundRunId.runId : undefined,
            currentUserId,
            now,
            issue.prUrl
          )
        : null,
    [runRows, issue, boundRunId, currentUserId, now]
  )
  const resultGroups = useMemo(
    () =>
      resultsRun
        ? parseSessionResultGroups(
            sessionResultsForPr(resultsRun.results, issue?.prUrl)
          )
        : [],
    [resultsRun, issue?.prUrl]
  )
  const hasRunResults = resultGroups.length > 0
  const hasGuide = hasRunResults || hasDiff
  const showGuide = askedGuide && (hasGuide || search.section !== undefined)
  const { state: prDescriptionState } = usePrDescription(issue ?? null, {
    enabled: showGuide && !hasRunResults && issue?.prState === `open`,
  })
  const guideGroups = useMemo(
    () =>
      hasRunResults ? resultGroups : prDescriptionGroups(prDescriptionState),
    [hasRunResults, resultGroups, prDescriptionState]
  )
  // EXP-1251: a section page — null while its files load or for a stale
  // section (then the Guide shows).
  const sectionPage = useMemo(
    () =>
      showGuide && search.section !== undefined
        ? guideSectionPage(guideGroups, guideFiles, search.section)
        : null,
    [showGuide, search.section, guideGroups, guideFiles]
  )
  // The file in focus on a section page: seeded by `?file=`, then the tree /
  // sheet / cards own it. The route is reused across issues, so issue B
  // never opens on issue A's file (reset during render, before use).
  const [selectedFile, setSelectedFile] = useState<string | null>(
    search.file ?? null
  )
  const [selectedFor, setSelectedFor] = useState(issue?.id ?? null)
  if (selectedFor !== (issue?.id ?? null)) {
    setSelectedFor(issue?.id ?? null)
    setSelectedFile(search.file ?? null)
  }
  useEffect(() => {
    if (search.file) setSelectedFile(search.file)
  }, [search.file])

  const goRun = useCallback(
    (sessionId: string, replace: boolean = isMobile) => {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId },
        search: runFaceSearch({ from: search.from }),
        replace,
      })
    },
    [navigate, teamSlug, search.from, isMobile]
  )
  /** The issue face (no target) or its Guide / a Guide section page. */
  const goFace = useCallback(
    (target?: { section?: GuideSectionKey; file?: string | null }) => {
      if (!board) return
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: { teamSlug, boardSlug: board.slug, issueIdentifier },
        search: target
          ? guideSearch({ from: search.from, ...target })
          : search.from
            ? { from: search.from }
            : {},
        replace: isMobile,
      })
    },
    [navigate, teamSlug, board, issueIdentifier, search.from, isMobile]
  )
  const openGuide = useCallback(() => goFace({}), [goFace])
  const openSection = useCallback(
    (target: GuideChangesTarget) => {
      setSelectedFile(null)
      goFace({ section: target.section })
    },
    [goFace]
  )
  // EXP-1248: a stack member REPLACES the subject in place — the same face,
  // never a new tab.
  const { stack, memberTarget } = useIssueStack(issue, team?.id)
  const openStackMember = useCallback(
    (issueId: string) => {
      const target = memberTarget(issueId)
      if (!target) return
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: {
          teamSlug,
          boardSlug: target.boardSlug,
          issueIdentifier: target.identifier,
        },
        search: guideSearch({ from: search.from }),
        replace: true,
      })
    },
    [memberTarget, navigate, teamSlug, search.from]
  )
  const mergeThrough = useMergeThrough()
  const isMember = useIsTeamMember(team?.id ?? ``, currentUserId ?? ``)
  const canMerge =
    Boolean(currentUserId) && isMember && issue?.prState === `open`

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
  const stackCard = stack ? (
    <GuideStackCard
      stack={stack}
      onOpen={openStackMember}
      onMergeThrough={canMerge ? mergeThrough.request : undefined}
    />
  ) : undefined
  const guideBody = sectionPage ? (
    <GuideSectionDiff
      page={sectionPage}
      selected={selectedFile}
      onSelect={setSelectedFile}
      onBack={openGuide}
      isMobile={isMobile}
    />
  ) : (
    <GuideBody
      groups={guideGroups}
      files={guideFiles}
      filesState={liveDiff ? null : prFilesState}
      numbered={hasRunResults}
      loading={!hasRunResults && prDescriptionState.kind === `loading`}
      error={
        !hasRunResults && prDescriptionState.kind === `error`
          ? prDescriptionState.message
          : null
      }
      stack={stackCard}
      onOpenChanges={openSection}
      onRetry={reloadPrFiles}
      renderText={renderResultText}
    />
  )
  const faces = availableFaces({
    hasIssue: true,
    hasRun: Boolean(runTarget),
    hasResults: hasRunResults,
    hasDiff: hasDiff || showGuide,
  })
  // EXP-1162: the state lives on the TABS — the Run tab says its run is live
  // (amber while it waits on a person), the Guide tab that the pull request
  // is open.
  const dots = faceDots({
    faces,
    runLive: runTarget ? isSessionLive(runTarget, now) : false,
    needsInput: runTarget?.needsInput === true,
    prOpen: issue.prState === `open`,
  })
  const face: WorkFaceKind = showGuide ? `guide` : `issue`
  const onFace = (next: WorkFaceKind) => {
    if (next === `issue`) goFace()
    else if (next === `run` && runTarget) goRun(runTarget.id)
    else if (next === `guide`) openGuide()
  }
  const mergeTarget = {
    ...mergeTargetProps({ kind: `issue`, issue }),
    steerEnabled: steerConfig?.enabled === true,
  }

  // EXP-893: the phone. Faces are screen state behind `replace` navigations,
  // so Back always leaves the issue. EXP-1150: the faces are TABS under the
  // header and the body swipes between them.
  if (isMobile) {
    return (
      <MobileIssuePage
        issue={issue}
        board={board}
        team={team}
        users={users}
        teamSlug={teamSlug}
        readOnly={readOnly}
        origin={search.from}
        faces={faces}
        face={face}
        dots={dots}
        runTarget={runTarget}
        issueRuns={issueRuns}
        onFace={onFace}
        goRun={goRun}
        guideBody={guideBody}
        sectionFiles={sectionPage?.files ?? null}
        selectedFile={selectedFile}
        onSelectFile={setSelectedFile}
        mergeCapsule={canMerge ? <MergeCapsule {...mergeTarget} /> : undefined}
        mergeCircle={
          canMerge ? <MobileMergeCircle {...mergeTarget} /> : undefined
        }
        mergeThroughDialog={mergeThrough.dialog}
      />
    )
  }

  // EXP-851: the issue IS the content panel; the Guide draws in its work
  // column under the same header (EXP-1251).
  return (
    <>
      <IssueDetailView
        issue={issue}
        users={users}
        board={board}
        teamSlug={teamSlug}
        teamId={team.id}
        readOnly={readOnly}
        origin={search.from}
        faceBody={showGuide ? guideBody : undefined}
        /* EXP-949/1251: GitHub only while the Guide shows. */
        headerAction={
          showGuide && issue.prUrl ? (
            <PrGithubButton prUrl={issue.prUrl} />
          ) : undefined
        }
        faceToggle={
          <WorkFaceToggle
            face={face}
            /* EXP-1162: the tabs carry the state — live run, open PR. */
            run={runFaceMark(runTarget, issue.prState)}
            dots={dots}
            runMenu={{
              runs: issueRuns,
              checkedRunId: runTarget?.id,
              onOpen: (target) => goRun(target.id),
            }}
            items={faces.map((kind) => ({
              face: kind,
              label:
                kind === `issue`
                  ? ISSUE_FACE_LABEL
                  : kind === `run`
                    ? runFaceLabel(multipleRuns)
                    : GUIDE_FACE_LABEL,
              onSelect: () => onFace(kind),
            }))}
          />
        }
      />
      {mergeThrough.dialog}
    </>
  )
}

/** EXP-893 / EXP-1150: the phone's Work screen over an ISSUE subject — the
 *  face on show (the issue or its Guide), the face tabs every one of them
 *  wears under the header, and the swipe between them. EXP-1154: the ONE
 *  merge is the white capsule on the Guide's bar, the circle beside the
 *  Issue face's composer. A child so the swipe hook sits past the page's
 *  early returns. */
function MobileIssuePage({
  issue,
  board,
  team,
  users,
  teamSlug,
  readOnly,
  origin,
  faces,
  face,
  dots,
  runTarget,
  issueRuns,
  onFace,
  goRun,
  guideBody,
  sectionFiles,
  selectedFile,
  onSelectFile,
  mergeCapsule,
  mergeCircle,
  mergeThroughDialog,
}: {
  issue: Issue
  board: NonNullable<ReturnType<typeof useBoardViewData>[`board`]>
  team: NonNullable<ReturnType<typeof useBoardViewData>[`team`]>
  users: ReturnType<typeof useBoardViewData>[`users`]
  teamSlug: string
  readOnly: boolean
  origin?: string
  faces: WorkFaceKind[]
  face: WorkFaceKind
  dots: ReturnType<typeof faceDots>
  runTarget: CodingSession | null
  issueRuns: readonly PastRunRow[]
  onFace: (face: WorkFaceKind) => void
  goRun: (sessionId: string) => void
  /** The Guide (or a section page), built by the page. */
  guideBody: ReactNode
  /** A section page's files: the bar's file sheet. */
  sectionFiles: readonly DiffFile[] | null
  selectedFile: string | null
  onSelectFile: (path: string) => void
  mergeCapsule?: ReactNode
  mergeCircle?: ReactNode
  mergeThroughDialog: ReactNode
}) {
  const swipe = useFaceSwipe(faces, face, onFace)
  const tabs = (
    <MobileFaceTabs
      faces={faces}
      face={face}
      dots={dots}
      run={runFaceMark(runTarget, issue.prState)}
      runs={issueRuns}
      viewedRunId={runTarget?.id ?? null}
      onFace={onFace}
      onOpenRun={(target) => goRun(target.id)}
    />
  )
  if (face === `guide`) {
    return (
      <>
        <GuideFace
          issue={issue}
          board={board}
          teamSlug={teamSlug}
          teamId={team.id}
          readOnly={readOnly}
          origin={origin}
          body={guideBody}
          action={issue.prUrl ? <PrGithubButton prUrl={issue.prUrl} /> : undefined}
          graphBadge={
            /* SLOP-16: the same "Related work" overlay every face opens. */
            <PrGraphBadge teamId={team.id} teamSlug={teamSlug} issue={issue} />
          }
          leading={
            sectionFiles && sectionFiles.length > 0 ? (
              <ChangesFileSheet
                files={sectionFiles}
                selected={selectedFile}
                onSelect={onSelectFile}
              />
            ) : undefined
          }
          merge={mergeCapsule}
          tabs={tabs}
          swipe={swipe}
        />
        {mergeThroughDialog}
      </>
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
      mobileWork={{ tabs, swipe, merge: mergeCircle }}
    />
  )
}
