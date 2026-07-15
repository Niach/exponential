// Minimum-client-version gate. Native clients send an `x-client-version:
// <platform>/<version>` header on every request; when the per-platform
// CLIENT_MIN_VERSION_* env var is set and the client's version compares
// below it, the shared API entries (tRPC + shape proxies) return HTTP 426
// with a JSON body every client is built to recognize as "show the blocking
// update screen". Everything ambiguous fails OPEN: clients shipped before
// the header existed never send it and must keep working, and a malformed
// env var must never brick the whole install base.

export const CLIENT_VERSION_HEADER = `x-client-version`

const PLATFORMS = [`android`, `ios`, `desktop`] as const
export type ClientPlatform = (typeof PLATFORMS)[number]

// Strips pre-release/build suffixes (`0.13.2-staging`, `1.0.0+abc`) and
// parses up to three numeric segments; missing segments count as 0. Null
// when the first segment isn't a plain number.
export function parseVersionTuple(
  version: string
): [number, number, number] | null {
  const core = version.split(/[-+]/, 1)[0]
  const nums = core
    .split(`.`)
    .map((p) => (/^\d+$/.test(p) ? Number(p) : null))
  if (nums[0] === null || nums[0] === undefined) return null
  return [nums[0], nums[1] ?? 0, nums[2] ?? 0]
}

export function parseClientVersionHeader(
  value: string | null
): { platform: ClientPlatform; version: string } | null {
  if (!value) return null
  const slash = value.indexOf(`/`)
  if (slash <= 0) return null
  const platform = value.slice(0, slash).trim().toLowerCase()
  const version = value.slice(slash + 1).trim()
  if (!(PLATFORMS as readonly string[]).includes(platform) || !version)
    return null
  return { platform: platform as ClientPlatform, version }
}

function envVersion(kind: `MIN` | `LATEST`, platform: ClientPlatform) {
  const raw = process.env[`CLIENT_${kind}_VERSION_${platform.toUpperCase()}`]
  const trimmed = raw?.trim()
  return trimmed ? trimmed : null
}

function isBelow(
  version: [number, number, number],
  min: [number, number, number]
): boolean {
  for (let i = 0; i < 3; i++) {
    if (version[i] !== min[i]) return version[i] < min[i]
  }
  return false
}

// Returns the 426 Response when the request identifies itself as a gated
// platform below the configured minimum, else null (allow).
export function checkClientVersion(request: Request): Response | null {
  const parsed = parseClientVersionHeader(
    request.headers.get(CLIENT_VERSION_HEADER)
  )
  if (!parsed) return null

  const min = envVersion(`MIN`, parsed.platform)
  if (!min) return null
  const minTuple = parseVersionTuple(min)
  const versionTuple = parseVersionTuple(parsed.version)
  if (!minTuple || !versionTuple) return null
  if (!isBelow(versionTuple, minTuple)) return null

  return Response.json(
    {
      error: `client_upgrade_required`,
      platform: parsed.platform,
      min,
      latest: envVersion(`LATEST`, parsed.platform),
      message: `This version of Exponential is no longer supported. Please update to continue.`,
    },
    { status: 426, headers: { "cache-control": `no-store` } }
  )
}

// Payload for the permanently-stable GET /api/version endpoint.
export function versionPayload() {
  return Object.fromEntries(
    PLATFORMS.map((platform) => [
      platform,
      { min: envVersion(`MIN`, platform), latest: envVersion(`LATEST`, platform) },
    ])
  )
}
