import { createSignal, onCleanup, type Accessor } from "solid-js"

/** Whether `query` matches right now, following changes (React's
 *  `useMediaQuery`). Without a `window` or `matchMedia` it answers `false`. */
export function createMediaQuery(query: string): Accessor<boolean> {
  if (typeof window === `undefined` || typeof window.matchMedia !== `function`) return () => false
  const mql = window.matchMedia(query)
  const [matches, setMatches] = createSignal(mql.matches)
  const onChange = () => setMatches(window.matchMedia(query).matches)
  mql.addEventListener(`change`, onChange)
  onCleanup(() => mql.removeEventListener(`change`, onChange))
  return matches
}
