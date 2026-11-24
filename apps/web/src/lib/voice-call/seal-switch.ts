// #EXP-1941: THE switch of the call seal, per CALLER platform
// (`VOICE_CALL_SEAL=web,desktop,android`), alone in a module without the
// database. Unset = no call is sealed (today's clear call). A platform is
// named only once a real call from it has been proven sealed: the release
// shepherd sets it, no deploy. It never forces a seal: `voice.startCall`
// still needs the caller's claim and the host's `call-seal` cap.
import { CLIENT_VERSION_HEADER, parseClientVersionHeader } from "@/lib/client-version"

export const CALL_SEAL_PLATFORMS = [`web`, `desktop`, `android`, `ios`] as const
export type CallSealPlatform = (typeof CALL_SEAL_PLATFORMS)[number]

/** The caller platforms whose calls may be sealed (read per call, so tests can flip it). */
export function callSealPlatforms(): Set<CallSealPlatform> {
  const named = (process.env.VOICE_CALL_SEAL ?? ``).split(`,`).map((part) => part.trim().toLowerCase())
  return new Set(CALL_SEAL_PLATFORMS.filter((platform) => named.includes(platform)))
}

/** Which platform places this call. A phone says which one in its
 *  `x-client-version` header; a phone that does not is none of them, so its
 *  call is never sealed. */
export function callSealPlatformOf(platform: `phone` | `web` | `desktop`, request: Request): CallSealPlatform | null {
  if (platform !== `phone`) return platform
  const client = parseClientVersionHeader(request.headers.get(CLIENT_VERSION_HEADER))?.platform
  return client === `ios` || client === `android` ? client : null
}
