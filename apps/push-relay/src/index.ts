import { cert, getApps, initializeApp, type App } from "firebase-admin/app"
import { getMessaging } from "firebase-admin/messaging"
import { createRelayApp } from "./app"
import { consoleLogger as log } from "./log"

// The relay's entry point: the environment, firebase-admin, the port. The HTTP
// surface itself lives in `app.ts` (`createRelayApp`), where the tests drive
// it with a fake messaging client.

// ── Relay auth (mandatory) ────────────────────────────────────────────────────

// The secret is REQUIRED: without it the relay would be an open endpoint that
// lets anyone on the internet push operator-signed notifications to harvested
// device tokens. Failing to start is the only safe posture.
const RELAY_SECRET = requireRelaySecret()

function requireRelaySecret(): string {
  const secret = process.env.PUSH_RELAY_SECRET
  if (!secret) {
    log.error(
      `PUSH_RELAY_SECRET is not set — refusing to start. Set the same secret on this process and on the web app.`
    )
    process.exit(1)
  }
  return secret
}

// ── Firebase init (lazy singleton) ───────────────────────────────────────────

let firebaseApp: App | null | undefined

function getFirebaseApp(): App | null {
  if (firebaseApp !== undefined) return firebaseApp

  const raw = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
  if (!raw) {
    log.warn(`FIREBASE_SERVICE_ACCOUNT_JSON not set — relay disabled`)
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
    log.error(`Failed to init firebase-admin:`, err)
    firebaseApp = null
    return firebaseApp
  }
}

// ── Start ─────────────────────────────────────────────────────────────────────

const { app } = createRelayApp({
  secret: RELAY_SECRET,
  trustProxy: process.env.TRUST_PROXY === `true`,
  getMessaging: () => {
    const firebase = getFirebaseApp()
    return firebase ? getMessaging(firebase) : null
  },
})

const port = parseInt(process.env.PORT ?? `4001`, 10)
log.info(`listening on :${port}`)

export default {
  port,
  fetch: app.fetch,
}
