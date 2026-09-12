import { useMemo } from "react"
import { Link, useParams } from "@tanstack/react-router"
import { eq, inArray, useLiveQuery } from "@tanstack/react-db"
import { conceptIcon } from "@/lib/icons.generated"
import { getActionIcon } from "@/lib/board-icons"
import { sessionIdentity } from "@/lib/session-identity"
import {
  sessionAgentCaption,
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
import type { CodingSession, Issue, Pin } from "@/db/schema"
import {
  actionCollection,
  codingSessionCollection,
  issueCollection,
} from "@/lib/collections"
import { RunningIndicator } from "@/components/agent-session-row"
import { rowPrState } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useTeamPins } from "@/hooks/use-pins"
import { useTeamBoards } from "@/hooks/use-team-data"
import { trpc } from "@/lib/trpc-client"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// EXP-778: the sidebar's Pinned group — the caller's favourites in this team
// (issues, coding sessions, actions), in pin order. A row renders only when
// its target is synced: the pins shape is per user and never trash-scoped,
// so an issue on a trashed board (or a session of a team the user left)
// simply has no row until it comes back. Sits between the nav entries and
// Boards, hidden when empty; hovering a row offers Unpin.

const UiUnpinIcon = conceptIcon(`ui-unpin`)
const NavIssuesIcon = conceptIcon(`nav-issues`)

export function SidebarPinned({
  teamId,
  teamSlug,
}: {
  teamId: string
  teamSlug: string
}) {
  const pins = useTeamPins(teamId)
  if (pins.length === 0) return null
  return <PinnedRows pins={pins} teamId={teamId} teamSlug={teamSlug} />
}

