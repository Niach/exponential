import { eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { fcmTokens } from "@/db/schema"

// ── Relay config (lazy singleton) ─────────────────────────────────────────────

let relayUrl: string | null | undefined

function getRelayUrl(): string | null {
  if (relayUrl !== undefined) return relayUrl

  const url = process.env.PUSH_RELAY_URL
  if (!url) {
    console.warn(`[fcm] PUSH_RELAY_URL not set — push notifications disabled`)
    relayUrl = null
    return relayUrl
  }
  relayUrl = url
  return relayUrl
}

// ── Types ─────────────────────────────────────────────────────────────────────

export type FcmPayload = {
  title: string
  body?: string
  data: Record<string, string>
}

// ── Main export ───────────────────────────────────────────────────────────────

export async function sendToUser(
  userId: string,
  payload: FcmPayload
): Promise<void> {
  const url = getRelayUrl()
  if (!url) return

  const rows = await db
    .select({ token: fcmTokens.token })
    .from(fcmTokens)
    .where(eq(fcmTokens.userId, userId))

  if (rows.length === 0) return

  const tokens = rows.map((r) => r.token)

  const headers: Record<string, string> = {
    "Content-Type": `application/json`,
  }
  const relaySecret = process.env.PUSH_RELAY_SECRET
  if (relaySecret) {
    headers[`x-relay-secret`] = relaySecret
  }

  let invalidTokens: string[] = []
  try {
    const res = await fetch(`${url}/send`, {
      method: `POST`,
      headers,
      body: JSON.stringify({
        tokens,
        notification: { title: payload.title, body: payload.body },
        // The recipient's user id rides along so multi-account clients can
        // route a tapped notification into the signed-in account it belongs
        // to instead of whichever account happens to be active.
        data: { ...payload.data, userId },
      }),
    })

    if (!res.ok) {
      console.error(`[fcm] relay responded ${res.status}:`, await res.text())
      return
    }

    const json = (await res.json()) as { invalidTokens?: string[] }
    invalidTokens = json.invalidTokens ?? []
  } catch (err) {
    console.error(`[fcm] relay request failed:`, err)
    return
  }

  if (invalidTokens.length > 0) {
    // FCM invalidates a token for the whole device, so delete every account's
    // registration of it — not just the notified user's row.
    await db.delete(fcmTokens).where(inArray(fcmTokens.token, invalidTokens))
  }
}
