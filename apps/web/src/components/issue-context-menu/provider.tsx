import { useRef, type ReactNode } from "react"
import { MenuGestureHost } from "@exp/ui"
import type { Team } from "@/db/schema"
import { ISSUE_MENU_KIND } from "./attr"
import { IssueMenuSelectionContext, type IssueMenuSelection } from "./selection"
import { IssueMenuSession } from "./session"
import {
  AGENT_MENU_KIND,
  AgentMenuSession,
  CHAT_MENU_KIND,
  ChatMenuSession,
  useChatActions,
} from "@/components/thread/chat-menu"

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
// Thread 58cb32df (chats.json): the SAME host answers a chat row (kind
// `thread`: Stop · Stop and delete | Delete) and an agent row (kind
// `agent`: Compact · New conversation · Stop · Agent settings); the delete
// confirm lives here so it outlives the menu.

export function IssueContextMenuProvider({
  team,
  children,
}: {
  team: Team | null
  children: ReactNode
}) {
  const registry = useRef(new Set<IssueMenuSelection>()).current
  const chatActions = useChatActions(team?.slug)
  // The list (issues or chats) whose root holds the row the menu opened on.
  const selectionOf = (origin: HTMLElement | null) =>
    [...registry].find((adapter) => (origin ? adapter.root.current?.contains(origin) : false))

  return (
    <IssueMenuSelectionContext.Provider value={registry}>
      <MenuGestureHost
        menus={{
          [ISSUE_MENU_KIND]: ({ target, open, onOpenChange }) => (
            <IssueMenuSession
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
          [AGENT_MENU_KIND]: (session) => <AgentMenuSession {...session} team={team} />,
        }}
      >
        {children}
      </MenuGestureHost>
      {chatActions.dialog}
    </IssueMenuSelectionContext.Provider>
  )
}
