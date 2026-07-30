/* Cookieless attribution forwarding (EXP-362). The marketing site stores
   NOTHING on the device — no cookies, no local/sessionStorage, so no EU
   consent banner. Arriving ref/utm params live in a plain module variable
   for the current page load and are appended just-in-time to clicked links:
   - app links (app.exponential.at): the app's server captures them and a
     fresh signup claims them;
   - internal marketing links: the params stay alive in the URL across this
     MPA's full page loads, so a visitor who browses before clicking
     "Sign in" still carries their source. */

const PARAM_KEYS = [`ref`, `utm_source`, `utm_medium`, `utm_campaign`] as const

const APP_ORIGIN = `https://app.exponential.at`

let params: Array<[string, string]> = []
let installed = false

function captureFromLocation(): void {
  const search = new URLSearchParams(window.location.search)
  params = PARAM_KEYS.flatMap((key) => {
    const value = search.get(key)?.trim()
    return value ? [[key, value] as [string, string]] : []
  })
}

function isInternalPage(url: URL): boolean {
  return (
    url.origin === window.location.origin &&
    !/\.[a-z0-9]+$/i.test(url.pathname.split(`/`).pop() ?? ``)
  )
}

export function withAttributionParams(href: string): string {
  if (params.length === 0) return href
  try {
    const url = new URL(href, window.location.origin)
    if (!url.href.startsWith(APP_ORIGIN) && !isInternalPage(url)) return href
    for (const [key, value] of params) {
      if (!url.searchParams.has(key)) url.searchParams.set(key, value)
    }
    return url.href
  } catch {
    return href
  }
}

export function initAttributionForwarding(): void {
  if (installed || typeof window === `undefined`) return
  installed = true
  captureFromLocation()
  if (params.length === 0) return
  // One capture-phase listener rewrites hrefs just-in-time — covers every
  // CTA (including prerendered markup) without touching React rendering, so
  // hydration never mismatches.
  document.addEventListener(
    `click`,
    (event) => {
      const target = event.target as Element | null
      const anchor = target?.closest?.(`a[href]`)
      if (!(anchor instanceof HTMLAnchorElement)) return
      const rewritten = withAttributionParams(anchor.href)
      if (rewritten !== anchor.href) anchor.href = rewritten
    },
    { capture: true }
  )
}
