import * as React from "react"

const MOBILE_BREAKPOINT = 768

export function useIsMobile() {
  // Synchronous initial read: the app is client-only (`defaultSsr: false`),
  // so starting from the real viewport width avoids a first-render flash of
  // the desktop layout (e.g. the fixed-width issue properties panel) on
  // mobile viewports.
  const [isMobile, setIsMobile] = React.useState<boolean>(
    () =>
      typeof window !== `undefined` &&
      window.innerWidth < MOBILE_BREAKPOINT
  )

  React.useEffect(() => {
    if (typeof window.matchMedia !== `function`) {
      setIsMobile(window.innerWidth < MOBILE_BREAKPOINT)
      return
    }

    const mql = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`)
    const onChange = () => {
      setIsMobile(window.innerWidth < MOBILE_BREAKPOINT)
    }
    mql.addEventListener(`change`, onChange)
    setIsMobile(window.innerWidth < MOBILE_BREAKPOINT)
    return () => mql.removeEventListener(`change`, onChange)
  }, [])

  return isMobile
}
