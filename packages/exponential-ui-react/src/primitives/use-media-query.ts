import { useCallback, useSyncExternalStore } from "react"

/** Whether `query` matches right now, following changes. Without a
 *  `window` (SSR, static markup) or `matchMedia` it answers `false`. */
export function useMediaQuery(query: string): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      if (typeof window === `undefined` || typeof window.matchMedia !== `function`) return () => {}
      const mql = window.matchMedia(query)
      mql.addEventListener(`change`, onChange)
      return () => mql.removeEventListener(`change`, onChange)
    },
    [query]
  )
  const read = () =>
    typeof window !== `undefined` && typeof window.matchMedia === `function` && window.matchMedia(query).matches
  return useSyncExternalStore(subscribe, read, () => false)
}
