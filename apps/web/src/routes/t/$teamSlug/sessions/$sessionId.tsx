import { useCallback, useMemo } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { and, eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { AgentSessionView } from "@/components/agent-session"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { IssueActionsMenu } from "@/components/issue-actions-menu"
import { IssueCodingAction } from "@/components/issue-coding-action"
import { IssueMobileHeader } from "@/components/issue-mobile-header"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"
import { IssueTitleField } from "@/components/issue-title-field"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { PrGraphBadge } from "@/components/pr-graph-badge"
import { Button, PrGithubButton, useIsMobile, type SessionDotTone } from "@exp/ui"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import type { WorkFace } from "@/components/team/work-face-toggle"
import { codingSessionCollection, issueCollection } from "@/lib/collections"
import { descendantIds, nestSessions } from "@/lib/session-tree"
import { stackPosition, stackPositionLine } from "@/lib/pr-stack"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useOpenSession } from "@/hooks/use-open-session"
import { useReviewFiles } from "@/hooks/use-review-files"
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
// face. EXP-877: `?view=diff` is the run's third face, the full-column diff
// under the same header (`work-header.tsx`); EXP-879 `?view=results` is the
// fourth, the screenshots the run published. EXP-893: on a phone the faces
// are the Work screen's — `replace` navigations, so Back leaves the subject.
//
// EXP-312: a LIVE session is visible and steerable by its OWNER alone (the
// relay ticket mint refuses everyone else). A teammate's session id therefore
// renders an identity stub with the synced status badge and NO view mount —
// mounting it would try to mint a ticket the server will refuse.

