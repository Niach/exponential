import { useCallback, useMemo, useState, type ReactNode } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { AgentSessionView, renderResultText } from "@/components/agent-session"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { IssueActionsMenu } from "@/components/issue-actions-menu"
import { IssueCodingAction } from "@/components/issue-coding-action"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import {
  Button,
  CollapsedTitle,
  PrGithubButton,
  parseSessionResultGroups,
  useIsMobile,
  WORK_COLUMN_CLASS,
} from "@exp/ui"
import {
  GuideBody,
  GuideStackCard,
  guideFilesOf,
  useIssueStack,
} from "@/components/guide-face"
import { GuideSectionDiff } from "@/components/guide-section-diff"
import { useMergeThrough } from "@/components/session-merge-button"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import {
  guideSearch,
  guideSectionPage,
  parseGuideSearch,
  type GuideSearch,
  type GuideSectionKey,
} from "@/lib/work-faces"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { sessionDescendantIds, sessionTree } from "@/lib/sessions/session-tree"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useOpenSession } from "@/hooks/use-open-session"
import { useReviewFiles, useSessionPrFiles } from "@/hooks/use-review-files"
import type { Board, CodingSession, Issue, Team } from "@/db/schema"
import {
  rowPrState,
  useIssueRuns,
  useRunChain,
  useSessionRow,
  type AgentSessionRow,
} from "@/hooks/use-agents-data"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { sessionIdentity } from "@/lib/session-identity"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle, usePageTitle } from "@/lib/page-title"

// EXP-740: one coding session, FULLSCREEN on its own route — the web twin of
// the desktop IDE's `Screen::Session` center tab and the natives' pushed
// Agent-session screen. EXP-851: the view fills the whole content panel on
// every breakpoint — no shell, no list beside it. The list the run came from
// is the SIDEBAR's job now (`?from=`, `lib/detail-origin.ts`). EXP-870: this
// is the ONE run URL — an issue's run too, whose issue is the work tab's other
// face. EXP-1251: `?view=guide` is the run's Guide (its report over its diff)
// and `&section=` a section's diff page under it (the legacy `?view=diff` /
// `?view=results` land there, `parseGuideSearch`). EXP-893: on a phone the
// faces are the Work screen's — `replace` navigations, so Back leaves the
// subject.
//
// EXP-312: a LIVE session is visible and steerable by its OWNER alone (the
// relay ticket mint refuses everyone else). A teammate's session id therefore
// renders an identity stub with the synced status badge and NO view mount —
// mounting it would try to mint a ticket the server will refuse.
// EXP-1154/1251: `?view=guide` is the exception: the report rides the synced
// row (results sync team-wide) and the PR files are a fetch away, and a run
// without an issue links its PR body footer here, so a teammate reads the
// Guide read-only (`TeammateRunGuide`).

type SessionSearch = { from?: string } & GuideSearch

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
  // EXP-818: `?from=` is WHERE this run was opened from (`lib/detail-origin.ts`
  // — the desktop's `derive_origin`), so Back returns to that list instead of
  // always landing on the Agent page. Absent = the Agent page's own list.
  validateSearch: (search: Record<string, unknown>): SessionSearch => ({
    from: typeof search.from === `string` && search.from ? search.from : undefined,
    // EXP-1251: the Guide (and its section pages); anything else normalises
    // away to the run itself.
    ...parseGuideSearch(search),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SessionPage,
})

