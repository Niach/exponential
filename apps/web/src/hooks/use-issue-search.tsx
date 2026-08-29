import { createContext, useContext, type ReactNode } from "react"

// EXP-686: Search left the mobile tab bar (the bar is full: Issues, Inbox,
// Support, Devices, Actions, Reviews) and became a button in the board
// header next to Filter — native parity. The sheet itself still lives at the
// team layout, so the board's filter bar reaches it through this context
// rather than a prop drilled through every board/list component.
export interface IssueSearchValue {
  open: () => void
}

const IssueSearchContext = createContext<IssueSearchValue>({ open: () => {} })

export function IssueSearchProvider({
  value,
  children,
}: {
  value: IssueSearchValue
  children: ReactNode
}) {
  return (
    <IssueSearchContext.Provider value={value}>
      {children}
    </IssueSearchContext.Provider>
  )
}

/** Never null — outside a team layout (styleguide, tests) opening is a no-op. */
export function useIssueSearch(): IssueSearchValue {
  return useContext(IssueSearchContext)
}
