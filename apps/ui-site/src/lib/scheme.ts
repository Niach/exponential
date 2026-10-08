/* The site's light/dark scheme: <html data-scheme> + the theme-color meta
   (both set before paint by the inline script in index.html), switched by
   the top bar and stored per browser. Live renders follow it (a surface's
   `mode`). */
import { useEffect, useState } from "react"

export type Scheme = `light` | `dark`
const KEY = `xui-site-scheme`
const EVENT = `xui-site-scheme`

/** The browser bar colour per scheme (index.html's script carries the same pair). */
export const THEME_COLOR: Record<Scheme, string> = { light: `#fafafa`, dark: `#0a0a0a` }

function apply(scheme: Scheme) {
  document.documentElement.dataset.scheme = scheme
  document.querySelector(`meta[name="theme-color"]`)?.setAttribute(`content`, THEME_COLOR[scheme])
}

export const currentScheme = (): Scheme => (typeof document !== `undefined` && document.documentElement.dataset.scheme === `light` ? `light` : `dark`)

export function setScheme(scheme: Scheme) {
  apply(scheme)
  try {
    localStorage.setItem(KEY, scheme)
  } catch {
    /* private mode: the choice lasts the page */
  }
  window.dispatchEvent(new CustomEvent(EVENT))
}

/** The scheme, `dark` in the prerender and the first client render (so
 *  hydration matches), the real one after. */
export function useScheme(): Scheme {
  const [scheme, set] = useState<Scheme>(`dark`)
  useEffect(() => {
    const sync = () => set(currentScheme())
    sync()
    const media = window.matchMedia?.(`(prefers-color-scheme: light)`)
    const onMedia = () => {
      let stored: string | null = null
      try {
        stored = localStorage.getItem(KEY)
      } catch {
        /* ignore */
      }
      if (stored) return
      apply(media!.matches ? `light` : `dark`)
      sync()
    }
    window.addEventListener(EVENT, sync)
    media?.addEventListener(`change`, onMedia)
    return () => {
      window.removeEventListener(EVENT, sync)
      media?.removeEventListener(`change`, onMedia)
    }
  }, [])
  return scheme
}

/** A light/dark switch that follows the site scheme until the visitor picks.
 *  `mode` feeds the renderer; `cls` the stylesheet (`auto` until a pick, so
 *  the prerender's colours come from <html data-scheme>, not from `dark`). */
export function useModePick() {
  const scheme = useScheme()
  const [pick, setMode] = useState<Scheme | null>(null)
  return { mode: pick ?? scheme, cls: pick ?? (`auto` as const), setMode }
}
