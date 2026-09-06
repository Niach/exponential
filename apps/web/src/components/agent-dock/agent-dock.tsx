import { useEffect } from "react"
import {
  useMatchRoute,
  useNavigate,
  useParams,
  useSearch,
} from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import { rowPrState, useAgentsData, type AgentSessionRow } from "@/hooks/use-agents-data"
import { sessionDisplayState } from "@/components/issue-coding-rows"
import { RichTab } from "@/components/rich-tab"
import { Button } from "@/components/ui/button"
import { retainSteerSessions } from "@/lib/steer-session-store"
import {
  isChatSession,
  sessionIdentity,
  tabPhaseLabel,
} from "@/lib/session-identity"
import { useIsMobile } from "@/hooks/use-mobile"
import { useKillSession } from "@/hooks/use-kill-session"
import { useOpenSession } from "@/hooks/use-open-session"
import { useChromeHeightVar } from "@/hooks/use-chrome-height-var"

// EXP-740: the agent dock is the STRIP and nothing else — an IDE-style row of
// tabs for the current user's OWN running sessions (EXP-312: live sessions are
// owner-only; teammates' runs surface as status badges elsewhere), plus a
// trailing Chat glyph. There is no panel here any more: selecting a tab
// NAVIGATES to `/t/$teamSlug/sessions/$sessionId` (or, for a chat run, to the
// team's one `/t/$teamSlug/chat` page), which mounts the AgentSessionView
// full-height in the content panel. That killed the drag-resize, the 85 vh
// cap, the fullscreen toggle and the mobile takeover along with it.
//
// The strip is the team panel's LAST flex child at a fixed `h-9`, so it needs
// no `sticky`/`z-*`/glass of its own; it renders even with zero sessions,
// because the Chat button is always reachable.
// Desktop-only chrome (EXP-193): phones get no strip at all — sessions are
// reached from the Devices tab, the issue's Watch button and the chat FAB.

const ActionChatIcon = conceptIcon(`action-chat`)

