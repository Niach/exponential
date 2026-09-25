import { useCallback, useRef, useState, type ReactNode } from "react"
import type { Team } from "@/db/schema"
import { useIssueMenuGestures, type IssueMenuTarget } from "./gestures"
import { IssueMenuSelectionContext, type IssueMenuSelection } from "./selection"
import { IssueMenuSession } from "./session"

// EXP-1074 — THE issue context menu: ONE host per team layout.
//
// Before, every list row wrapped itself in a Radix ContextMenu root (plus the
// three dialogs the menu can open), the sidebar list and the reviews queue had
// none, and a row that left its status group — or a list that flipped to its
// empty state — took its open menu with it. Now any element opts in with
// `issueMenuProps(issueId)` (`attr.ts`), the gestures are document listeners
// (`gestures.ts`), and the menu itself is one controlled DropdownMenu anchored
// at the pointer (`session.tsx`), resolving the issue live by id.
//
// Every open is a FRESH session (keyed by issue + a sequence number): the
// previous menu's exit animation never cross-fades into the next, and the
// modal layer's body pointer-events lock is taken anew. The target is kept
// after a close on purpose — the menu's deferred dialogs (duplicate picker,
// relation picker, move-board confirm) outlive the menu that opened them.

interface OpenTarget extends IssueMenuTarget {
  seq: number
}

export function IssueContextMenuProvider({
  team,
  children,
}: {
  team: Team | null
  children: ReactNode
}) {
  const [target, setTarget] = useState<OpenTarget | null>(null)
  const [open, setOpen] = useState(false)
  const openRef = useRef(false)
  openRef.current = open
  const seqRef = useRef(0)
  const registry = useRef(new Set<IssueMenuSelection>()).current

  useIssueMenuGestures(
    useCallback((next: IssueMenuTarget) => {
      seqRef.current += 1
      setTarget({ ...next, seq: seqRef.current })
      setOpen(true)
    }, []),
    openRef
  )

  const selection = target
    ? [...registry].find((adapter) =>
        target.origin ? adapter.root.current?.contains(target.origin) : false
      )
    : undefined

  return (
    <IssueMenuSelectionContext.Provider value={registry}>
      {children}
      {target && (
        <IssueMenuSession
          key={`${target.issueId}:${target.seq}`}
          target={target}
          team={team}
          open={open}
          onOpenChange={setOpen}
          selection={selection}
        />
      )}
    </IssueMenuSelectionContext.Provider>
  )
}
