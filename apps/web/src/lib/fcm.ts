import { cert, getApps, initializeApp, type App } from "firebase-admin/app"
import { getMessaging } from "firebase-admin/messaging"
import { eq, inArray } from "drizzle-orm"
import { db } from "@/db/connection"
import { fcmTokens } from "@/db/schema"

let app: App | null | undefined

function getApp(): App | null {
  if (app !== undefined) return app
  const raw = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
  if (!raw) {
    console.warn(
      `[fcm] FIREBASE_SERVICE_ACCOUNT_JSON not set — push notifications disabled`
    )
    app = null
    return app
  }
  try {
    const credentials = JSON.parse(raw)
    app =
      getApps()[0] ??
      initializeApp({
        credential: cert({
          projectId: credentials.project_id,
          clientEmail: credentials.client_email,
          privateKey: credentials.private_key,
        }),
      })
    return app
  } catch (err) {
    console.error(`[fcm] Failed to init firebase-admin:`, err)
    app = null
    return app
  }
}

export type FcmPayload = {
  title: string
  body?: string
  data: Record<string, string>
}

export async function sendToUser(
  userId: string,
  payload: FcmPayload
): Promise<void> {
  const firebase = getApp()
  if (!firebase) return

  const tokens = await db
    .select({ token: fcmTokens.token })
    .from(fcmTokens)
    .where(eq(fcmTokens.userId, userId))

  if (tokens.length === 0) return

  const tokenStrings = tokens.map((row) => row.token)
  const messaging = getMessaging(firebase)
  const response = await messaging.sendEachForMulticast({
    tokens: tokenStrings,
    notification: { title: payload.title, body: payload.body },
    data: payload.data,
    android: {
      priority: `high`,
      notification: { channelId: `issues_default` },
    },
  })

  // Prune dead tokens — codes from
  // https://firebase.google.com/docs/cloud-messaging/manage-tokens
  const dead: string[] = []
  response.responses.forEach((res, i) => {
    if (res.success) return
    const code = res.error?.code
    if (
      code === `messaging/registration-token-not-registered` ||
      code === `messaging/invalid-registration-token`
    ) {
      dead.push(tokenStrings[i])
    } else {
      console.error(`[fcm] send error for token`, tokenStrings[i], res.error)
    }
  })
  if (dead.length > 0) {
    await db.delete(fcmTokens).where(inArray(fcmTokens.token, dead))
  }
}
