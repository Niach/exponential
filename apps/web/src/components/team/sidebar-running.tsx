import type * as React from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import {
  AgentRunMark,
  CHAT_ROW_MENU_KIND,
  menuProps,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SessionRowDevice,
  TreeGuides,
  TREE_INDENT,
  threadRunMarkState,
  treeGuides,
} from "@exp/ui"
import { sessionCollection } from "@/lib/collections"
import { chatsUnread } from "@exp/domain-contract/chats"
import type { TreeSessionRead } from "@exp/domain-contract/session-tree"
import {
  THREADS_LIST_COPY,
  railThreads,
  type ThreadRowWord,
  type ThreadsListGroup,
  type ThreadsListRow,
} from "@/lib/session-tree"
import { TABLESS_ORIGIN } from "@/lib/work-tabs"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { cn } from "@/lib/utils"
import { useEnteringRows } from "@/hooks/use-entering-rows"

// EXP-923 / EXP-1293 / EXP-1318: the ACTIVE band lives in the SIDEBAR,
// under the boards — `railThreads` over the Threads list of the ACTIVE team
// (fixture-locked `partitionThreads`, `threads-list.json` `railCases`):
// every Needs you row, every Running row, then the Recent rows whose word
// still asks for a merge (`readyToMerge`, `stack`); hidden entirely when
// empty. A row = the agent mark (the working spark, the amber badge while it
// needs you) + the title (it names the issue), NO captions. A click opens
// the thread with the `running` origin, which creates NO work tab
// (`lib/work-tabs.ts` `TABLESS_ORIGIN`). EXP-1323: a live tree nests its
// VISIBLE children under the root (TreeGuides, 14px a level; a child opens
// that session in its thread; S-3: the rest fold into one muted
// `<root>:earlier` row that opens the thread), a bold title = unread; a row that changes band
// enters on the motion tokens. Thread 58cb32df: a root row opens THE chat
// menu (chats.json MENUS) through the team layout's menu host.

/** The words a row shows while its title is held (EXP-1316: an
 *  issue-linked row waits for the issue shapes). */
export const SIDEBAR_RUNNING_LOADING_LABEL = `Loading`

/** A held row's title line: a skeleton bar in the text's place, never an
 *  empty row beside a glyph. */
function SidebarRunningTitle({ title, unread = false }: { title: string; unread?: boolean }) {
  if (title) {
    return (
      <span
        data-unread={unread || undefined}
        className={cn(`min-w-0 flex-1 truncate`, unread && `font-semibold`)}
      >
        {title}
      </span>
    )
  }
  return (
    <span className="flex min-w-0 flex-1 items-center" aria-label={SIDEBAR_RUNNING_LOADING_LABEL}>
      <span
        data-testid="sidebar-running-title-skeleton"
        aria-hidden
        className="h-2.5 w-24 max-w-full animate-pulse rounded-md bg-accent"
      />
    </span>
  )
}

/** The list data the sidebar reads ONCE and hands to its sections. */
export interface SidebarThreads {
  groups: ThreadsListGroup[]
  /** A row's quiet word (its root's PRs), what keeps a Recent row Active. */
  wordOf: (row: ThreadsListRow) => ThreadRowWord | null
}

/** The nav's live state for the Threads entry: green while anything runs,
 *  amber while something needs you; EXP-1323: else the unread dot while a
 *  thread holds a reply the viewer has not read; #EXP-1388: an archived
 *  chat never lights it (`chatsUnread(groups, reads)`). */
export function threadsNavState(
  groups: readonly ThreadsListGroup[],
  reads: readonly TreeSessionRead[] = []
): {
  live: boolean
  needsYou: boolean
  unread: boolean
} {
  const needsYou = groups.some((group) => group.key === `needs_you` && group.rows.length > 0)
  const running = groups.some((group) => group.key === `running` && group.rows.length > 0)
  return { live: needsYou || running, needsYou, unread: chatsUnread(groups, reads) }
}

/** The sidebar row's left padding = `SidebarMenuButton`'s own 8px. */
const SIDEBAR_TREE_BASE = 8

/** A thread row's mark state (EXP-1312): THE one in `@exp/ui`. */
export { threadRunMarkState }

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

