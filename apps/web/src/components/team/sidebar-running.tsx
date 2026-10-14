import type * as React from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import {
  AgentRunMark,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  type RunMarkState,
} from "@exp/ui"
import { sessionCollection } from "@/lib/collections"
import {
  THREADS_LIST_COPY,
  threadRowMarkState,
  type ThreadsListGroup,
  type ThreadsListRow,
} from "@/lib/session-tree"
import type { ThreadsListExtras } from "@/hooks/use-session-tree"
import { TABLESS_ORIGIN } from "@/lib/work-tabs"

// EXP-923 / EXP-1293: RUNNING lives in the SIDEBAR, under the boards — the
// Threads list's `needs_you` + `running` groups of the ACTIVE team
// (`useThreadsList`, fixture-locked `partitionThreads`), hidden entirely
// when nothing runs. A row = the agent mark (the working spark, the amber
// badge while it needs you) + the first issue's identifier + the title, NO
// captions. A click opens the thread with the `running` origin, which
// creates NO work tab (`lib/work-tabs.ts` `TABLESS_ORIGIN`).

/** The list data the sidebar reads ONCE and hands to its sections. */
export interface SidebarThreads {
  groups: ThreadsListGroup[]
  extras: ThreadsListExtras
}

/** The rows the Running section lists: needs you first, then running. */
export function runningThreadRows(groups: readonly ThreadsListGroup[]): ThreadsListRow[] {
  return groups
    .filter((group) => group.key === `needs_you` || group.key === `running`)
    .flatMap((group) => group.rows)
}

/** The nav's live state for the Threads entry: green while anything runs,
 *  amber while something needs you. */
export function threadsNavState(groups: readonly ThreadsListGroup[]): {
  live: boolean
  needsYou: boolean
} {
  const needsYou = groups.some((group) => group.key === `needs_you` && group.rows.length > 0)
  const running = groups.some((group) => group.key === `running` && group.rows.length > 0)
  return { live: needsYou || running, needsYou }
}

/** A thread row's mark state — the ×4 rule (`threadRowMarkState`, EXP-1312)
 *  in the run marks' words (waiting = the bare mark). */
export function threadRunMarkState(row: {
  needsYou: boolean
  busy: boolean
}): RunMarkState | undefined {
  const state = threadRowMarkState(row)
  if (state === `needs_you`) return `needs_input`
  if (state === `working`) return `working`
  return undefined
}

/** The agent CLI of a thread's root (its brand mark). */
function rootCli(sessionId: string): string | null {
  return sessionCollection.get(sessionId)?.cli ?? null
}

export function ThreadRunMark({
  row,
  ringClassName = `ring-sidebar`,
  className,
}: {
  row: Pick<ThreadsListRow, `sessionId` | `needsYou` | `busy`>
  ringClassName?: string
  className?: string
}) {
  return (
    <AgentRunMark
      agent={rootCli(row.sessionId)}
      state={threadRunMarkState(row)}
      ringClassName={ringClassName}
      className={className}
    />
  )
}

/** Which thread the app is SHOWING: its page, or one of its sessions'. */
function useShownThreadId(): string | null {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const thread = /\/threads\/([^/]+)$/.exec(pathname)?.[1]
  if (thread) return thread
  const session = /\/sessions\/([^/]+)$/.exec(pathname)?.[1]
  if (!session) return null
  const row = sessionCollection.get(session)
  return row ? (row.rootId ?? row.id) : null
}

function useOpenThread(teamSlug: string): (threadId: string) => void {
  const navigate = useNavigate()
  return (threadId) => {
    void navigate({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug, threadId },
      search: { from: TABLESS_ORIGIN },
    })
  }
}

/** The expanded sidebar's Running group — hidden with nothing live. */
export function SidebarRunningSection({
  teamSlug,
  threads,
}: {
  teamSlug: string
  threads: SidebarThreads
}) {
  const rows = runningThreadRows(threads.groups)
  const shown = useShownThreadId()
  const openThread = useOpenThread(teamSlug)
  if (rows.length === 0) return null
  return (
    <SidebarGroup data-testid="sidebar-running">
      {/* EXP-1022: the main menu's plain group label, like Boards above it —
          the filled group band belongs to LISTS, not the menu. */}
      <SidebarGroupLabel>{THREADS_LIST_COPY.groups.running}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {rows.map((row) => {
            const identifier = threads.extras.identifierByRoot.get(row.sessionId)
            return (
              <SidebarMenuItem key={row.sessionId}>
                <SidebarMenuButton
                  density="compact"
                  isActive={shown === row.sessionId}
                  onClick={() => openThread(row.sessionId)}
                  data-testid={`sidebar-running-${row.sessionId}`}
                >
                  <ThreadRunMark row={row} />
                  {identifier && (
                    <span className="shrink-0 font-mono text-xs text-muted-foreground">
                      {identifier}
                    </span>
                  )}
                  <span className="min-w-0 flex-1 truncate">{row.title}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** The compact rail's Running column — one 32px button per running thread
 *  under the board icons. The rail owns the button chrome, so it hands one
 *  in, exactly like `SidebarPinnedIcons`. */
export function SidebarRunningIcons({
  teamSlug,
  threads,
  renderItem,
  separator,
}: {
  teamSlug: string
  threads: SidebarThreads
  renderItem: (item: {
    key: string
    label: string
    active: boolean
    onClick: () => void
    icon: React.ReactNode
  }) => React.ReactNode
  separator?: React.ReactNode
}) {
  const rows = runningThreadRows(threads.groups)
  const shown = useShownThreadId()
  const openThread = useOpenThread(teamSlug)
  if (rows.length === 0) return null
  return (
    <>
      {separator}
      {rows.map((row) => {
        const identifier = threads.extras.identifierByRoot.get(row.sessionId)
        return renderItem({
          key: row.sessionId,
          label: identifier ? `${identifier} ${row.title}` : row.title,
          active: shown === row.sessionId,
          onClick: () => openThread(row.sessionId),
          icon: <ThreadRunMark row={row} />,
        })
      })}
    </>
  )
}
