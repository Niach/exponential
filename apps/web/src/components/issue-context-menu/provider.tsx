import { useRef, type ReactNode } from "react"
import { MenuGestureHost } from "@exp/ui"
import type { Team } from "@/db/schema"
import { ISSUE_MENU_KIND } from "./attr"
import { IssueMenuSelectionContext, type IssueMenuSelection } from "./selection"
import { IssueMenuSession } from "./session"

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

export function IssueContextMenuProvider({
  team,
  children,
}: {
  team: Team | null
  children: ReactNode
}) {
  const registry = useRef(new Set<IssueMenuSelection>()).current

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
              selection={[...registry].find((adapter) =>
                target.origin ? adapter.root.current?.contains(target.origin) : false
              )}
            />
          ),
        }}
      >
        {children}
      </MenuGestureHost>
    </IssueMenuSelectionContext.Provider>
  )
}
