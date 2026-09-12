import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { AgentSessionView } from "@/components/agent-session"
import { AgentShell } from "@/components/agent-shell"
import { InboxView } from "@/components/inbox/inbox-view"
import { relativeTime } from "@/components/comment-rows/format"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { Button } from "@/components/ui/button"
import { conceptIcon } from "@/lib/icons.generated"
import {
  codingSessionCollection,
  deviceCollection,
} from "@/lib/collections"
import { parseOrigin } from "@/lib/detail-origin"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"
import {
  findStartedRun,
  STARTED_RUN_DEADLINE_MS,
  STARTED_RUN_SKEW_MS,
} from "@/lib/started-run-match"
import { useOpenSession } from "@/hooks/use-open-session"
import {
  deviceCanResumeRun,
  deviceRowIsOnline,
} from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import type { CodingSession, Device } from "@/db/schema"
import { rowPrState, useSessionRow } from "@/hooks/use-agents-data"
import { useNow } from "@/hooks/use-now"
import { useSessionDevice } from "@/hooks/use-session-device"
import { sessionIdentity } from "@/lib/session-identity"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"

// EXP-740: one coding session, FULLSCREEN on its own route — the web twin of
// the desktop IDE's `Screen::Session` center tab and the natives' pushed
// Agent-session screen. The dock strip below only picks which session is here;
// the steering view fills the content panel on every breakpoint.
//
// EXP-312: a LIVE session is visible and steerable by its OWNER alone (the
// relay ticket mint refuses everyone else). A teammate's session id therefore
// renders an identity stub with the synced status badge and NO view mount —
// mounting it would try to mint a ticket the server will refuse.

