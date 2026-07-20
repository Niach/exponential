export interface OriginCheckResult {
  allowed: boolean
  // The origin to echo in Access-Control-Allow-Origin (never `*`: the
  // allowlist makes the response origin-dependent, so we always echo +
  // `Vary: Origin`). Null when the request carried no usable origin.
  echoOrigin: string | null
}

function parseOrigin(value: string | null): URL | null {
  if (!value) return null
  try {
    const url = new URL(value)
    if (url.protocol !== `http:` && url.protocol !== `https:`) return null
    return url
  } catch {
    return null
  }
}

// Pattern grammar: `hostname` or `hostname:port`, optionally with a leading
// `*.` wildcard. A pattern without a port matches any port; `*.example.com`
// matches subdomains only (list the apex separately). Matching is
// case-insensitive on the host part.
function matchesPattern(origin: URL, pattern: string): boolean {
  const trimmed = pattern.trim().toLowerCase()
  if (!trimmed) return false

  const [hostPattern, portPattern] = trimmed.split(`:`)
  if (portPattern !== undefined) {
    const originPort = origin.port || (origin.protocol === `https:` ? `443` : `80`)
    if (originPort !== portPattern) return false
  }

  const hostname = origin.hostname.toLowerCase()
  if (hostPattern.startsWith(`*.`)) {
    const base = hostPattern.slice(2)
    return hostname !== base && hostname.endsWith(`.${base}`)
  }
  return hostname === hostPattern
}

// Origin headers are advisory outside browsers (curl can forge anything);
// this is browser-facing scoping, with rate limiting as the real backstop.
export function isOriginAllowed(
  originHeader: string | null,
  refererHeader: string | null,
  allowedDomains: string[]
): OriginCheckResult {
  const origin = parseOrigin(originHeader) ?? parseOrigin(refererHeader)
  const echoOrigin = origin ? origin.origin : null

  if (allowedDomains.length === 0) {
    // Unconfigured allowlist blocks the key entirely — allow-all was
    // removed in EXP-209 (anyone could lift a public key off a page and
    // submit from anywhere).
    return { allowed: false, echoOrigin: null }
  }

  if (!origin) {
    // Restricted key + no Origin/Referer: deny rather than guess.
    return { allowed: false, echoOrigin: null }
  }

  const allowed = allowedDomains.some((pattern) =>
    matchesPattern(origin, pattern)
  )
  return { allowed, echoOrigin: allowed ? echoOrigin : null }
}
