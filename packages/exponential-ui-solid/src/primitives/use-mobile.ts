import { createSignal, onCleanup, onMount, type Accessor } from "solid-js"

const MOBILE_BREAKPOINT = 768

/** Below the mobile breakpoint (React's `useIsMobile`): read synchronously
 *  at creation (no first-frame desktop flash), then followed. */
export function createIsMobile(): Accessor<boolean> {
  const narrow = () => typeof window !== `undefined` && window.innerWidth < MOBILE_BREAKPOINT
  const [isMobile, setIsMobile] = createSignal(narrow())
  onMount(() => {
    if (typeof window.matchMedia !== `function`) {
      setIsMobile(narrow())
      return
    }
    const mql = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`)
    const onChange = () => setIsMobile(narrow())
    mql.addEventListener(`change`, onChange)
    setIsMobile(narrow())
    onCleanup(() => mql.removeEventListener(`change`, onChange))
  })
  return isMobile
}
