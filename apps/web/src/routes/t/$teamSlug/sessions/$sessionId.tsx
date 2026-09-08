import { useCallback, useMemo, useState } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useCanGoBack,
  useNavigate,
  useRouter,
} from "@tanstack/react-router"
import { useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { AgentSessionView } from "@/components/agent-session"
import { agentLabel } from "@/components/agent-usage-bar"
import { relativeTime } from "@/components/comment-rows/format"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { MarkdownEditor } from "@/components/issue-editor/markdown-editor"
import { Button } from "@/components/ui/button"
import { conceptIcon } from "@/lib/icons.generated"
import { deviceCollection } from "@/lib/collections"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"
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
  const router = useRouter()
  const canGoBack = useCanGoBack()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)
  const { data: authSession } = useSession()
  const currentUserId = authSession?.user?.id

  const { row, session, isReady } = useSessionRow(
    team?.id,
    currentUserId,
    sessionId
  )

  // Back is the browser's back wherever there is history to pop (the session
  // was opened FROM somewhere — an issue, the strip, Devices); a cold deep
  // link falls back to Devices, which lists every run.
  const goBack = useCallback(() => {
    if (canGoBack) {
      router.history.back()
      return
    }
    void navigate({ to: `/t/$teamSlug/devices`, params: { teamSlug } })
  }, [canGoBack, router, navigate, teamSlug])

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
            <Link to="/t/$teamSlug/devices" params={{ teamSlug }}>
              Go to Devices
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

  const byline = pastRunByline(session, {
    deviceLabel: device.label ?? session.deviceLabel,
    agentLabel: session.agent ? agentLabel(session.agent) : null,
    relativeTime:
      pastRunEndedAt(session) > 0
        ? relativeTime(new Date(pastRunEndedAt(session)))
        : ``,
  })

  // The resumed run arrives as a NEW row over Electric; the button only has to
  // send the command, so it settles as soon as the relay accepted it.
  const resume = async () => {
    if (!session.deviceId) return
    setResuming(true)
    try {
      await trpc.steer.startSession.mutate({
        resumeSessionId: session.id,
        deviceId: session.deviceId,
      })
    } catch (e) {
      toast.error(e instanceof Error ? e.message : `Could not resume that run`)
    } finally {
      setResuming(false)
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