const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  // EXP-818: `?from=` is WHERE this run was opened from (`lib/detail-origin.ts`
  // — the desktop's `derive_origin`), so Back returns to that list instead of
  // always landing on the Agent page. Absent = the Agent page's own list.
  validateSearch: (search: Record<string, unknown>): { from?: string } => ({
    from: typeof search.from === `string` && search.from ? search.from : undefined,
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
  const { from } = Route.useSearch()
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
  // a board, the issue whose Watch started this (where the Watch button is
  // waiting again) — and to the Agent page when there was no list context
  // (a deep link, a full-page screen). Still a deliberate navigation, not the
  // browser's history: EXP-827 removed that for good reason (the sidebar's
  // Sessions rows navigate from anywhere), the destination is just no longer
  // hard-coded. The browser's own back is untouched.
  const origin = useMemo(() => parseOrigin(from), [from])
  const goBack = useCallback(() => {
    if (origin?.kind === `inbox`) {
      void navigate({ to: `/t/$teamSlug/inbox`, params: { teamSlug }, search: {} })
      return
    }
    if (origin?.kind === `board`) {
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug`,
        params: { teamSlug, boardSlug: origin.boardSlug },
        search: {},
      })
      return
    }
    if (origin?.kind === `issue`) {
      void navigate({
        to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
        params: {
          teamSlug,
          boardSlug: origin.boardSlug,
          issueIdentifier: origin.identifier,
        },
        search: {},
      })
      return
    }
    void navigate({ to: `/t/$teamSlug/agent`, params: { teamSlug } })
  }, [navigate, teamSlug, origin])
  // EXP-827: the linked issue opens IN the shell, beside the sessions list.
  // The origin rides along so the issue's own Back still knows it.
  const openIssue = useCallback(() => {
    void navigate({
      to: `/t/$teamSlug/sessions/$sessionId/issue`,
      params: { teamSlug, sessionId },
      search: from ? { from } : {},
    })
  }, [navigate, teamSlug, sessionId, from])

  if (!team || !currentUserId || !isReady) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <div className="p-6 text-sm text-muted-foreground">Loading…</div>
      </div>
    )
  }

  // EXP-818: every state renders beside a LIST — which one is the origin's
  // call (`lib/detail-origin.ts`, the desktop's tool-column rule): a run
  // opened off the inbox keeps the inbox stream on the left, everything else
  // gets the Agent shell's sessions list. Below md the session is the whole
  // screen either way.
  const shell = (content: React.ReactNode) =>
    origin?.kind === `inbox` ? (
      <div className="flex h-full min-h-0">
        <div className="hidden w-80 shrink-0 flex-col border-r border-border md:flex">
          <InboxView
            teamSlug={teamSlug}
            compact
            activeIssueId={null}
            onOpenIssue={(openedIssue) => {
              void navigate({
                to: `/t/$teamSlug/inbox`,
                params: { teamSlug },
                search: { issue: openedIssue.id },
              })
            }}
          />
        </div>
        <div className="flex min-w-0 flex-1 flex-col">{content}</div>
      </div>
    ) : (
      <AgentShell
        teamId={team.id}
        currentUserId={currentUserId}
        activeSessionId={sessionId}
      >
        {content}
      </AgentShell>
    )

  if (!session || !row) {
    return shell(
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
    return shell(
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

  return shell(
    <div className="flex h-full min-h-0 flex-col">
      {/* The run may END while this page is open — the view stays mounted and
          read-only (its own tab in the strip vanishes, because `running`
          excludes ended rows). Nothing navigates away underneath the user.
          EXP-773: a finished run carries its close-out under the header — the
          byline the Past lists used to expand, Resume on the machine that
          still holds the worktree, and the agent's summary. The feed connects
          to the relay exactly like a live one; the device republishes its
          journal. */}
      <AgentSessionView
        key={session.id}
        session={session}
        currentUserId={currentUserId}
        identity={identity}
        mergeTarget={row.mergeTarget}
        banner={
          session.status === `ended` ? (
            <EndedRunHeader session={session} />
          ) : undefined
        }
        issue={row.issue ?? null}
        onOpenIssue={row.issue ? openIssue : undefined}
        onBack={goBack}
      />
    </div>
  )
}

/** The header the non-view states carry — the AgentSessionView draws its own. */
function SessionStubHeader({
  onBack,
  title,
}: {
  onBack: () => void
  title: string
}) {
  return (
    <div className="flex items-center gap-1 border-b border-border px-1 py-1.5">
      <Button
        variant="ghost"
        size="icon"
        className="shrink-0"
        aria-label="Back"
        onClick={onBack}
      >
        <UiBackIcon />
      </Button>
      <span className="min-w-0 flex-1 truncate text-center text-sm font-medium">
        {title}
      </span>
      {/* Balances the back button so the title stays optically centred. */}
      <span className="size-9 shrink-0" />
    </div>
  )
}

const ResumeIcon = conceptIcon(`run-resume`)

/** EXP-773: the ended-run block above the transcript — the caption the Past
 * rows carry (lib/past-runs.ts `pastRunByline`), Resume when the run's machine
 * is online and advertises `resume-run`, and the agent's own close-out summary
 * as a small muted markdown block. */
function EndedRunHeader({ session }: { session: CodingSession }) {
  const [resuming, setResuming] = useState(false)
  const device = useSessionDevice(session)
  const canResume = useCanResumeOn(session)
  // EXP-818: a resume OPENS the run it started, exactly like a remote start
  // does (`use-remote-start.ts`): the relaunched run arrives as a NEW row over
  // Electric, and leaving the reader on the dead one (with a spinner that
  // never settles) was the whole complaint. `resumedFromId` names it.
  const openSession = useOpenSession()
  const [sentAt, setSentAt] = useState<number | null>(null)
  const { data: sessionRows } = useLiveQuery(
    (query) =>
      sentAt !== null
        ? query.from({ s: codingSessionCollection })
        : undefined,
    [sentAt !== null]
  )
  const deadlineRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(
    () => () => {
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
    },
    []
  )
  useEffect(() => {
    if (sentAt === null) return
    const match = findStartedRun(
      (sessionRows ?? []) as CodingSession[],
      { kind: `resumed`, fromId: session.id },
      session.userId,
      sentAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
    if (deadlineRef.current) clearTimeout(deadlineRef.current)
    setSentAt(null)
    setResuming(false)
    openSession(match)
  }, [sessionRows, sentAt, session.id, session.userId, openSession])

  const byline = pastRunByline({
    deviceLabel: device.label ?? session.deviceLabel,
    relativeTime:
      pastRunEndedAt(session) > 0
        ? relativeTime(new Date(pastRunEndedAt(session)))
        : ``,
  })

  // The command is only half of it: the button stays busy until the new row
  // syncs in and its page opens, and says so if the machine never starts it
  // (the desktop holds the reason — a conflicted worktree, a failed doctor).
  const resume = async () => {
    if (!session.deviceId) return
    setResuming(true)
    try {
      await trpc.steer.startSession.mutate({
        resumeSessionId: session.id,
        deviceId: session.deviceId,
      })
      setSentAt(Date.now())
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
      deadlineRef.current = setTimeout(() => {
        setSentAt(null)
        setResuming(false)
        toast.error(
          `${device.label ?? `That machine`} never started this run`,
          { description: `Open the Exponential desktop app there to see why.` }
        )
      }, STARTED_RUN_DEADLINE_MS)
    } catch (e) {
      setResuming(false)
      toast.error(e instanceof Error ? e.message : `Could not resume that run`)
    }
  }

  return (
    <div className="flex shrink-0 flex-col gap-2 border-b border-border bg-card/40 px-3 py-2">
      <div className="flex min-w-0 items-center gap-3">
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {byline || `This run has ended`}
        </span>
        {canResume && (
          <Button
            variant="outline"
            size="sm"
            className="shrink-0"
            disabled={resuming}
            onClick={resume}
          >
            {resuming ? (
              <LoaderCircle className="animate-spin" />
            ) : (
              <ResumeIcon />
            )}
            Resume
          </Button>
        )}
      </div>
      {session.summary && (
        <div className="max-h-40 overflow-y-auto text-xs text-muted-foreground">
          <MarkdownEditor
            markdown={session.summary}
            editable={false}
            onChange={() => {}}
            // EXP-698: the run summary is feed-sized markdown.
            appearance="chat"
          />
        </div>
      )}
    </div>
  )
}

/** EXP-637: Resume relaunches the run on the machine that still holds its
 * worktree — the button is hidden when that machine is offline or too old to
 * resume, rather than failing after the click. */
function useCanResumeOn(session: CodingSession): boolean {
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const now = useNow(30_000)
  const deviceId = session.deviceId
  const userId = session.userId
  return useMemo(() => {
    if (!deviceId) return false
    const rows = ((deviceRows ?? []) as Device[]).filter(
      (row) => row.deviceId === deviceId
    )
    const row = rows.find((r) => r.userId === userId) ?? rows[0]
    if (!row) return false
    return (
      deviceRowIsOnline(row.lastSeenAt, now) &&
      deviceCanResumeRun({ caps: row.caps ?? [] })
    )
  }, [deviceRows, deviceId, userId, now])
}
