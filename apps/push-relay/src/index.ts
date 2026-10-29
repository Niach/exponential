import { cert, getApps, initializeApp, type App } from "firebase-admin/app"
import { getMessaging } from "firebase-admin/messaging"
import { createApnsClient, type ApnsClient } from "./apns"
import { createRelayApp } from "./app"
import { consoleLogger as log } from "./log"
import { RelayStore } from "./store"

// The relay's entry point: the environment, firebase-admin, the port. The HTTP
// surface itself lives in `app.ts` (`createRelayApp`), where the tests drive
// it with a fake messaging client.

// ── Relay auth (v1) ───────────────────────────────────────────────────────────

// The v1 /send hands raw device tokens to whoever holds PUSH_RELAY_SECRET:
// without the secret it would be an open endpoint that lets anyone on the
// internet push operator-signed notifications to harvested device tokens. So
// without it the v1 surface does not exist at all (404) — and when the
// accountless mode is not available either, failing to start is the only
// safe posture (EXP-1428 relaxed the old unconditional exit for an
// accountless-only relay).
const RELAY_SECRET = process.env.PUSH_RELAY_SECRET || null

// ── Accountless mode (EXP-1428) ───────────────────────────────────────────────

const DB_PATH = process.env.PUSH_RELAY_DB_PATH || `./data/push-relay.sqlite`

function openStore(): RelayStore | null {
  if (process.env.PUSH_RELAY_ACCOUNTLESS === `false`) return null
  try {
    return RelayStore.open(DB_PATH)
  } catch (err) {
    log.error(`could not open the accountless store at ${DB_PATH} — /v2 disabled:`, err)
    return null
  }
}

function apnsClient(): ApnsClient | null {
  const { APNS_KEY_P8, APNS_KEY_ID, APNS_TEAM_ID } = process.env
  if (!APNS_KEY_P8 || !APNS_KEY_ID || !APNS_TEAM_ID) return null
  try {
    return createApnsClient({ keyP8: APNS_KEY_P8, keyId: APNS_KEY_ID, teamId: APNS_TEAM_ID })
  } catch (err) {
    // Log the failure, never the key.
    log.error(`APNS_KEY_P8 is not a usable ES256 key — APNs disabled: ${err instanceof Error ? err.name : `error`}`)
    return null
  }
}

const store = openStore()
const apns = store ? apnsClient() : null
const apnsTopics = (process.env.APNS_TOPICS ?? ``)
  .split(`,`)
  .map((topic) => topic.trim())
  .filter(Boolean)

if (!RELAY_SECRET && !store) {
  log.error(
    `PUSH_RELAY_SECRET is not set and the accountless store is unavailable — refusing to start. Set the same secret on this process and on the web app, or fix PUSH_RELAY_DB_PATH.`
  )
  process.exit(1)
}
if (!RELAY_SECRET) log.warn(`PUSH_RELAY_SECRET not set — accountless-only mode, v1 /send answers 404`)
if (store && !apns) log.warn(`APNS_KEY_P8/APNS_KEY_ID/APNS_TEAM_ID not set — accountless APNs installs disabled`)
if (apns && apnsTopics.length === 0) log.warn(`APNS_TOPICS is empty — no APNs install can register`)

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
  accountless: store
    ? {
        store,
        apns,
        apnsTopics,
        placeholderTitle: process.env.PUSH_PLACEHOLDER_TITLE || undefined,
        placeholderBody: process.env.PUSH_PLACEHOLDER_BODY || undefined,
      }
    : undefined,
})

const port = parseInt(process.env.PORT ?? `4001`, 10)
log.info(
  `listening on :${port} (v1 ${RELAY_SECRET ? `on` : `off`}, accountless ${store ? `on at ${DB_PATH}` : `off`})`
)

export default {
  port,
  fetch: app.fetch,
}
