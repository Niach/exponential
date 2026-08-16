// Pure attribution helpers (EXP-362) — no DB imports so they stay trivially
// unit-testable. Attribution is COOKIELESS: ref/utm values ride URLs only
// (the marketing site forwards them onto app links at click time), so the
// only server-side capture surfaces are the landing request itself and the
// post-signup claim mutation.

const PARAM_MAX = 128
const URLISH_MAX = 256
// Creem's signed affiliate click token (EXP-384). Unlike a campaign name, a
// truncated token is garbage — over-long values are DROPPED, never sliced.
const CREEM_REF_MAX = 512

export type AttributionParams = {
  ref?: string
  utmSource?: string
  utmMedium?: string
  utmCampaign?: string
}

const truncate = (value: string | null, max: number): string | undefined => {
  const trimmed = value?.trim()
  return trimmed ? trimmed.slice(0, max) : undefined
}

export function extractAttributionParams(url: URL): AttributionParams {
  const params: AttributionParams = {}
  const ref = truncate(url.searchParams.get(`ref`), PARAM_MAX)
  const utmSource = truncate(url.searchParams.get(`utm_source`), PARAM_MAX)
  const utmMedium = truncate(url.searchParams.get(`utm_medium`), PARAM_MAX)
  const utmCampaign = truncate(url.searchParams.get(`utm_campaign`), PARAM_MAX)
  if (ref) params.ref = ref
  if (utmSource) params.utmSource = utmSource
  if (utmMedium) params.utmMedium = utmMedium
  if (utmCampaign) params.utmCampaign = utmCampaign
  return params
}

// Cross-host referrers only — a same-host Referer is internal navigation,
// not an acquisition source.
export function externalReferrer(req: Request): string | undefined {
  const raw = req.headers.get(`referer`)
  if (!raw) return undefined
  try {
    const referrer = new URL(raw)
    if (referrer.host === new URL(req.url).host) return undefined
    return truncate(referrer.href, URLISH_MAX)
  } catch {
    return undefined
  }
}

// Best-effort crawler filter — visitor counts are directional, not exact.
// whatsapp/facebookexternalhit are link unfurlers whose UAs otherwise look
// browser-like (most other unfurlers already match bot|preview).
const BOT_UA =
  /bot|crawl|spider|slurp|preview|fetch|monitor|scrape|curl|wget|headless|whatsapp|facebookexternalhit/i

// Acquisition entry surfaces (EXP-522): only the marketing-reachable front
// door counts as a "landing". Deep app URLs (/t/..., dead route prefixes,
// bot probes like /wp-json/) record nothing — they were the bulk of the
// visitor noise in prod.
const LANDING_PATHS = [`/`, `/auth`]

// Speculative navigations: Chrome's prefetch proxy (Sec-Purpose:
// prefetch;anonymous-client-ip) strips cookies, so a signed-in owner's own
// bookmark otherwise mints a fresh anonymous visitor (EXP-522).
function isSpeculativeRequest(req: Request): boolean {
  const purpose = `${req.headers.get(`sec-purpose`) ?? ``} ${
    req.headers.get(`purpose`) ?? ``
  } ${req.headers.get(`x-moz`) ?? ``}`
  return /prefetch|prerender/i.test(purpose)
}

// The session-cookie substring check matches better-auth's `session_token`
// cookie under any prefix (`better-auth.session_token`, `__Secure-...`).
function hasSessionCookie(req: Request): boolean {
  return (req.headers.get(`cookie`) ?? ``).includes(`session_token`)
}

// A real human browser navigating to a document: GET, HTML accept, not an
// asset path or API, not a speculative prefetch, not a known crawler.
function isDocumentNavigation(req: Request): boolean {
  if (req.method !== `GET`) return false
  const accept = req.headers.get(`accept`) ?? ``
  if (!accept.includes(`text/html`)) return false
  const { pathname } = new URL(req.url)
  if (pathname.startsWith(`/api/`) || pathname.startsWith(`/_`)) return false
  const lastSegment = pathname.split(`/`).pop() ?? ``
  if (lastSegment.includes(`.`)) return false
  if (isSpeculativeRequest(req)) return false
  const ua = req.headers.get(`user-agent`) ?? ``
  if (!ua || BOT_UA.test(ua)) return false
  return true
}

// Anonymous document GETs on an entry path only. Signed-in users are not
// visitors — neither by cookie nor by token credential (native clients and
// API keys are first-class auth, see lib/auth/resolve-bearer.ts).
export function shouldCaptureLanding(req: Request): boolean {
  if (!isDocumentNavigation(req)) return false
  const { pathname } = new URL(req.url)
  if (
    !LANDING_PATHS.some(
      (path) => pathname === path || pathname.startsWith(`${path}/`)
    )
  ) {
    return false
  }
  if (hasSessionCookie(req)) return false
  if (req.headers.get(`authorization`) || req.headers.get(`x-api-key`)) {
    return false
  }
  return true
}

// Signed-in counterpart (EXP-522): any document navigation carrying a session
// cookie is candidate signed-in activity — the caller still resolves the
// session (the cookie may be stale) and dedupes to one row per user per day.
export function shouldCaptureReturnVisit(req: Request): boolean {
  return isDocumentNavigation(req) && hasSessionCookie(req)
}

export function truncateAttributionInput(args: {
  ref?: string | null
  utmSource?: string | null
  utmMedium?: string | null
  utmCampaign?: string | null
  creemRef?: string | null
  referrer?: string | null
  landingPath?: string | null
}): {
  ref: string | null
  utmSource: string | null
  utmMedium: string | null
  utmCampaign: string | null
  creemRef: string | null
  referrer: string | null
  landingPath: string | null
} {
  const creemRef = args.creemRef?.trim()
  return {
    ref: truncate(args.ref ?? null, PARAM_MAX) ?? null,
    utmSource: truncate(args.utmSource ?? null, PARAM_MAX) ?? null,
    utmMedium: truncate(args.utmMedium ?? null, PARAM_MAX) ?? null,
    utmCampaign: truncate(args.utmCampaign ?? null, PARAM_MAX) ?? null,
    creemRef: creemRef && creemRef.length <= CREEM_REF_MAX ? creemRef : null,
    referrer: truncate(args.referrer ?? null, URLISH_MAX) ?? null,
    landingPath: truncate(args.landingPath ?? null, URLISH_MAX) ?? null,
  }
}