function PinnedRows({
  pins,
  teamId,
  teamSlug,
}: {
  pins: Pin[]
  teamId: string
  teamSlug: string
}) {
  const boards = useTeamBoards(teamId)
  const sessionIds = useMemo(
    () => pins.flatMap((pin) => (pin.sessionId ? [pin.sessionId] : [])),
    [pins]
  )
  const { data: sessions } = useLiveQuery(
    (query) =>
      sessionIds.length > 0
        ? query
            .from({ s: codingSessionCollection })
            .where(({ s }) => inArray(s.id, sessionIds))
        : undefined,
    [sessionIds]
  )
  // The pinned issues plus the pinned sessions' issues — ONE query, so a
  // session row can name its identifier the way the Sessions group does.
  const issueIds = useMemo(() => {
    const ids = new Set(pins.flatMap((pin) => (pin.issueId ? [pin.issueId] : [])))
    for (const session of sessions ?? []) {
      if (session.issueId) ids.add(session.issueId)
    }
    return [...ids].sort()
  }, [pins, sessions])
  const { data: issues } = useLiveQuery(
    (query) =>
      issueIds.length > 0
        ? query
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueIds]
  )
  const { data: actions } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )
  const { sessionId: routeSessionId, issueIdentifier: routeIssueIdentifier } =
    useParams({ strict: false })
  const openSession = useOpenSession()
  const openComposer = useOpenComposer()

  const boardsById = useMemo(
    () => new Map((boards ?? []).map((board) => [board.id, board])),
    [boards]
  )
  const issuesById = useMemo(
    () => new Map(((issues ?? []) as Issue[]).map((issue) => [issue.id, issue])),
    [issues]
  )
  const sessionsById = useMemo(
    () =>
      new Map(
        ((sessions ?? []) as CodingSession[]).map((session) => [
          session.id,
          session,
        ])
      ),
    [sessions]
  )
  const actionsById = useMemo(
    () => new Map((actions ?? []).map((action) => [action.id, action])),
    [actions]
  )

  const unpin = (pin: Pin) => {
    const targetId = pin.issueId ?? pin.sessionId ?? pin.actionId
    if (!targetId) return
    void trpc.pins.toggle.mutate({ teamId, kind: pin.kind, targetId })
  }

  const items = pins.flatMap((pin) => {
    if (pin.kind === `issue` && pin.issueId) {
      const issue = issuesById.get(pin.issueId)
      const board = issue ? boardsById.get(issue.boardId) : undefined
      if (!issue || !board) return []
      return [
        <SidebarMenuItem key={pin.id}>
          <SidebarMenuButton
            asChild
            isActive={routeIssueIdentifier === issue.identifier}
          >
            <Link
              to="/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier"
              params={{
                teamSlug,
                boardSlug: board.slug,
                issueIdentifier: issue.identifier,
              }}
            >
              <NavIssuesIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
              <span className="shrink-0 font-mono text-xs text-muted-foreground">
                {issue.identifier}
              </span>
              <span className="min-w-0 flex-1 truncate">{issue.title}</span>
            </Link>
          </SidebarMenuButton>
          <UnpinAction label={issue.identifier} onClick={() => unpin(pin)} />
        </SidebarMenuItem>,
      ]
    }
    if (pin.kind === `session` && pin.sessionId) {
      const session = sessionsById.get(pin.sessionId)
      if (!session) return []
      const issue = session.issueId ? issuesById.get(session.issueId) : undefined
      const identity = sessionIdentity({ session, issue })
      const pinPrState = rowPrState(session, issue)
      const state = sessionDisplayState(session, pinPrState)
      const working = sessionRowIsWorking(session, pinPrState)
      // A pinned run may be over (the Sessions group lists live ones only):
      // an ended row gets the Past list's steady grey dot, never a live one.
      const ended = session.status === `ended` || session.status === `merged`
      const title = identity.identifier
        ? identity.subject
        : (session.actionName ?? (session.issueId ? identity.subject : `Batch run`))
      // EXP-850 §8: the live run's own caption, the row's second line.
      const caption = sessionAgentCaption(session)
      return [
        <SidebarMenuItem key={pin.id}>
          <SidebarMenuButton
            isActive={routeSessionId === session.id}
            className={caption ? `h-auto py-1` : undefined}
            // EXP-851: a pinned row is context-free — the main menu stays.
            onClick={() => openSession(session, { origin: null })}
          >
            <span className="flex w-4 shrink-0 items-center justify-center">
              <RunningIndicator state={state} paused={ended} working={working} />
            </span>
            <span className="flex min-w-0 flex-1 flex-col items-start">
              <span className="flex w-full min-w-0 items-center gap-1.5">
                {identity.identifier && (
                  <span className="shrink-0 font-mono text-xs text-muted-foreground">
                    {identity.identifier}
                  </span>
                )}
                <span className="min-w-0 flex-1 truncate">{title}</span>
              </span>
              {caption && (
                <span
                  className="w-full truncate text-xs text-muted-foreground"
                  title={caption}
                >
                  {caption}
                </span>
              )}
            </span>
          </SidebarMenuButton>
          <UnpinAction label={title} onClick={() => unpin(pin)} />
        </SidebarMenuItem>,
      ]
    }
    if (pin.kind === `action` && pin.actionId) {
      const action = actionsById.get(pin.actionId)
      if (!action) return []
      const ActionIcon = getActionIcon(action)
      return [
        <SidebarMenuItem key={pin.id}>
          <SidebarMenuButton
            onClick={() => openComposer({ actionId: action.id })}
            title={`Run ${action.name}`}
          >
            <ActionIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">{action.name}</span>
          </SidebarMenuButton>
          <UnpinAction label={action.name} onClick={() => unpin(pin)} />
        </SidebarMenuItem>,
      ]
    }
    return []
  })

  if (items.length === 0) return null
  return (
    <SidebarGroup>
      <SidebarGroupLabel>Pinned</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>{items}</SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

function UnpinAction({
  label,
  onClick,
}: {
  label: string
  onClick: () => void
}) {
  return (
    <SidebarMenuAction
      showOnHover
      title="Unpin"
      aria-label={`Unpin ${label}`}
      onClick={onClick}
    >
      <UiUnpinIcon />
    </SidebarMenuAction>
  )
}
