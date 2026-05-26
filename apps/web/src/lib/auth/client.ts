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

export async function fetchSessionOnce(): Promise<SessionData | null> {
  if (cachedSession && Date.now() - cacheTimestamp < CACHE_TTL_MS) {
    return cachedSession
  }

  if (inflight) return inflight

  inflight = authClient.getSession().then((result) => {
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
    inflight = null
    return cachedSession
  })

  return inflight
}

export function invalidateSessionCache() {
  cachedSession = null
  cacheTimestamp = 0
}
