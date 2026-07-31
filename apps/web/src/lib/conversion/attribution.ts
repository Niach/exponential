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
const BOT_UA =
  /bot|crawl|spider|slurp|preview|fetch|monitor|scrape|curl|wget|headless/i

// Paths that are never a human "landing": APIs, widget assets, the helpdesk
// magic-link surface (its URL is a credential), router internals.
const EXCLUDED_PREFIXES = [`/api/`, `/widget/`, `/support`, `/_`]

// Document GETs from anonymous browsers only. The session-cookie substring
// check matches better-auth's `session_token` cookie under any prefix
// (`better-auth.session_token`, `__Secure-...`) — signed-in users are not
// visitors.
export function shouldCaptureLanding(req: Request): boolean {
  if (req.method !== `GET`) return false
  const accept = req.headers.get(`accept`) ?? ``
  if (!accept.includes(`text/html`)) return false
  const { pathname } = new URL(req.url)
  if (EXCLUDED_PREFIXES.some((prefix) => pathname.startsWith(prefix))) {
    return false
  }
  const lastSegment = pathname.split(`/`).pop() ?? ``
  if (lastSegment.includes(`.`)) return false
  const cookies = req.headers.get(`cookie`) ?? ``
  if (cookies.includes(`session_token`)) return false
  const ua = req.headers.get(`user-agent`) ?? ``
  if (!ua || BOT_UA.test(ua)) return false
  return true
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
