import { useMemo, useState, type PointerEvent as ReactPointerEvent } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { startListDrag } from "@exponential_app/eui/web"
import { SIDEBAR_TREE_MAX_DEPTH } from "@exp/domain-contract/sidebar-tree"
import {
  AgentRunMark,
  MenuGestureHost,
  menuProps,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuTrailing,
  SessionRowDevice,
  TeamAvatar,
  chatRunMarkState,
  useSidebar,
  type MenuSessionProps,
} from "@exp/ui"
import { sessionCollection, sidebarLayoutCollection } from "@/lib/collections"
import {
  CHATS_LIST_COPY,
  railChats,
  type ChatRowWord,
  type ChatsListGroup,
  type ChatsListRow,
} from "@/lib/session-tree"
import { TABLESS_ORIGIN } from "@/lib/work-tabs"
import { useDeviceGlyphs } from "@/hooks/use-device-glyphs"
import { cn } from "@/lib/utils"
import { useEnteringRows } from "@/hooks/use-entering-rows"
import { SIDEBAR_TEAM_LIST_ATTR, sidebarTeamDrop } from "@/hooks/use-sidebar-tree"
import { openHandlers, sidebarMenuEntries, SidebarPointerMenu } from "@/components/team/sidebar-menus"
import { useCanDeleteChat, useChatActions } from "@/components/chat/chat-menu"

// EXP-923 / EXP-1293 / EXP-1318: the ACTIVE band lives in the SIDEBAR,
// at its bottom — `railChats` over the Chats list of EVERY member team
// (#EXP-2027: no active team; each row wears its team's mark, trailing)
// (fixture-locked `partitionChats`, `chats-list.json` `railCases`):
// every Needs you row, every Running row, then the Recent rows whose word
// still asks for a merge (`readyToMerge`, `stack`); hidden entirely when
// empty. A row = the agent mark (the working spark, the yellow badge while it
// needs you) + the title (it names the issue), NO captions. A click opens
// the chat with the `running` origin, which creates NO work tab
// (`lib/work-tabs.ts` `TABLESS_ORIGIN`). #EXP-1461: ROOTS only (a child
// never lists), a bold title = unread; a row that changes band
// enters on the motion tokens. #EXP-2124 (sidebar-menus.json): a row's
// right-click opens THE table's `active-session` menu (Open, Open in new tab,
// Copy link, Add to sidebar while it is not placed, the chat's Delete behind
// its confirm for its starter or an owner). #EXP-2027
// (sidebar-tree.json `drop`): a row drags onto its team's list
// (`sidebarDropSession`, through the list's registered drop).

/** #EXP-2124: an Active row's menu kind (`menuProps`). */
export const SIDEBAR_ACTIVE_MENU_KIND = `sidebar-active`

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
export interface SidebarChats {
  groups: ChatsListGroup[]
  /** A row's quiet word (its root's PRs), what keeps a Recent row Active. */
  wordOf: (row: ChatsListRow) => ChatRowWord | null
}

/** A chat row's mark state (EXP-1312): THE one in `@exp/ui`. */
export { chatRunMarkState }

/** The agent CLI of a chat's root (its brand mark). */
function rootCli(sessionId: string): string | null {
  return sessionCollection.get(sessionId)?.cli ?? null
}

export function ChatRunMark({
  row,
  ringClassName = `ring-sidebar`,
  className,
}: {
  row: Pick<ChatsListRow, `sessionId` | `needsYou` | `busy`>
  ringClassName?: string
  className?: string
}) {
  return (
    <AgentRunMark
      agent={rootCli(row.sessionId)}
      state={chatRunMarkState(row)}
      ringClassName={ringClassName}
      className={className}
    />
  )
}

/** Which chat the app is SHOWING: its page, or one of its sessions'. */
function useShownChatId(): string | null {
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const session = /\/sessions\/([^/]+)$/.exec(pathname)?.[1]
  if (!session) return null
  const row = sessionCollection.get(session)
  return row ? (row.rootId ?? row.id) : null
}

function useOpenChat(): (teamSlug: string, chatId: string) => void {
  const navigate = useNavigate()
  return (teamSlug, chatId) => {
    void navigate({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug, sessionId: chatId },
      search: { from: TABLESS_ORIGIN },
    })
  }
}

/** A member team as an Active row needs it: its slug (the chat's URL) and
 *  its mark. */
export interface SidebarRunningTeam {
  slug: string
  iconUrl: string | null | undefined
  name: string
}

/** An Active row's drag onto its team's list: the rows dropped between are
 *  that list's (`SIDEBAR_TEAM_LIST_ATTR`), the dragged one this row (a list
 *  row of the same session steps aside: the drop is then a plain move). */
function startSessionDrag(event: ReactPointerEvent<HTMLLIElement>, row: ChatsListRow) {
  const element = event.currentTarget
  startListDrag(
    event.nativeEvent,
    row.sessionId,
    {
      rows: () => {
        const list = document.querySelector(`[${SIDEBAR_TEAM_LIST_ATTR}="${row.teamId}"]`)
        const own = [...(list?.querySelectorAll<HTMLElement>(`[data-reorder-key]`) ?? [])].filter(
          (el) => el.getAttribute(`data-reorder-key`) !== row.sessionId
        )
        return [...own, element]
      },
      maxDepth: SIDEBAR_TREE_MAX_DEPTH,
      color: `var(--primary)`,
    },
    {
      move: (context) =>
        sidebarTeamDrop(row.teamId)?.(
          { id: row.sessionId, teamId: row.teamId, parentId: row.parentId },
          context
        ),
    }
  )
}

