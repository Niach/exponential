import { eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { fcmTokens } from "@/db/schema"

// ── Relay config (lazy singleton) ─────────────────────────────────────────────

type RelayConfig = { url: string; secret: string }

let relayConfig: RelayConfig | null | undefined

function getRelayConfig(): RelayConfig | null {
  if (relayConfig !== undefined) return relayConfig

  const url = process.env.PUSH_RELAY_URL
  const secret = process.env.PUSH_RELAY_SECRET
  if (!url || !secret) {
    console.warn(
      `[fcm] PUSH_RELAY_URL or PUSH_RELAY_SECRET not set — push notifications disabled`
    )
    relayConfig = null
    return relayConfig
  }
  relayConfig = { url, secret }
  return relayConfig
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
  const cfg = getRelayConfig()
  if (!cfg) return

  const rows = await db
    .select({ token: fcmTokens.token })
    .from(fcmTokens)
    .where(eq(fcmTokens.userId, userId))

  if (rows.length === 0) return

  const tokens = rows.map((r) => r.token)

  let invalidTokens: string[] = []
  try {
    const res = await fetch(`${cfg.url}/send`, {
      method: `POST`,
      headers: {
        "Content-Type": `application/json`,
        Authorization: `Bearer ${cfg.secret}`,
      },
      body: JSON.stringify({
        tokens,
        notification: { title: payload.title, body: payload.body },
        data: payload.data,
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
    await db.delete(fcmTokens).where(inArray(fcmTokens.token, invalidTokens))
  }
}
