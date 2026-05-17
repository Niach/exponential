import { Hono } from "hono"
import { cert, getApps, initializeApp, type App } from "firebase-admin/app"
import { getMessaging } from "firebase-admin/messaging"
import { z } from "zod"

// ── Firebase init (lazy singleton) ───────────────────────────────────────────

let firebaseApp: App | null | undefined

function getFirebaseApp(): App | null {
  if (firebaseApp !== undefined) return firebaseApp

  const raw = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
  if (!raw) {
    console.warn(`[push-relay] FIREBASE_SERVICE_ACCOUNT_JSON not set — relay disabled`)
    firebaseApp = null
    return firebaseApp
  }
  try {
    const creds = JSON.parse(raw)
    firebaseApp =
      getApps()[0] ??
      initializeApp({
        credential: cert({
          projectId: creds.project_id,
          clientEmail: creds.client_email,
          privateKey: creds.private_key,
        }),
      })
    return firebaseApp
  } catch (err) {
    console.error(`[push-relay] Failed to init firebase-admin:`, err)
    firebaseApp = null
    return firebaseApp
  }
}

// ── Validation ────────────────────────────────────────────────────────────────

const sendSchema = z.object({
  tokens: z.array(z.string().min(1)).min(1).max(500),
  notification: z.object({
    title: z.string().min(1),
    body: z.string().optional(),
  }),
  data: z.record(z.string(), z.string()),
})

// ── Dead-token error codes per Firebase docs ──────────────────────────────────

const DEAD_CODES = new Set([
  `messaging/registration-token-not-registered`,
  `messaging/invalid-registration-token`,
])

// ── Hono app ──────────────────────────────────────────────────────────────────

const app = new Hono()

// Unauthenticated health check for Docker HEALTHCHECK / uptime monitors
app.get(`/healthz`, (c) => c.json({ ok: true }))

app.post(`/send`, async (c) => {
  const firebase = getFirebaseApp()
  if (!firebase) {
    return c.json({ error: `Firebase not configured` }, 503)
  }

  const body = await c.req.json().catch(() => null)
  const parsed = sendSchema.safeParse(body)
  if (!parsed.success) {
    return c.json({ error: `Bad request`, issues: parsed.error.issues }, 400)
  }

  const { tokens, notification, data } = parsed.data
  const messaging = getMessaging(firebase)

  let response
  try {
    response = await messaging.sendEachForMulticast({
      tokens,
      notification: { title: notification.title, body: notification.body },
      data,
      android: {
        priority: `high`,
        notification: { channelId: `issues_default` },
      },
      apns: {
        headers: { "apns-priority": `10` },
        payload: {
          aps: {
            alert: { title: notification.title, body: notification.body },
            sound: `default`,
            contentAvailable: true,
          },
        },
      },
    })
  } catch (err) {
    console.error(`[push-relay] FCM multicast failed:`, err)
    return c.json({ error: `FCM error` }, 500)
  }

  const invalidTokens: string[] = []
  response.responses.forEach((res, i) => {
    if (res.success) return
    const code = res.error?.code
    if (code && DEAD_CODES.has(code)) {
      invalidTokens.push(tokens[i])
    } else {
      console.error(`[push-relay] send error token=${tokens[i]}`, res.error)
    }
  })

  return c.json({ invalidTokens })
})

// ── Start ─────────────────────────────────────────────────────────────────────

const port = parseInt(process.env.PORT ?? `4001`, 10)
console.log(`[push-relay] listening on :${port}`)

export default {
  port,
  fetch: app.fetch,
}
