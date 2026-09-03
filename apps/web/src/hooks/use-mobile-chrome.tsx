import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useState,
  type ReactNode,
} from "react"

// EXP-698 r5 — the phone's bottom edge has ONE occupant. A multi-selection
// puts the bulk bar in the tab bar's slot (native parity: iOS suppresses
// `MobileTabBar`, Android its `BottomNavBar`), so the bar announces itself
// here and the tab bar + its FAB step aside for as long as the selection
// lives. Modelled on `use-issue-search.tsx`: the team layout owns the state,
// the two bars reach it through context rather than a prop drilled through
// every board/list component.
export interface MobileChromeValue {
  bulkBarPresent: boolean
  setBulkBarPresent: (present: boolean) => void
}

const MobileChromeContext = createContext<MobileChromeValue>({
  bulkBarPresent: false,
  setBulkBarPresent: () => {},
})

export function MobileChromeProvider({ children }: { children: ReactNode }) {
  // A COUNT, not a flag: the board view and My Issues each mount their own
  // bar, and a route change can overlap the new bar's mount with the old
  // one's unmount — a plain boolean would then be left false with a bar on
  // screen.
  const [bars, setBars] = useState(0)
  const setBulkBarPresent = useCallback((present: boolean) => {
    setBars((count) => Math.max(0, count + (present ? 1 : -1)))
  }, [])
  const value = useMemo(
    () => ({ bulkBarPresent: bars > 0, setBulkBarPresent }),
    [bars, setBulkBarPresent]
  )
  return (
    <MobileChromeContext.Provider value={value}>
      {children}
    </MobileChromeContext.Provider>
  )
}

/** Never null — outside a team layout (styleguide, tests) it is inert. */
export function useMobileChrome(): MobileChromeValue {
  return useContext(MobileChromeContext)
}