function SessionPage() {
  const { teamSlug, sessionId } = Route.useParams()
  const { from, view, section, file } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id

  const { row, session, isReady } = useSessionRow(
    team?.id,
    currentUserId,
    sessionId
  )
  const title = row ? sessionIdentity(row) : null
  usePageTitle(
    title
      ? pageTitle(
          title.identifier
            ? `${title.identifier} ${title.subject}`
            : title.subject,
          `Agent`
        )
      : undefined
  )

  // EXP-818: Back returns to the ORIGIN the run was opened from — the inbox,
  // a board, the Automations list — and to the Agent page when there was no
  // list context (a deep link, a pinned row, a full-page screen). Still a
  // deliberate navigation, not the browser's history (EXP-827). EXP-870: the
  // destination is `originListNavigation`, the list nav's own back row.
  const origin = useMemo(() => parseOrigin(from), [from])
  const goBack = useCallback(() => {
    void navigate(
      (originListNavigation(teamSlug, origin) ?? {
        to: `/t/$teamSlug/agent`,
        params: { teamSlug },
      }) as never
    )
  }, [navigate, teamSlug, origin])

  if (!team || !currentUserId || !isReady) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="p-6 text-sm text-muted-foreground">Loading…</div>
      </div>
    )
  }

  if (!session || !row) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <SessionStubHeader onBack={goBack} title="Session" />
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-sm text-muted-foreground">Session not found.</p>
          <Button asChild variant="outline" size="sm">
            <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
              Go to Agent
            </Link>
          </Button>
        </div>
      </div>
    )
  }

  const identity = sessionIdentity(row)

  // EXP-312: a teammate's run — the synced row is all this client may ever
  // see. No AgentSessionView, so no ticket is minted.
  if (session.userId !== currentUserId) {
    // EXP-1194/1251: the report and the PR's diff — a teammate reads the
    // Guide read-only.
    if (view === `guide`) {
      return (
        <TeammateRunGuide
          session={session}
          title={identity.subject}
          section={section}
          file={file ?? null}
          onSection={(next) =>
            void navigate({
              to: `/t/$teamSlug/sessions/$sessionId`,
              params: { teamSlug, sessionId: session.id },
              search: guideSearch({ from, section: next }),
            })
          }
          onBack={goBack}
        />
      )
    }
    return (
      <div className="flex h-full min-h-0 flex-col">
        <SessionStubHeader onBack={goBack} title={identity.subject} />
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <div className="flex min-w-0 items-center gap-1.5">
            {identity.identifier && (
              <span className="shrink-0 font-mono text-xs text-muted-foreground">
                {identity.identifier}
              </span>
            )}
            <span className="min-w-0 truncate text-sm font-medium">
              {identity.subject}
            </span>
          </div>
          <SessionStatusBadge
            session={session}
            prState={rowPrState(session, row.issue)}
          />
          <p className="text-sm text-muted-foreground">
            Only the owner can steer this session.
          </p>
        </div>
      </div>
    )
  }

  return (
    <OwnSessionPage
      key={session.id}
      team={team}
      teamSlug={teamSlug}
      row={row}
      session={session}
      currentUserId={currentUserId}
      from={from}
      face={view === `guide` ? `guide` : `run`}
      guideSection={section}
      diffFile={file ?? null}
      onBack={goBack}
    />
  )
}

/** The owner's run: the view plus the issue face's header parts when the run
 * is issue-bound. A child component so its hooks (permissions, roster, the
 * property handlers) stay unconditional past the page's early returns. */