/** Which session a thread page shows in place (`?tab=session&session=`). */
function useShownSessionId(): string | null {
  return useRouterState({
    select: (s) => {
      const search = (s.location.search ?? {}) as Record<string, unknown>
      return search.tab === `session` && typeof search.session === `string`
        ? search.session
        : null
    },
  })
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

function useOpenThread(
  teamSlug: string
): (threadId: string, sessionId?: string) => void {
  const navigate = useNavigate()
  return (threadId, sessionId) => {
    void navigate({
      to: `/t/$teamSlug/threads/$threadId`,
      params: { teamSlug, threadId },
      search: sessionId
        ? { tab: `session`, session: sessionId, from: TABLESS_ORIGIN }
        : { from: TABLESS_ORIGIN },
    })
  }
}

/** The expanded sidebar's Active group — hidden with nothing in it. */
export function SidebarRunningSection({
  teamSlug,
  threads,
}: {
  teamSlug: string
  threads: SidebarThreads
}) {
  const rows = railThreads(threads.groups, threads.wordOf)
  const shown = useShownThreadId()
  const shownSession = useShownSessionId()
  const openThread = useOpenThread(teamSlug)
  const deviceGlyph = useDeviceGlyphs()
  const entering = useEnteringRows(rows.map((row) => row.sessionId))
  const guides = treeGuides(rows.map((row) => row.depth))
  if (rows.length === 0) return null
  return (
    <SidebarGroup data-testid="sidebar-running">
      {/* EXP-1022: the main menu's plain group label, like Boards above it —
          the filled group band belongs to LISTS, not the menu. */}
      <SidebarGroupLabel>{THREADS_LIST_COPY.rail}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {rows.map((row, index) => {
            if (row.earlier !== undefined) {
              // S-3: the tree's hidden sub-sessions fold into ONE muted row
              // (`<root>:earlier`); it opens the thread.
              return (
                <SidebarMenuItem key={row.sessionId} className={entering(row.sessionId)}>
                  <SidebarMenuButton
                    density="compact"
                    onClick={() => openThread(row.rootId)}
                    data-testid={`sidebar-running-${row.sessionId}`}
                    className="relative text-muted-foreground"
                    style={{ paddingLeft: SIDEBAR_TREE_BASE + row.depth * TREE_INDENT }}
                  >
                    <TreeGuides guide={guides[index]} base={SIDEBAR_TREE_BASE} />
                    <AgentRunMark agent={rootCli(row.rootId)} state="ended" ringClassName="ring-sidebar" />
                    <SidebarRunningTitle title={row.title} />
                  </SidebarMenuButton>
                </SidebarMenuItem>
              )
            }
            const device = deviceGlyph(row.deviceId)
            const child = row.depth > 0
            return (
              <SidebarMenuItem key={row.sessionId} className={entering(row.sessionId)}>
                <SidebarMenuButton
                  density="compact"
                  isActive={
                    child
                      ? shownSession === row.sessionId
                      : shown === row.sessionId && shownSession === null
                  }
                  onClick={() =>
                    child ? openThread(row.rootId, row.sessionId) : openThread(row.sessionId)
                  }
                  data-testid={`sidebar-running-${row.sessionId}`}
                  // Thread 58cb32df: a chat row's menu (S-3: Delete) on
                  // right-click / long-press.
                  {...(child ? {} : menuProps(CHAT_ROW_MENU_KIND, row.sessionId))}
                  className={cn(child && `relative`)}
                  style={
                    child
                      ? { paddingLeft: SIDEBAR_TREE_BASE + row.depth * TREE_INDENT }
                      : undefined
                  }
                >
                  {child && <TreeGuides guide={guides[index]} base={SIDEBAR_TREE_BASE} />}
                  <ThreadRunMark row={row} />
                  <SidebarRunningTitle title={row.title} unread={row.unread} />
                  {/* EXP-1318: the root's device, trailing. */}
                  {device && <SessionRowDevice icon={device.icon} name={device.name} />}
                </SidebarMenuButton>
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

/** The compact rail's Active column — one 32px button per Active thread
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
  // The compact rail lists THREADS only: nested children stay in the list.
  const rows = railThreads(threads.groups, threads.wordOf).filter((row) => row.depth === 0)
  const shown = useShownThreadId()
  const openThread = useOpenThread(teamSlug)
  if (rows.length === 0) return null
  return (
    <>
      {separator}
      {rows.map((row) =>
        renderItem({
          key: row.sessionId,
          label: row.title || SIDEBAR_RUNNING_LOADING_LABEL,
          active: shown === row.sessionId,
          onClick: () => openThread(row.sessionId),
          icon: <ThreadRunMark row={row} />,
        })
      )}
    </>
  )
}
