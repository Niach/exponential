import { createContext, useContext, useEffect, type RefObject } from "react"

// EXP-1074 — the phone's "Select" item (FEED-12: a long-press opens the menu,
// so selection mode starts from a menu item, not a competing recognizer).
// The menu host sits above every list, so a list REGISTERS its selection
// here while mounted; the host picks the adapter whose root holds the row.

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
