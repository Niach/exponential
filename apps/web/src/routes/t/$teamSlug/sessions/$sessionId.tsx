import { useCallback, useMemo } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import { AgentSessionView } from "@/components/agent-session"
import { relativeTime } from "@/components/comment-rows/format"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { IssueActionsMenu } from "@/components/issue-actions-menu"
import { IssueRunSwitcher } from "@/components/issue-run-switcher"
import { IssuePropertiesTray } from "@/components/issue-properties-tray"
import { IssueTitleField } from "@/components/issue-title-field"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { Button } from "@/components/ui/button"
import { MobileDetailHeader } from "@/components/team/mobile-detail-header"
import type { WorkFace } from "@/components/team/work-face-toggle"
import { codingSessionCollection } from "@/lib/collections"
import {
  CONTINUATION_COST_NOTE,
  CONTINUATION_NOTE,
} from "@/components/session-account-switch"
import { originListNavigation, parseOrigin } from "@/lib/detail-origin"
import { useOpenSession } from "@/hooks/use-open-session"
import type { Board, CodingSession, Issue, Team } from "@/db/schema"
import {
  rowPrState,
  useIssueRuns,
  useSessionRow,
  type AgentSessionRow,
} from "@/hooks/use-agents-data"
import { useIssuePropertyHandlers } from "@/hooks/use-issue-property-handlers"
import { sessionIdentity } from "@/lib/session-identity"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// EXP-740: one coding session, FULLSCREEN on its own route — the web twin of
// the desktop IDE's `Screen::Session` center tab and the natives' pushed
// Agent-session screen. EXP-851: the view fills the whole content panel on
// every breakpoint — no shell, no list beside it. The list the run came from
// is the SIDEBAR's job now (`?from=`, `lib/detail-origin.ts`). EXP-870: this
// is the ONE run URL — an issue's run too, whose issue is the work tab's other
// face. EXP-877: `?view=diff` is the run's third face, the full-column diff
// under the same header (`work-header.tsx`).
//
// EXP-312: a LIVE session is visible and steerable by its OWNER alone (the
// relay ticket mint refuses everyone else). A teammate's session id therefore
// renders an identity stub with the synced status badge and NO view mount —
// mounting it would try to mint a ticket the server will refuse.

type SessionSearch = { from?: string; view?: `diff` }

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  // EXP-818: `?from=` is WHERE this run was opened from (`lib/detail-origin.ts`
  // — the desktop's `derive_origin`), so Back returns to that list instead of
  // always landing on the Agent page. Absent = the Agent page's own list.
  validateSearch: (search: Record<string, unknown>): SessionSearch => ({
    from: typeof search.from === `string` && search.from ? search.from : undefined,
    view: search.view === `diff` ? `diff` : undefined,
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
      face={view === `diff` ? `diff` : `run`}
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
  const issue: Issue | null = row.issue ?? null
  const board: Board | null = row.board ?? null
  const permissions = useTeamPermissions(team)
  const { users } = useTeamUsers(team.id)
  const readOnly = issue ? !permissions.canMutateIssue(issue) : true
  const handlers = useIssuePropertyHandlers({ issue, teamSlug, readOnly })

  // EXP-870: the run's linked issue is the same work tab's ISSUE face — its
  // canonical URL, beside the same list (`from` rides along).
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
    })
  }, [navigate, teamSlug, board, issue, from])

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
          ...(next === `diff` ? { view: `diff` as const } : {}),
        },
        replace: true,
      })
    },
    [navigate, teamSlug, session.id, from, openIssue]
  )

  // EXP-886: the issue's runs of mine — the header's Run/Runs label and the
  // switcher between them. Picking one is the same navigation the Run face
  // makes (`from` rides along); the tab's Run face follows the URL.
  const { runs: issueRuns } = useIssueRuns(issue?.id, team.id, currentUserId)
  const openRun = useCallback(
    (target: CodingSession) => {
      void navigate({
        to: `/t/$teamSlug/sessions/$sessionId`,
        params: { teamSlug, sessionId: target.id },
        search: from ? { from } : {},
      })
    },
    [navigate, teamSlug, from]
  )

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

  return (
    <div className="flex h-full min-h-0 flex-col">
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
          // EXP-849: a run that was CONTINUED (an account switch, a resume)
          // names the run before and after it, so the chain reads as one
          // conversation instead of three orphans.
          <SessionContinuationBand session={session} />
        }
        face={face}
        onFace={onFace}
        onIssueFace={issue && board ? openIssue : undefined}
        issueHeader={issueHeader}
        runSwitcher={
          issue ? (
            <IssueRunSwitcher
              runs={issueRuns}
              viewedRunId={session.id}
              onOpen={openRun}
            />
          ) : undefined
        }
        multipleRuns={issueRuns.length > 1}
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

/** EXP-849: the continuation chain — `resumed_from_id` links the run a
 * switch or resume came out of to the one that took over. One quiet line with
 * both ends, each opening that run. Absent when this run is neither.
 *
 * The backward link carries the ×4 sentence (`CONTINUATION_NOTE`) plus the
 * one-time transcript re-read it cost, said ONCE on the run that paid it — a
 * second context charge on a new account must never be a surprise. */
function SessionContinuationBand({ session }: { session: CodingSession }) {
  const openSession = useOpenSession()
  const { data: sessionRows } = useLiveQuery((query) =>
    query.from({ s: codingSessionCollection })
  )
  const rows = (sessionRows ?? []) as CodingSession[]
  const from = session.resumedFromId
    ? (rows.find((row) => row.id === session.resumedFromId) ?? null)
    : null
  const next = rows.find((row) => row.resumedFromId === session.id) ?? null
  if (!from && !next) return null
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-border bg-card/40 px-3 py-1.5 text-[11px] text-muted-foreground">
      {from && (
        <div className="flex min-w-0 flex-col items-start">
          <button
            type="button"
            className="underline-offset-2 hover:underline"
            onClick={() => openSession(from)}
          >
            {`${CONTINUATION_NOTE} · started ${relativeTime(from.startedAt)}`}
          </button>
          <span className="text-muted-foreground/70">
            {CONTINUATION_COST_NOTE}
          </span>
        </div>
      )}
      {next && (
        <button
          type="button"
          className="underline-offset-2 hover:underline"
          onClick={() => openSession(next)}
        >
          {`Continues in a newer run · started ${relativeTime(next.startedAt)}`}
        </button>
      )}
    </div>
  )
}
