import { useMemo, useState } from "react"
import { useParams } from "@tanstack/react-router"
import type { CodingSession } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { nestSessions, type SessionTreeRow } from "@/lib/session-tree"
import { sessionIdentity } from "@/lib/session-identity"
import {
  sessionDisplayState,
  sessionRowIsWorking,
} from "@/lib/coding-session-display"
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
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

/** EXP-818: the rows a COLLAPSED set leaves visible — a row whose parent (or
 * any ancestor) is collapsed is skipped. Keyed on the flattened depths, so it
 * needs nothing but `nestSessions`' output. */
function visibleTreeRows(
  rows: readonly SessionTreeRow<CodingSession>[],
  collapsed: ReadonlySet<string>
): SessionTreeRow<CodingSession>[] {
  const out: SessionTreeRow<CodingSession>[] = []
  let hideBelow: number | null = null
  for (const row of rows) {
    if (hideBelow !== null && row.depth > hideBelow) continue
    hideBelow = null
    out.push(row)
    if (row.hasChildren && collapsed.has(row.session.id)) hideBelow = row.depth
  }
  return out
}

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
  // EXP-818: a parent run's subtree folds away — an orchestrator with six
  // children used to push every other group out of the sidebar. Expanded by
  // default (a child run is the interesting part of a tree), per-parent, for
  // as long as the sidebar is mounted.
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(
    () => new Set<string>()
  )
  const tree = useMemo(
    () => nestSessions(running.map((row) => row.session)),
    [running]
  )
  const rows = useMemo(() => visibleTreeRows(tree, collapsed), [tree, collapsed])
  const toggle = (sessionId: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (!next.delete(sessionId)) next.add(sessionId)
      return next
    })

  if (running.length === 0) return null
  const byId = new Map(running.map((row) => [row.session.id, row]))

  return (
    <SidebarGroup>
      <SidebarGroupLabel>Sessions</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {rows.map(({ session, depth, hasChildren }) => (
            <SessionItem
              key={session.id}
              row={byId.get(session.id)!}
              depth={depth}
              active={routeSessionId === session.id}
              expandable={hasChildren}
              expanded={!collapsed.has(session.id)}
              onToggle={() => toggle(session.id)}
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
  expandable,
  expanded,
  onToggle,
  onOpen,
}: {
  row: AgentSessionRow
  depth: number
  active: boolean
  /** EXP-818: this run started others — the row carries the fold twisty. */
  expandable: boolean
  expanded: boolean
  onToggle: () => void
  onOpen: () => void
}) {
  const { session, issue, device, paused } = row
  const identity = sessionIdentity(row)
  const prState = rowPrState(session, issue)
  const state = sessionDisplayState(session, prState)
  const working = sessionRowIsWorking(session, prState)
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
        {expandable ? (
          /* The twisty is its own target inside the row button — a click
             folds the subtree instead of opening the run. */
          <span
            role="button"
            tabIndex={-1}
            aria-label={expanded ? `Collapse child runs` : `Expand child runs`}
            className="flex w-3 shrink-0 items-center justify-center text-muted-foreground hover:text-foreground"
            onClick={(event) => {
              event.stopPropagation()
              onToggle()
            }}
          >
            {expanded ? (
              <ChevronDownIcon className="size-3" />
            ) : (
              <ChevronRightIcon className="size-3" />
            )}
          </span>
        ) : (
          <span aria-hidden className="w-3 shrink-0" />
        )}
        <span className="flex w-4 shrink-0 items-center justify-center">
          {isChat ? (
            <ActionChatIcon className="size-3.5 text-muted-foreground" />
          ) : (
            <RunningIndicator state={state} paused={paused} working={working} />
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
