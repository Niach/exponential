import { useEffect, useState } from "react"

// EXP-698: the app's floating bottom chrome — the desktop agent dock, the
// mobile tab bar — paints OVER whatever scrolls beneath it, so every scroller
// underneath has to reserve exactly that much bottom clearance. Each bar
// publishes its MEASURED height as a CSS custom property on the document root
// and the scrollers spend it as `pb-[var(--name,0px)]`; a hard-coded guess
// drifts the moment the bar resizes (a dock drag, the safe-area inset, one
// more tab).
//
// Returns a REF CALLBACK, not a ref object: the bars mount and unmount their
// own element mid-session (the dock only exists once there is a session), and
// a callback re-runs the observer when the node itself changes.
//
// A bar that is GONE publishes `0px` — it does NOT remove the property. The
// consumers' `var(--tabbar-h, 4.25rem)` literal is a first-paint estimate for
// the frames before the observer has measured anything; removing the property
// hands that estimate back and reserves a phantom bar's height on exactly the
// routes that hide the bar. The property is only dropped when the hook itself
// goes away.
export function useChromeHeightVar(
  name: string,
  enabled = true
): (node: HTMLElement | null) => void {
  const [node, setNode] = useState<HTMLElement | null>(null)

  useEffect(() => {
    const root = document.documentElement
    const remove = () => root.style.removeProperty(name)
    if (!node || !enabled) {
      root.style.setProperty(name, `0px`)
      return remove
    }
    const publish = () => {
      root.style.setProperty(
        name,
        `${Math.round(node.getBoundingClientRect().height)}px`
      )
    }
    // One measurement is right even without a ResizeObserver; only the
    // "keeps up with a resize" half of the contract needs one.
    publish()
    if (typeof ResizeObserver === `undefined`) return remove
    const observer = new ResizeObserver(publish)
    observer.observe(node)
    return () => {
      observer.disconnect()
      remove()
    }
  }, [name, node, enabled])

  return setNode
}