type SessionSearch = { from?: string; view?: `diff` | `results` }

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  head: () => ({ meta: [{ title: pageTitle(`Agent`) }] }),
  // EXP-818: `?from=` is WHERE this run was opened from (`lib/detail-origin.ts`
  // — the desktop's `derive_origin`), so Back returns to that list instead of
  // always landing on the Agent page. Absent = the Agent page's own list.
  validateSearch: (search: Record<string, unknown>): SessionSearch => ({
    from: typeof search.from === `string` && search.from ? search.from : undefined,
    // EXP-879: `?view=results` is the fourth face; anything else normalises
    // away to the run itself.
    view:
      search.view === `diff` || search.view === `results`
        ? search.view
        : undefined,
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
  const { from, view } = Route.useSearch()
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
      face={view === `diff` || view === `results` ? view : `run`}
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
  onBack,
}: {
  team: Team
  teamSlug: string
  row: AgentSessionRow
  session: CodingSession
  currentUserId: string
  from: string | undefined
  face: WorkFace
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

  // EXP-877: the run and diff faces are the same URL with `?view=` — a
  // replace, so Back still leaves the run rather than stepping through faces.
  const onFace = useCallback(
    (next: WorkFace) => {
      if (next === `issue`) {
        openIssue()
        return
      }
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: session.id },
        search: {
          ...(from ? { from } : {}),
          ...(next === `diff` || next === `results` ? { view: next } : {}),
        },
        replace: true,
      })
    },
    [navigate, teamSlug, session.id, from, openIssue]
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

  // EXP-893: the phone's Changes face without a live diff — the issue's PR
  // files (a batch run's representative issue carries the PR). EXP-952: read
  // as soon as a phone has an OPEN PR to count, not only on the face — the
  // switcher's Changes row prints the same `+N −M` the face draws (a live
  // diff still outranks them in the view).
  const prIssue =
    issue ?? (row.mergeTarget?.kind === `issue` ? row.mergeTarget.issue : null)
  const { state: prFilesState } = useReviewFiles(prIssue, {
    enabled: isMobile && (face === `diff` || prIssue?.prState === `open`),
  })
  const prFiles =
    prFilesState.kind === `files` ? prFilesState.files : null
  const prUrl = prIssue?.prUrl ?? session.prUrl ?? null

  // EXP-893: the switcher's `Start coding` row once this run ended for good
  // — a new run on the issue, through the Agent page composer.
  const openComposer = useOpenComposer()
  const onStart = issue
    ? () => openComposer({ issueIds: [issue.id] })
    : undefined

  const issueHeader =
    issue && board
      ? {
          title: <IssueTitleField issue={issue} readOnly={readOnly} />,
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
  // Start).
  const renderMobileHeader =
    issue && board
      ? ({
          dot,
          shownFace,
        }: {
          dot: { tone: SessionDotTone; connecting: boolean }
          shownFace: `run` | `changes` | `results`
        }) => (
          <IssueMobileHeader
            issue={issue}
            board={board}
            teamSlug={teamSlug}
            teamId={team.id}
            readOnly={readOnly}
            origin={from}
            handlers={handlers}
            dot={dot}
            /* EXP-934: a session route never shows the ISSUE face (that is the
               issue's own URL), so the `…` never belongs in this bar — only
               Stop / Resume do. */
            face={shownFace}
            graphBadge={
              /* EXP-897: the same stacked chip the md+ header wears — the face
                 showing decides which section its sheet opens on. */
              <PrGraphBadge
                teamId={team.id}
                teamSlug={teamSlug}
                face={shownFace === `run` ? `run` : `changes`}
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
              ) : shownFace === `changes` && prUrl ? (
                /* EXP-949: GitHub belongs to the Changes face alone — the
                   same action slot the issue route's Changes face fills. */
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
          <>
            {/* EXP-897: where this run's pull request sits in its stack, and
                what a run below it is asking the person. */}
            {issue && <StackPositionBand issue={issue} teamSlug={teamSlug} />}
            <EscalationBand
              session={session}
              teamId={team.id}
              currentUserId={currentUserId}
            />
          </>
        }
        face={face}
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
             header (EXP-1058: the stacked issue chip in both). */
          <PrGraphBadge
            teamId={team.id}
            teamSlug={teamSlug}
            face={face === `diff` ? `changes` : `run`}
            issue={issue}
            session={session}
          />
        }
        renderMobileHeader={renderMobileHeader}
        onBack={onBack}
      />
      {handlers.duplicatePicker}
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

/** EXP-897: where this run's pull request sits in its STACK. One quiet line —
 *  `2 of 3 · on top of #ABC-12` — with the members below and above linking to
 *  their review pages. Absent when the issue is in no stack. Renders on the
 *  phone too: it rides the view's `banner`, not the md+ header. */
function StackPositionBand({
  issue,
  teamSlug,
}: {
  issue: Issue
  teamSlug: string
}) {
  const { data: issueRows } = useLiveQuery(
    (query) =>
      query
        .from({ i: issueCollection })
        .where(({ i }) => eq(i.teamId, issue.teamId)),
    [issue.teamId]
  )
  const at = useMemo(
    () => stackPosition(issue, (issueRows ?? []) as Issue[]),
    [issue, issueRows]
  )
  if (!at) return null
  return (
    <div
      className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-card/40 px-3 py-1.5 text-[11px] text-muted-foreground"
      data-testid="stack-position-band"
    >
      <span>{stackPositionLine(at.position, at.size, at.below?.identifier ?? null)}</span>
      {at.below && (
        <Button variant="link" size="inline" asChild className="font-mono">
          <Link
            to="/t/$teamSlug/reviews/$issueIdentifier"
            params={{ teamSlug, issueIdentifier: at.below.identifier }}
          >
            {`↓ #${at.below.identifier}`}
          </Link>
        </Button>
      )}
      {at.above && (
        <Button variant="link" size="inline" asChild className="font-mono">
          <Link
            to="/t/$teamSlug/reviews/$issueIdentifier"
            params={{ teamSlug, issueIdentifier: at.above.identifier }}
          >
            {`↑ #${at.above.identifier}`}
          </Link>
        </Button>
      )}
    </div>
  )
}

/** EXP-897: a run deeper in the tree asked the PERSON a question
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
    const ids = new Set(descendantIds(nestSessions(sessions), session.id))
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
