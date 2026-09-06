import { useCallback } from "react"
import {
  createFileRoute,
  Link,
  redirect,
  useCanGoBack,
  useNavigate,
  useRouter,
} from "@tanstack/react-router"
import { AgentSessionView } from "@/components/agent-session"
import { SessionStatusBadge } from "@/components/issue-coding-rows"
import { Button } from "@/components/ui/button"
import { conceptIcon } from "@/lib/icons.generated"
import { rowPrState, useSessionRow } from "@/hooks/use-agents-data"
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
          excludes ended rows). Nothing navigates away underneath the user. */}
      <AgentSessionView
        key={session.id}
        session={session}
        currentUserId={currentUserId}
        identity={identity}
        mergeTarget={row.mergeTarget}
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