function OwnSessionPage({
  team,
  teamSlug,
  row,
  session,
  currentUserId,
  from,
  face,
  guideSection,
  diffFile,
  onBack,
}: {
  team: Team
  teamSlug: string
  row: AgentSessionRow
  session: CodingSession
  currentUserId: string
  from: string | undefined
  face: `run` | `guide`
  /** EXP-1251: `?section=` on the Guide. */
  guideSection: GuideSectionKey | undefined
  /** EXP-1154: `?file=` on a section page. */
  diffFile: string | null
  onBack: () => void
}) {
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const issue: Issue | null = row.issue ?? null
  const board: Board | null = row.board ?? null
  const permissions = useTeamPermissions(team)
  const { users } = useTeamUsers(team.id)
  const readOnly = issue ? !permissions.canMutateIssue(issue) : true
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })

  // EXP-870: the run's linked issue is the same work tab's ISSUE face — its
  // canonical URL, beside the same list (`from` rides along). EXP-893: a
  // `replace` on a phone, where the faces are one screen.
  const openIssue = useCallback(() => {
    if (!board || !issue) return
    void navigate({
      to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
      params: {
        teamSlug,
        boardSlug: board.slug,
        issueIdentifier: issue.identifier,
      },
      search: from ? { from } : {},
      replace: isMobile,
    })
  }, [navigate, teamSlug, board, issue, from, isMobile])

  // EXP-877/1251: the Run face and the Guide are the same URL with
  // `?view=` — a replace, so Back still leaves the run rather than stepping
  // through faces. A section page pushes on md+ (Back returns to the Guide).
  const onFace = useCallback(
    (next: `run` | `guide`, section?: GuideSectionKey) => {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: session.id },
        search:
          next === `guide`
            ? guideSearch({ from, section })
            : from
              ? { from }
              : {},
        replace: isMobile || section === undefined,
      })
    },
    [navigate, teamSlug, session.id, from, isMobile]
  )

  // EXP-886: the issue's runs of mine — the header's Run/Runs label and the
  // switcher between them. Picking one is the same navigation the Run face
  // makes (`from` rides along); the tab's Run face follows the URL. EXP-893:
  // on a phone the run swaps IN PLACE (a replace). EXP-974: an ISSUE-LESS
  // run lists its RESUME CHAIN instead — a resumed chat and the run it came
  // out of wear one toggle, and the `Runs` caret is how the reader moves
  // between them (the continuation band above the transcript is gone).
  const { runs: issueRuns } = useIssueRuns(issue?.id, team.id, currentUserId)
  const { runs: chainRuns } = useRunChain(
    issue ? undefined : session.id,
    team.id,
    currentUserId
  )
  const menuRuns = issue ? issueRuns : chainRuns
  const openRun = useCallback(
    (target: CodingSession) => {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: target.id },
        search: from ? { from } : {},
        replace: isMobile,
      })
    },
    [navigate, teamSlug, from, isMobile]
  )

  // EXP-893/1251: the Guide's diff without a live diff — the issue's PR
  // files (a batch run's covered issue that shares the run's PR carries
  // them), read while the Guide shows (a live diff still outranks them).
  const prIssue =
    issue ??
    row.batchIssues.find(
      (covered) => covered.prUrl != null && covered.prUrl === session.prUrl
    ) ??
    null
  const { state: prFilesState } = useReviewFiles(prIssue, {
    enabled: face === `guide`,
  })
  // EXP-1194: a run with no issue to key its PR on (a chat or action run's
  // chore PR) reads it by the run (Reviews opens its Guide here).
  const { state: runPrFilesState } = useSessionPrFiles(
    prIssue ? null : session,
    { enabled: face === `guide` }
  )
  const shownPrFilesState = prIssue ? prFilesState : runPrFilesState
  const prFiles =
    shownPrFilesState.kind === `files` ? shownPrFilesState.files : null
  const prUrl = prIssue?.prUrl ?? session.prUrl ?? null

  // EXP-1248: the Guide's Stack card — the PR's stack, a member REPLACES the
  // subject in place (its issue's Guide); hovering one merges through it.
  const { stack, memberTarget } = useIssueStack(prIssue, team.id)
  const mergeThrough = useMergeThrough()
  const guideStack = stack ? (
    <GuideStackCard
      stack={stack}
      onOpen={(issueId) => {
        const target = memberTarget(issueId)
        if (!target) return
        void navigate({
          to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
          params: {
            teamSlug,
            boardSlug: target.boardSlug,
            issueIdentifier: target.identifier,
          },
          search: guideSearch({ from }),
          replace: true,
        })
      }}
      onMergeThrough={prIssue && permissions.canMutateIssue(prIssue) ? mergeThrough.request : undefined}
    />
  ) : undefined

  // EXP-893: the switcher's `Start coding` row once this run ended for good
  // — a new run on the issue, through the Agent page composer.
  const openComposer = useOpenComposer()
  const onStart = issue
    ? () => openComposer({ issueIds: [issue.id] })
    : undefined

  const issueHeader =
    issue && board
      ? {
          // EXP-1162: the run face's bar is always collapsed — the title is
          // edited on the Issue face, where it is a row of the body.
          title: (
            <CollapsedTitle
              identifier={issue.identifier}
              title={issue.title}
              animate={false}
            />
          ),
          trailing: (
            <>
              <PinToggleButton
                teamId={team.id}
                kind="issue"
                targetId={issue.id}
                variant="ghost"
              />
              <IssueActionsMenu
                issue={issue}
                board={board}
                teamSlug={teamSlug}
                readOnly={readOnly}
              />
            </>
          ),
          tray: (
            <IssuePropertiesTray
              issue={issue}
              board={board}
              users={users}
              teamId={team.id}
              currentUserId={currentUserId}
              readOnly={readOnly}
              handlers={handlers}
              preferredSessionId={session.id}
            />
          ),
        }
      : undefined

  // EXP-893: an issue subject's phone header — the SAME bar the issue face
  // wears, with Stop / Resume in its trailing slot on the Run face only
  // (`IssueCodingAction` without its Start capsule: the bar's circle owns
  // Start). EXP-1150: the face tabs ride under it.
  const renderMobileHeader =
    issue && board
      ? ({
          shownFace,
          tabs,
        }: {
          shownFace: `run` | `guide`
          tabs: ReactNode
        }) => (
          <IssueMobileHeader
            issue={issue}
            board={board}
            teamSlug={teamSlug}
            teamId={team.id}
            readOnly={readOnly}
            origin={from}
            handlers={handlers}
            tabs={tabs}
            /* EXP-934: a session route never shows the ISSUE face (that is the
               issue's own URL), so the `…` never belongs in this bar — only
               Stop / Resume do. */
            face={shownFace}
            graphBadge={
              /* EXP-897: the same badge the md+ header wears. */
              <PrGraphBadge
                teamId={team.id}
                teamSlug={teamSlug}
                issue={issue}
                session={session}
              />
            }
            action={
              shownFace === `run` ? (
                <IssueCodingAction
                  issue={issue}
                  board={board}
                  teamId={team.id}
                  currentUserId={currentUserId}
                  preferredSessionId={session.id}
                  showStart={false}
                />
              ) : shownFace === `guide` && prUrl ? (
                /* EXP-949: GitHub belongs to the Guide alone — the same
                   action slot the issue route's Guide fills. */
                <PrGithubButton prUrl={prUrl} />
              ) : undefined
            }
          />
        )
      : undefined

  return (
    // EXP-1112: on a phone the shell gives the outlet no definite height (the
    // window scrolls, `app-shell.ts`), so `h-full` collapsed to the content
    // and the feed — `overscroll-contain`, never overflowing — swallowed every
    // swipe: the newest rows and the strips under them stayed below the fold.
    // The Run face is a fixed-height screen, so it takes the DYNAMIC viewport
    // here (never `100vh`, the large viewport behind iOS's toolbar) and the
    // feed becomes its scroller again; md+ keeps filling the panel's outlet.
    <div className="flex h-dvh min-h-0 flex-col md:h-full">
      {/* The run may END while this page is open — the view stays mounted and
          read-only, and its work tab stays until closed (EXP-870). Nothing
          navigates away underneath the user. EXP-877: Resume lives in the
          header's coding action; the feed connects to the relay exactly like a
          live one and the device republishes its journal. */}
      <AgentSessionView
        session={session}
        currentUserId={currentUserId}
        identity={sessionIdentity(row)}
        mergeTarget={row.mergeTarget}
        banner={
          /* What a run below this one is asking the person. */
          <EscalationBand
            session={session}
            teamId={team.id}
            currentUserId={currentUserId}
          />
        }
        face={face}
        guideSection={guideSection}
        diffFile={diffFile}
        guideStack={guideStack}
        onFace={onFace}
        onIssueFace={issue && board ? openIssue : undefined}
        issueHeader={issueHeader}
        issueRuns={menuRuns}
        onOpenRun={openRun}
        onStart={onStart}
        prFiles={prFiles}
        prUrl={prUrl}
        graphBadge={
          /* EXP-1079: the ONE node feeds the phone's bar and the md+ work
             header. */
          <PrGraphBadge
            teamId={team.id}
            teamSlug={teamSlug}
            issue={issue}
            session={session}
          />
        }
        renderMobileHeader={renderMobileHeader}
        onBack={onBack}
      />
      {handlers.duplicatePicker}
      {mergeThrough.dialog}
    </div>
  )
}

