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

export async function fetchSessionOnce(): Promise<SessionData | null> {
  const result = await authClient.getSession()
  return result.data?.session ? (result.data as SessionData) : null
}
