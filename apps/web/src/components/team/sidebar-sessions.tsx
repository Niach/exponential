import { useParams } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import { nestSessions } from "@/lib/session-tree"
import { sessionIdentity } from "@/lib/session-identity"
import { sessionDisplayState } from "@/lib/coding-session-display"
import { cn } from "@/lib/utils"
import { RunningIndicator } from "@/components/agent-session-row"
import { rowPrState, useAgentsData, type AgentSessionRow } from "@/hooks/use-agents-data"
import { useOpenSession } from "@/hooks/use-open-session"
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

// EXP-818: the sidebar's Sessions group — the IDE rail's Sessions section,
// on the web. The caller's OWN running sessions (EXP-312), one row each: the
// state dot, the identifier + title, the host machine muted on the right;
// child runs nest under their parent (`parent_session_id`). It replaced the
// bottom dock band (EXP-740/771): a run is opened from here, from the issue's
// Watch, or from the Agent page's list — never from a strip under the card.
//
// Render-only: the dock's REAPER duty lives in `useSteerSessionReaper`,
// mounted by the team layout — this group never mounts on phones (the
// sidebar is a Sheet there), so it cannot own the sockets' lifetime.

const ActionChatIcon = conceptIcon(`action-chat`)

export function SidebarSessions({
  teamId,
  currentUserId,
}: {
  teamId: string
  currentUserId: string
}) {
  const { running } = useAgentsData(teamId, currentUserId)
  const { sessionId: routeSessionId } = useParams({ strict: false })
  const openSession = useOpenSession()

  if (running.length === 0) return null
  const byId = new Map(running.map((row) => [row.session.id, row]))
  const tree = nestSessions(running.map((row) => row.session))

  return (
    <SidebarGroup>
      <SidebarGroupLabel>Sessions</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {tree.map(({ session, depth }) => (
            <SessionItem
              key={session.id}
              row={byId.get(session.id)!}
              depth={depth}
              active={routeSessionId === session.id}
              onOpen={() => openSession(session)}
            />
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

function SessionItem({
  row,
  depth,
  active,
  onOpen,
}: {
  row: AgentSessionRow
  depth: number
  active: boolean
  onOpen: () => void
}) {
  const { session, issue, device, paused } = row
  const identity = sessionIdentity(row)
  const state = sessionDisplayState(session, rowPrState(session, issue))
  const isChat = identity.identifier === null && session.actionName === `Chat`
  const title = identity.identifier
    ? identity.subject
    : (session.actionName ?? (session.issueId ? identity.subject : `Batch run`))
  return (
    <SidebarMenuItem>
      <SidebarMenuButton
        isActive={active}
        onClick={onOpen}
        className={cn(paused && `opacity-60`)}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
        title={paused ? `${device.label ?? `The device`} is offline` : undefined}
      >
        <span className="flex w-4 shrink-0 items-center justify-center">
          {isChat ? (
            <ActionChatIcon className="size-3.5 text-muted-foreground" />
          ) : (
            <RunningIndicator state={state} paused={paused} />
          )}
        </span>
        {identity.identifier && (
          <span className="shrink-0 font-mono text-xs text-muted-foreground">
            {identity.identifier}
          </span>
        )}
        <span className="min-w-0 flex-1 truncate">{title}</span>
        {device.label && (
          <span className="max-w-[6rem] shrink-0 truncate text-xs text-muted-foreground">
            {device.label}
          </span>
        )}
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}