/** EXP-1154/1251: a teammate's run on `?view=guide`: the report the owner
 *  sees (`coding_sessions.results`, synced team-wide) over the run's PR
 *  files (`codingSessions.prFiles`), read-only under the stub's header with
 *  the GitHub button; a Changes row opens its section page. No face toggle
 *  and no view mount, so no ticket is minted. */
export function TeammateRunGuide({
  session,
  title,
  section,
  file,
  onSection,
  onBack,
}: {
  session: CodingSession
  title: string
  section?: GuideSectionKey
  file: string | null
  onSection: (section: GuideSectionKey | undefined) => void
  onBack: () => void
}) {
  const isMobile = useIsMobile()
  const groups = useMemo(
    () => parseSessionResultGroups(session.results),
    [session.results]
  )
  const { state, reload } = useSessionPrFiles(session, {
    enabled: Boolean(session.prUrl),
  })
  const files = guideFilesOf(state)
  const [selected, setSelected] = useState<string | null>(file)
  const page =
    section !== undefined ? guideSectionPage(groups, files, section) : null
  const empty = groups.length === 0 && !session.prUrl
  return (
    <div className="flex h-full min-h-0 flex-col">
      <MobileDetailHeader
        title={title}
        onBack={onBack}
        menu={session.prUrl ? <PrGithubButton prUrl={session.prUrl} /> : undefined}
      />
      {empty ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center">
          <p className="text-sm text-muted-foreground">
            This run has no report yet.
          </p>
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card/40">
          <div className={WORK_COLUMN_CLASS}>
            {page ? (
              <GuideSectionDiff
                page={page}
                selected={selected}
                onSelect={setSelected}
                onBack={() => onSection(undefined)}
                isMobile={isMobile}
              />
            ) : (
              <GuideBody
                groups={groups}
                files={files}
                filesState={session.prUrl ? state : null}
                onOpenChanges={(target) => onSection(target.section)}
                onRetry={reload}
                renderText={renderResultText}
              />
            )}
          </div>
        </div>
      )}
    </div>
  )
}

