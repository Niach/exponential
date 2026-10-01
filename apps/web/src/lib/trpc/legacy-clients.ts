import {
  CLIENT_VERSION_HEADER,
  parseClientVersionHeader,
  parseVersionTuple,
  type ClientPlatform,
} from "@/lib/client-version"

// EXP-1158 compat shim. Before EXP-1158 a native client OMITTED `account` on
// a start when the user picked the ambient "Default" login (`system` was
// "the login a start never names"). A device from this release resolves an
// absent account to the LAST USED login instead, so such a start would spend
// another account's quota. These are the FIRST builds per platform that name
// `system` themselves; anything older gets it named by the server
// (`steer.startSession`). Older devices map `system` to the ambient login, so
// the shim is safe against both device generations.
// Delete this file and its one call site once CLIENT_MIN_VERSION_IOS > 0.14.49
// and _ANDROID > 0.14.50 and _DESKTOP/_CLI > 0.14.58.
export const FIRST_VERSION_NAMING_SYSTEM_ACCOUNT: Record<
  ClientPlatform,
  [number, number, number]
> = {
  ios: [0, 14, 50],
  android: [0, 14, 51],
  desktop: [0, 14, 59],
  cli: [0, 14, 59],
}

/**
 * Whether the request comes from a NATIVE build that still omits `account`
 * for the ambient login. Fails closed on everything ambiguous: no header (the
 * web app, MCP callers), an unknown platform or an unparseable version are
 * all "not legacy", so their absent account keeps meaning "last used".
 */
export function omitsSystemAccount(request: Request | undefined): boolean {
  const parsed = parseClientVersionHeader(
    request?.headers.get(CLIENT_VERSION_HEADER) ?? null
  )
  if (!parsed) return false
  const version = parseVersionTuple(parsed.version)
  if (!version) return false
  const first = FIRST_VERSION_NAMING_SYSTEM_ACCOUNT[parsed.platform]
  for (let i = 0; i < 3; i++) {
    if (version[i] !== first[i]) return version[i]! < first[i]!
  }
  return false
}
