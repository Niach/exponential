// Client-side first-touch attribution (EXP-362) — a plain in-memory module
// variable, deliberately NOT a cookie and NOT local/sessionStorage: nothing
// is persisted on the device, so no EU consent banner is ever needed. The
// params ride URLs instead (the marketing site forwards them onto app links;
// the auth flow threads them through the OAuth callbackURL and the
// post-signup redirect), and each full page load re-captures them here.

export type FirstTouch = {
  ref?: string
  utmSource?: string
  utmMedium?: string
  utmCampaign?: string
  referrer?: string
  landingPath?: string
}

const ATTRIBUTION_KEYS = [
  `ref`,
  `utm_source`,
  `utm_medium`,
  `utm_campaign`,
] as const

let captured: FirstTouch | null = null

function capture(): FirstTouch {
  if (captured) return captured
  const result: FirstTouch = {}
  if (typeof window !== `undefined`) {
    const params = new URLSearchParams(window.location.search)
    const ref = params.get(`ref`)?.trim()
    const utmSource = params.get(`utm_source`)?.trim()
    const utmMedium = params.get(`utm_medium`)?.trim()
    const utmCampaign = params.get(`utm_campaign`)?.trim()
    if (ref) result.ref = ref
    if (utmSource) result.utmSource = utmSource
    if (utmMedium) result.utmMedium = utmMedium
    if (utmCampaign) result.utmCampaign = utmCampaign
    // Only a cross-origin referrer is an acquisition source; internal
    // navigation (or the OAuth provider bouncing back) is not.
    const referrer = document.referrer
    if (referrer) {
      try {
        if (new URL(referrer).origin !== window.location.origin) {
          result.referrer = referrer
        }
      } catch {
        // ignore unparsable referrers
      }
    }
    if (Object.keys(result).length > 0) {
      result.landingPath = window.location.pathname
    }
  }
  captured = result
  return result
}

/** The first-touch attribution seen by this page load (captured lazily). */
export function getFirstTouch(): FirstTouch {
  return capture()
}

export function hasFirstTouchParams(): boolean {
  const touch = capture()
  return Boolean(
    touch.ref || touch.utmSource || touch.utmMedium || touch.utmCampaign
  )
}

// Thread the captured ref/utm params onto an app-internal URL (the OAuth
// callbackURL or the post-signup redirect) so they survive the full-page
// navigations of the auth flow and the next load re-captures them.
export function withFirstTouchParams(url: string): string {
  const touch = capture()
  if (!hasFirstTouchParams()) return url
  try {
    const target = new URL(url, window.location.origin)
    const pairs: Array<[string, string | undefined]> = [
      [`ref`, touch.ref],
      [`utm_source`, touch.utmSource],
      [`utm_medium`, touch.utmMedium],
      [`utm_campaign`, touch.utmCampaign],
    ]
    for (const [key, value] of pairs) {
      if (value && !target.searchParams.has(key)) {
        target.searchParams.set(key, value)
      }
    }
    // Keep relative URLs relative — Better Auth clamps callbackURL origins.
    return url.startsWith(`http`)
      ? target.href
      : `${target.pathname}${target.search}${target.hash}`
  } catch {
    return url
  }
}

export function attributionParamKeys(): readonly string[] {
  return ATTRIBUTION_KEYS
}
