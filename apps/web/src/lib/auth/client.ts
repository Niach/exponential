import { createAuthClient } from "better-auth/react"
import {
  genericOAuthClient,
  inferAdditionalFields,
} from "better-auth/client/plugins"
import { creemClient } from "@creem_io/better-auth/client"
import type { auth } from "@/lib/auth"

export const authClient = createAuthClient({
  baseURL:
    typeof window !== `undefined`
      ? window.location.origin
      : undefined,
  plugins: [
    genericOAuthClient(),
    inferAdditionalFields<typeof auth>(),
    creemClient(),
  ],
})

export type SessionData = NonNullable<
  Awaited<ReturnType<typeof authClient.getSession>>[`data`]
>

let cachedSession: SessionData | null = null
let cacheTimestamp = 0
const CACHE_TTL_MS = 30_000

let inflight: Promise<SessionData | null> | null = null
// Bumped on every invalidation so a getSession that was already in flight
// when sign-out cleared the cache can be recognized as stale: its result
// describes the OLD identity and must neither repopulate the cache nor be
// reported as a live session.
let generation = 0

export async function fetchSessionOnce(): Promise<SessionData | null> {
  if (cachedSession && Date.now() - cacheTimestamp < CACHE_TTL_MS) {
    return cachedSession
  }

  if (inflight) return inflight

  const startedGeneration = generation
  inflight = authClient.getSession().then((result) => {
    if (generation !== startedGeneration) {
      // Invalidated (signed out) while this request was in flight — fail
      // closed and leave the module state alone: `inflight` was already
      // cleared (or belongs to a newer request by now).
      return null
    }
    const data = result.data?.session ? (result.data as SessionData) : null
    if (data) {
      cachedSession = data
      cacheTimestamp = Date.now()
    } else if (result.error && cachedSession) {
      // 429 or transient error — return the last known session instead of
      // treating it as "logged out" and redirecting to login.
      cacheTimestamp = Date.now()
    }
    inflight = null
    return data ?? cachedSession
  }).catch(() => {
    if (generation !== startedGeneration) return null
    inflight = null
    return cachedSession
  })

  return inflight
}

export function invalidateSessionCache() {
  cachedSession = null
  cacheTimestamp = 0
  // Also drop the in-flight request: handing it to a later caller (or letting
  // its resolution write the cache) would resurrect the pre-sign-out session
  // for the full cache TTL.
  inflight = null
  generation += 1
}