function RunningDot() {
  return (
    <span className="relative flex size-2 shrink-0">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

export function AgentDock({
  teamId,
  teamSlug,
  currentUserId,
}: {
  teamId: string
  teamSlug: string
  currentUserId: string
}) {
  // EXP-312: only the caller's own sessions get dock tabs (a teammate's
  // session can't be opened live, the ticket mint refuses non-owners), and
  // useAgentsData already scopes its rows to the caller.
  const { running } = useAgentsData(teamId, currentUserId)
  const isMobile = useIsMobile()
  const navigate = useNavigate()
  // The session the route is showing — the tab's active state, and one more
  // store the reaper below must keep alive.
  const { sessionId: routeSessionId } = useParams({ strict: false })
  // EXP-739: `/chat?session=<id>` names WHICH chat that page is showing; with
  // no `session` it shows the newest running one, so the strip has to agree.
  const search = useSearch({ strict: false }) as { session?: string }
  const chatSearchId = search.session ?? null

  // EXP-698: the strip still publishes its measured height for a page that
  // owns a viewport-sized scroller of its own and therefore really does run
  // under it. No consumer today; the property is cheap and the alternative is
  // re-deriving `h-9` in CSS somewhere else.
  const publishDockHeight = useChromeHeightVar(`--dock-h`)

  // EXP-621: the reaper for background steer connections — stores for the
  // user's running sessions (and whatever the route is showing) are kept alive
  // so navigating back to a session resumes instantly; anything else ages out
  // inside the registry (grace-delayed, so a transiently empty live query
  // can't kill a socket).
  useEffect(() => {
    const keep = new Set(running.map((row) => row.session.id))
    if (routeSessionId) keep.add(routeSessionId)
    retainSteerSessions(keep)
  }, [running, routeSessionId])

  // `running` is newest-first, so the head chat is the one `/chat` defaults to.
  const newestChatId =
    running.find((row) => isChatSession(row.session))?.session.id ?? null

  if (isMobile) return null

  return (
    <div
      ref={publishDockHeight}
      className="flex h-9 shrink-0 items-center gap-1 overflow-x-auto border-t border-border/60 px-2"
    >
      <div role="tablist" className="flex min-w-0 items-center gap-1">
        {running.map((row) => (
          <DockTab
            key={row.session.id}
            row={row}
            currentUserId={currentUserId}
            routeSessionId={routeSessionId ?? null}
            chatSearchId={chatSearchId}
            newestChatId={newestChatId}
          />
        ))}
      </div>
      {/* EXP-739: chat is always one click away, whatever is running — the
          IDE dock strip carries the same glyph in the same slot. */}
      <Button
        variant="ghost"
        size="icon"
        className="ml-auto size-6 shrink-0"
        aria-label="Chat"
        title="Chat"
        onClick={() =>
          void navigate({ to: `/t/$teamSlug/chat`, params: { teamSlug } })
        }
      >
        <ActionChatIcon className="size-4" />
      </Button>
    </div>
  )
}

function DockTab({
  row,
  currentUserId,
  routeSessionId,
  chatSearchId,
  newestChatId,
}: {
  row: AgentSessionRow
  currentUserId: string
  /** The `$sessionId` the route is on, if any. */
  routeSessionId: string | null
  /** `?session=` on the chat route, if any. */
  chatSearchId: string | null
  /** The chat `/chat` falls back to when `?session=` is absent. */
  newestChatId: string | null
}) {
  const { session, device, paused } = row
  const identity = sessionIdentity(row)
  const isChat = isChatSession(session)
  const matchRoute = useMatchRoute()
  const openSession = useOpenSession()
  // A chat run has no session route of its own — the team's ONE chat page is
  // where it is steered, so this tab is active when that page is open AND
  // showing THIS chat: either named by `?session=`, or the newest running one
  // when nothing is named.
  const active = isChat
    ? Boolean(matchRoute({ to: `/t/$teamSlug/chat` })) &&
      (chatSearchId === session.id ||
        (chatSearchId === null && newestChatId === session.id))
    : routeSessionId === session.id
  // EXP-688: the X ends a live run of your own (with the same confirmation
  // the session view's "…" menu shows). A finished or paused row has nothing
  // left to stop and the tab is not a panel toggle any more, so it simply
  // carries no X.
  const { canKill, requestKill, dialog } = useKillSession(
    session,
    currentUserId,
    device.label,
    paused
  )
  return (
    <>
      <RichTab
        active={active}
        // EXP-550: the host machine is offline — the agent is parked, the
        // tab greys out instead of pinging "live".
        paused={paused}
        status={tabStatus(row)}
        icon={
          isChat ? (
            <ActionChatIcon className="size-3.5 shrink-0" aria-hidden />
          ) : undefined
        }
        identifier={identity.identifier}
        title={
          identity.identifier
            ? identity.subject
            : // No identifier to lead with: the action's name (or "Batch")
              // takes the mono slot on its own, as it always has.
              (session.actionName ??
              (session.issueId ? identity.subject : `Batch`))
        }
        badge={
          device.label && (
            <span className="max-w-[8rem] shrink-0 truncate text-foreground/50">
              {` · ${device.label}`}
            </span>
          )
        }
        tooltip={tabPhaseLabel(row)}
        onSelect={() => openSession(session)}
        onClose={canKill ? () => requestKill() : undefined}
        closeLabel="Kill session"
      />
      {dialog}
    </>
  )
}

/** The tab's 6px state dot — a live ping while the agent runs, otherwise the
 * EXP-214 display split: needs-input amber beats everything, a merged PR
 * renders blue, review stays green, anything parked greys out. */
function tabStatus(row: AgentSessionRow) {
  const { session, issue, paused } = row
  if (paused) return `bg-muted-foreground/40`
  if (session.status !== `running` && session.status !== `in_review`) {
    return `bg-muted-foreground/40`
  }
  const state = sessionDisplayState(session, rowPrState(session, issue))
  if (state === `running`) return <RunningDot />
  if (state === `needs_input`) return `bg-amber-500`
  if (state === `done`) return `bg-sky-500`
  return `bg-emerald-500`
}