/** The header the non-view states carry — the AgentSessionView draws its own.
 *  EXP-851: the shared detail header, the same bar every detail wears. */
function SessionStubHeader({
  onBack,
  title,
}: {
  onBack: () => void
  title: string
}) {
  return <MobileDetailHeader title={title} onBack={onBack} />
}

/** A run deeper in the tree asked the PERSON a question
 *  (`exponential_sessions_ask_parent({ to: 'user' })` sets the child's
 *  `needs_input` + `agent_caption` and sends an `agent_message`). The ROOT run
 *  is where the person is watching, so the question surfaces there — one row
 *  per waiting descendant, opening that run's own composer to answer it.
 *
 *  Root runs only: on a child, the band would repeat its own question. */
function EscalationBand({
  session,
  teamId,
  currentUserId,
}: {
  session: CodingSession
  teamId: string
  currentUserId: string
}) {
  const openSession = useOpenSession()
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      query
        .from({ s: codingSessionCollection })
        .where(({ s }) =>
          and(eq(s.teamId, teamId), eq(s.userId, currentUserId))
        ),
    [teamId, currentUserId]
  )
  const sessions = useMemo(
    () => (sessionRows ?? []) as CodingSession[],
    [sessionRows]
  )
  const waiting = useMemo(() => {
    const ids = new Set(sessionDescendantIds(sessionTree(sessions), session.id))
    return sessions.filter((row) => ids.has(row.id) && row.needsInput)
  }, [sessions, session.id])
  const askedIds = useMemo(
    () =>
      [...new Set(waiting.map((row) => row.issueId).filter((id): id is string => id !== null))].sort(),
    [waiting]
  )
  const { data: issueRows } = useLiveQuery(
    (query) =>
      askedIds.length > 0
        ? query
            .from({ ai: issueCollection })
            .where(({ ai }) => inArray(ai.id, askedIds))
        : undefined,
    [askedIds.join(`,`)]
  )
  if (session.parentSessionId || waiting.length === 0) return null
  const byId = new Map(((issueRows ?? []) as Issue[]).map((row) => [row.id, row]))
  return (
    <div
      className="flex shrink-0 flex-col gap-1 border-b border-border bg-card/40 px-3 py-1.5 text-[11px] text-amber-400"
      data-testid="escalation-band"
    >
      {waiting.map((child) => {
        const issue = child.issueId ? byId.get(child.issueId) : undefined
        const name = issue?.identifier ?? child.id.slice(0, 8)
        return (
          <Button
            key={child.id}
            variant="link"
            size="inline"
            className="min-w-0 justify-start truncate text-left text-current"
            onClick={() => openSession(child)}
          >
            {`Run ${name} asks: ${child.agentCaption ?? ``}`.trimEnd()}
          </Button>
        )
      })}
    </div>
  )
}