/** The expanded sidebar's Active group — hidden with nothing in it. */
export function SidebarRunningSection({
  teams,
  chats,
}: {
  /** Every member team by id (#EXP-2027: the band spans them all). */
  teams: ReadonlyMap<string, SidebarRunningTeam>
  chats: SidebarChats
}) {
  const rows = railChats(chats.groups, chats.wordOf).filter((row) => teams.has(row.teamId))
  const shown = useShownChatId()
  const openChat = useOpenChat()
  const deviceGlyph = useDeviceGlyphs()
  const { isMobile } = useSidebar()
  const entering = useEnteringRows(rows.map((row) => row.sessionId))
  const placed = usePlacedSessions()
  // The chat a Delete was asked for: its own team's slug is where the open
  // chat closes back to (the confirm outlives the menu).
  const [deletingTeam, setDeletingTeam] = useState<string | null>(null)
  const chatActions = useChatActions(deletingTeam ? teams.get(deletingTeam)?.slug : undefined)
  const activeMenu = (session: MenuSessionProps) => {
    const row = rows.find((candidate) => candidate.sessionId === session.target.id)
    const team = row ? teams.get(row.teamId) : undefined
    if (!row || !team) return null
    return (
      <ActiveSessionMenu
        session={session}
        row={row}
        team={team}
        placed={placed.has(row.sessionId)}
        onOpen={() => openChat(team.slug, row.sessionId)}
        onDelete={() => {
          setDeletingTeam(row.teamId)
          chatActions.requestDelete(row.sessionId)
        }}
      />
    )
  }
  if (rows.length === 0) return chatActions.dialog
  return (
    <MenuGestureHost menus={{ [SIDEBAR_ACTIVE_MENU_KIND]: activeMenu }}>
    <SidebarGroup data-testid="sidebar-running">
      {/* EXP-1022: the main menu's plain group label, like Boards above it —
          the filled group band belongs to LISTS, not the menu. */}
      <SidebarGroupLabel>{CHATS_LIST_COPY.rail}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {rows.map((row) => {
            const device = deviceGlyph(row.deviceId)
            const team = teams.get(row.teamId) as SidebarRunningTeam
            return (
              <SidebarMenuItem
                key={row.sessionId}
                className={entering(row.sessionId)}
                data-reorder-key={row.sessionId}
                data-reorder-depth="1"
                data-reorder-indent="0"
                onDragStart={(e) => e.preventDefault()}
                onPointerDown={!isMobile && row.depth === 0 ? (e) => startSessionDrag(e, row) : undefined}
              >
                <SidebarMenuButton
                  density="compact"
                  isActive={shown === row.sessionId}
                  onClick={() => openChat(team.slug, row.sessionId)}
                  data-testid={`sidebar-running-${row.sessionId}`}
                  {...menuProps(SIDEBAR_ACTIVE_MENU_KIND, row.sessionId)}
                >
                  <ChatRunMark row={row} />
                  <SidebarRunningTitle title={row.title} unread={row.unread} />
                </SidebarMenuButton>
                {/* EXP-1318: the root's device, trailing — EXP-1477: in the
                    rail's one trailing column; #EXP-2027: the team's mark
                    beside it. */}
                <SidebarMenuTrailing className="gap-1.5">
                  {device && <SessionRowDevice icon={device.icon} name={device.name} />}
                  <span title={team.name} data-testid="running-team-mark" className="flex">
                    <TeamAvatar iconUrl={team.iconUrl} size={14} />
                  </span>
                </SidebarMenuTrailing>
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
    {chatActions.dialog}
    </MenuGestureHost>
  )
}

/** The sessions the viewer placed in a team's list (an Active row's Add to
 *  sidebar hides once its session is one). */
function usePlacedSessions(): ReadonlySet<string> {
  const { data } = useLiveQuery(
    (query) =>
      query
        .from({ s: sidebarLayoutCollection })
        .where(({ s }) => and(eq(s.section, `team`), eq(s.kind, `item`))),
    []
  )
  return useMemo(
    () => new Set(((data ?? []) as { targetId: string | null }[]).flatMap((row) => (row.targetId ? [row.targetId] : []))),
    [data]
  )
}

/** #EXP-2124: an Active row's menu off THE table. */
function ActiveSessionMenu({
  session,
  row,
  team,
  placed,
  onOpen,
  onDelete,
}: {
  session: MenuSessionProps
  row: ChatsListRow
  team: SidebarRunningTeam
  placed: boolean
  onOpen: () => void
  onDelete: () => void
}) {
  const root = sessionCollection.get(row.sessionId) ?? null
  const canDelete = useCanDeleteChat(root)
  const href = `/t/${encodeURIComponent(team.slug)}/sessions/${encodeURIComponent(row.sessionId)}`
  return (
    <SidebarPointerMenu
      session={session}
      title={row.title || CHATS_LIST_COPY.rail}
      entries={sidebarMenuEntries(`active-session`, { canDelete }, {
        ...openHandlers(href, onOpen, { kind: `chat`, chatId: row.sessionId, from: null, teamId: row.teamId, teamSlug: team.slug }),
        ...(placed
          ? {}
          : {
              "add-to-sidebar": () =>
                sidebarTeamDrop(row.teamId)?.(
                  { id: row.sessionId, teamId: row.teamId, parentId: row.parentId },
                  { id: row.sessionId, into: row.teamId }
                ),
            }),
        delete: onDelete,
      })}
    />
  )
}

