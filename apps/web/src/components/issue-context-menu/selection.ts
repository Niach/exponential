import { createContext, useContext, useEffect, useState, type RefObject } from "react"

// EXP-1074 — a list's selection, as the menu host sees it. The menu host
// sits above every list, so a list REGISTERS its selection here while
// mounted; the host picks the adapter whose root holds the row.
// #EXP-1388 (owner 2026-10-27, chats.json): on touch a long-press on a row
// of a registered list (issues or chats) ENTERS the selection with that row
// instead of opening a menu (`useLongPressSelects`); a pointer's menu keeps
// its Select item.

export interface IssueMenuSelection {
  /** The list's element — which adapter a row belongs to. */
  root: RefObject<HTMLElement | null>
  isSelected: (issueId: string) => boolean
  toggle: (issueId: string) => void
}

export const IssueMenuSelectionContext = createContext<Set<IssueMenuSelection> | null>(
  null
)

/** Registers a list's selection with the menu host for as long as the
 *  adapter is non-null (memoize it: a new object re-registers). */
export function useIssueMenuSelection(adapter: IssueMenuSelection | null): void {
  const registry = useContext(IssueMenuSelectionContext)
  useEffect(() => {
    if (!registry || !adapter) return
    registry.add(adapter)
    return () => {
      registry.delete(adapter)
    }
  }, [registry, adapter])
}

/** A touch-first device: a long-press is a gesture, not a right-click. */
const coarsePointer = (): boolean =>
  typeof window !== `undefined` && window.matchMedia?.(`(pointer: coarse)`).matches === true

/** #EXP-1388: true when this menu open is a touch long-press on a row of a
 *  selecting list: the row is toggled into the selection and the menu
 *  closes at once (the caller then renders nothing). */
export function useLongPressSelects(
  selection: IssueMenuSelection | undefined,
  id: string,
  open: boolean,
  onOpenChange: (open: boolean) => void
): boolean {
  const [touch] = useState(coarsePointer)
  const selects = Boolean(selection) && touch
  useEffect(() => {
    if (!selects || !open) return
    selection?.toggle(id)
    onOpenChange(false)
  }, [selects, open, selection, id, onOpenChange])
  return selects
}
