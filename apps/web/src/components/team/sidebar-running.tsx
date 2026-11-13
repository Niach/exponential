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
  SidebarMenuTrailing,
  SessionRowDevice,
  chatRunMarkState,
} from "@exp/ui"
import { sessionCollection } from "@/lib/collections"
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

// EXP-923 / EXP-1293 / EXP-1318: the ACTIVE band lives in the SIDEBAR,
// under the boards — `railChats` over the Chats list of the ACTIVE team
// (fixture-locked `partitionChats`, `chats-list.json` `railCases`):
// every Needs you row, every Running row, then the Recent rows whose word
// still asks for a merge (`readyToMerge`, `stack`); hidden entirely when
// empty. A row = the agent mark (the working spark, the yellow badge while it
// needs you) + the title (it names the issue), NO captions. A click opens
// the chat with the `running` origin, which creates NO work tab
// (`lib/work-tabs.ts` `TABLESS_ORIGIN`). #EXP-1461: ROOTS only (a child
// never lists), a bold title = unread; a row that changes band
// enters on the motion tokens. Chat 58cb32df: a root row opens THE chat
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

function useOpenChat(teamSlug: string): (chatId: string) => void {
  const navigate = useNavigate()
  return (chatId) => {
    void navigate({
      to: `/t/$teamSlug/sessions/$sessionId`,
      params: { teamSlug, sessionId: chatId },
      search: { from: TABLESS_ORIGIN },
    })
  }
}

/** The expanded sidebar's Active group — hidden with nothing in it. */
export function SidebarRunningSection({
  teamSlug,
  chats,
}: {
  teamSlug: string
  chats: SidebarChats
}) {
  const rows = railChats(chats.groups, chats.wordOf)
  const shown = useShownChatId()
  const openChat = useOpenChat(teamSlug)
  const deviceGlyph = useDeviceGlyphs()
  const entering = useEnteringRows(rows.map((row) => row.sessionId))
  if (rows.length === 0) return null
  return (
    <SidebarGroup data-testid="sidebar-running">
      {/* EXP-1022: the main menu's plain group label, like Boards above it —
          the filled group band belongs to LISTS, not the menu. */}
      <SidebarGroupLabel>{CHATS_LIST_COPY.rail}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>
          {rows.map((row) => {
            const device = deviceGlyph(row.deviceId)
            return (
              <SidebarMenuItem key={row.sessionId} className={entering(row.sessionId)}>
                <SidebarMenuButton
                  density="compact"
                  isActive={shown === row.sessionId}
                  onClick={() => openChat(row.sessionId)}
                  data-testid={`sidebar-running-${row.sessionId}`}
                  // Chat 58cb32df: a chat row's menu (S-3: Delete) on
                  // right-click / long-press.
                  {...menuProps(CHAT_ROW_MENU_KIND, row.sessionId)}
                >
                  <ChatRunMark row={row} />
                  <SidebarRunningTitle title={row.title} unread={row.unread} />
                </SidebarMenuButton>
                {/* EXP-1318: the root's device, trailing — EXP-1477: in the
                    rail's one trailing column. */}
                {device && (
                  <SidebarMenuTrailing>
                    <SessionRowDevice icon={device.icon} name={device.name} />
                  </SidebarMenuTrailing>
                )}
              </SidebarMenuItem>
            )
          })}
        </SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

