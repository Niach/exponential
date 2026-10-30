import { cert, getApps, initializeApp, type App } from "firebase-admin/app"
import { getMessaging } from "firebase-admin/messaging"
import { createApnsClient, type ApnsClient } from "./apns"
import { createRelayApp } from "./app"
import { consoleLogger as log } from "./log"
import { RelayStore } from "./store"

// The relay's entry point: the environment, firebase-admin, the APNs key, the
// store, the port. The HTTP surface itself lives in `app.ts`
// (`createRelayApp`), where the tests drive it with fakes.

// ── Operator secret (compat shim + operator routes) ──────────────────────────

// PUSH_RELAY_SECRET gates the deprecated /send compat shim (compat-send.ts),
// /stats and /healthz?deep=1. /send hands raw device tokens to whoever holds
// it, so without the secret those routes do not exist at all (404). The push
// API itself never needs it.
const RELAY_SECRET = process.env.PUSH_RELAY_SECRET || null

// ── Push API store (EXP-1428) ─────────────────────────────────────────────────

const DB_PATH = process.env.PUSH_RELAY_DB_PATH || `./data/push-relay.sqlite`

// The push API IS the relay: without its store there is nothing to serve,
// so failing to start is the only honest posture.
function openStore(): RelayStore {
  try {
    return RelayStore.open(DB_PATH)
  } catch (err) {
    log.error(`could not open the push API store at ${DB_PATH} — refusing to start. Fix PUSH_RELAY_DB_PATH (and its volume).`, err)
    process.exit(1)
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
const apns = apnsClient()
const apnsTopics = (process.env.APNS_TOPICS ?? ``)
  .split(`,`)
  .map((topic) => topic.trim())
  .filter(Boolean)

if (!RELAY_SECRET) log.info(`PUSH_RELAY_SECRET not set — /send compat shim, /stats and /healthz?deep=1 answer 404`)
if (!apns) log.warn(`APNS_KEY_P8/APNS_KEY_ID/APNS_TEAM_ID not set — APNs installs disabled`)
if (apns && apnsTopics.length === 0) log.warn(`APNS_TOPICS is empty — no APNs install can register`)

// ── Firebase init (lazy singleton) ───────────────────────────────────────────

let firebaseApp: App | null | undefined

function getFirebaseApp(): App | null {
  if (firebaseApp !== undefined) return firebaseApp

  const raw = process.env.FIREBASE_SERVICE_ACCOUNT_JSON
  if (!raw) {
    log.warn(`FIREBASE_SERVICE_ACCOUNT_JSON not set — FCM disabled`)
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
  api: {
    store,
    apns,
    apnsTopics,
    placeholderTitle: process.env.PUSH_PLACEHOLDER_TITLE || undefined,
    placeholderBody: process.env.PUSH_PLACEHOLDER_BODY || undefined,
  },
})

const port = parseInt(process.env.PORT ?? `4001`, 10)
log.info(
  `listening on :${port} (push API store ${DB_PATH}, fcm ${getFirebaseApp() ? `on` : `off`}, apns ${apns ? `on` : `off`}, /send compat shim ${RELAY_SECRET ? `on` : `off`})`
)

export default {
  port,
  fetch: app.fetch,
}
