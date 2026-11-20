import { useRef, useState, type ReactNode } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { MenuGestureHost, type MenuSessionProps } from "@exp/ui"
import type { Board, Issue, Team } from "@/db/schema"
import { boardCollection, issueCollection } from "@/lib/collections"
import { useTeamById } from "@/hooks/use-team-data"
import { useAgentRow } from "@/hooks/use-agent-row"
import { useSessionRow } from "@/hooks/use-session-tree"
import { TeamStatusesProvider } from "@/hooks/use-team-statuses"
import { IssueRefProvider } from "@/components/issue-ref-provider"
import { ISSUE_MENU_KIND } from "./attr"
import {
  IssueMenuSelectionContext,
  useLongPressSelects,
  type IssueMenuSelection,
} from "./selection"
import { IssueMenuSession } from "./session"
import {
  AGENT_MENU_KIND,
  AgentMenuSession,
  CHAT_MENU_KIND,
  ChatMenuSession,
  useChatActions,
} from "@/components/chat/chat-menu"

// EXP-1074 — THE issue context menu: ONE host per team layout.
//
// Any element opts in with `issueMenuProps(issueId)` (`attr.ts`); the
// gestures are the generic `@exp/ui` `MenuGestureHost` (a right-click or a
// touch long-press, as document listeners), and the menu itself is one
// `Menu` at the pointer (`session.tsx`, a bottom sheet on a phone),
// resolving the issue live by id. Every open is a FRESH session, and the
// target outlives a close on purpose — the menu's deferred dialogs
// (duplicate picker, relation picker, move-board confirm, delete prompt)
// outlive the menu that opened them.
//
// Chat 58cb32df (chats.json): the SAME host answers a chat row (kind
// `chat`: Stop · Stop and delete | Delete) and an agent row (kind
// `agent`: Compact · New conversation · Stop · Agent settings); the delete
// confirm lives here so it outlives the menu.
//
// #EXP-2027: md+ has no active team, so `team` (the layout's) is only a
// FALLBACK: every menu acts in the team of the ROW it opened on (an issue's
// board's team, a chat's or an agent's own `teamId`) — its statuses,
// labels, members, boards, permissions and slugs — and takes `team` only
// when the row names none (or its team row has not synced).

export function IssueContextMenuProvider({
  team,
  children,
}: {
  team: Team | null
  children: ReactNode
}) {
  const registry = useRef(new Set<IssueMenuSelection>()).current
  // The chat a delete was asked for: its own team's slug is where the open
  // chat closes back to.
  const [chatRootId, setChatRootId] = useState<string | null>(null)
  const chatTeam = useRowTeam(useSessionRow(chatRootId).row?.teamId, team)
  const baseChatActions = useChatActions(chatTeam?.slug)
  const chatActions = {
    ...baseChatActions,
    requestDelete: (rootId: string) => {
      setChatRootId(rootId)
      baseChatActions.requestDelete(rootId)
    },
  }
  // The list (issues or chats) whose root holds the row the menu opened on.
  const selectionOf = (origin: HTMLElement | null) =>
    [...registry].find((adapter) => (origin ? adapter.root.current?.contains(origin) : false))

  return (
    <IssueMenuSelectionContext.Provider value={registry}>
      <MenuGestureHost
        menus={{
          [ISSUE_MENU_KIND]: ({ target, open, onOpenChange }) => (
            <IssueMenuInTeam
              target={target}
              team={team}
              open={open}
              onOpenChange={onOpenChange}
              selection={selectionOf(target.origin)}
            />
          ),
          // #EXP-1388: the Chats list registers its selection the same way.
          [CHAT_MENU_KIND]: (session) => (
            <ChatMenuSession
              {...session}
              actions={chatActions}
              selection={selectionOf(session.target.origin)}
            />
          ),
          [AGENT_MENU_KIND]: (session) => <AgentMenuInTeam {...session} team={team} />,
        }}
      >
        {children}
      </MenuGestureHost>
      {chatActions.dialog}
    </IssueMenuSelectionContext.Provider>
  )
}

/** #EXP-1388 (owner 2026-10-27): a touch long-press on a row of a selecting
 *  issue list enters the selection directly, as on the Chats list and the
 *  natives; anything else opens THE issue menu. */
function IssueMenuOrSelect(props: Parameters<typeof IssueMenuSession>[0]) {
  const selects = useLongPressSelects(props.selection, props.target.id, props.open, props.onOpenChange)
  if (selects) return null
  return <IssueMenuSession {...props} />
}

/** #EXP-2027: the row's own team (by id), `fallback` when the row names
 *  none or its team row is not synced; the fallback object itself when the
 *  ids agree, so a team-scoped page keeps its layout team. */
function useRowTeam(teamId: string | null | undefined, fallback: Team | null): Team | null {
  const other = teamId && teamId !== fallback?.id ? teamId : null
  const own = useTeamById(other)
  return other ? (own ?? fallback) : fallback
}

/** #EXP-2027: THE issue menu in the issue's own team (issue → board →
 *  `teamId`; the issues shape keeps `team_id` server-side). Another team's
 *  row gets that team's status rows and issue refs (Open issue, the
 *  duplicate and relation pickers) around the session. */
function IssueMenuInTeam(props: Parameters<typeof IssueMenuSession>[0]) {
  const issueId = props.target.id
  const { data: issueRows } = useLiveQuery(
    (query) =>
      query.from({ issues: issueCollection }).where(({ issues }) => eq(issues.id, issueId)),
    [issueId]
  )
  const boardId = ((issueRows?.[0] ?? null) as Issue | null)?.boardId ?? null
  const { data: boardRows } = useLiveQuery(
    (query) =>
      boardId
        ? query.from({ boards: boardCollection }).where(({ boards }) => eq(boards.id, boardId))
        : undefined,
    [boardId]
  )
  const boardTeamId = ((boardRows?.[0] ?? null) as Board | null)?.teamId
  const team = useRowTeam(boardTeamId, props.team)
  const session = <IssueMenuOrSelect {...props} team={team} />
  if (!team || team.id === props.team?.id) return session
  return (
    <TeamStatusesProvider teamId={team.id}>
      <IssueRefProvider teamId={team.id} teamSlug={team.slug}>
        {session}
      </IssueRefProvider>
    </TeamStatusesProvider>
  )
}

/** #EXP-2027: an agent row's menu in the agent's own team (the global
 *  Assistant names none: the layout's). */
function AgentMenuInTeam({ team, ...session }: MenuSessionProps & { team: Team | null }) {
  const { agent } = useAgentRow(session.target.id)
  return <AgentMenuSession {...session} team={useRowTeam(agent?.teamId, team)} />
}
