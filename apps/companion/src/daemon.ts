import { loadConfig } from "./config"
import { openState } from "./state"
import { createLogger } from "./logger"
import { startEventSource } from "./event-source"
import { startDispatcher } from "./dispatcher"
import { buildIssuePipeline } from "./pipeline"
import { createNotifier, type Notifier } from "./notifier"
import {
  heartbeat,
  pollControl,
  reportGithubIdentity,
  reportWhatsappChats,
  reportWhatsappOwnJid,
  reportWhatsappQr,
  reportWhatsappStatus,
} from "./exponential-api"
import { loadAccessToken } from "./github-auth"
import { getAuthedUser, listAccessibleRepos } from "./github-api"
import { startPrPollLoop } from "./pr-poll-loop"
import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"
import type { Dispatcher } from "./dispatcher"
import type { StateHandle } from "./state"

const ACTIVITY_CURSOR_KEY = `pollControl.activityCursor`

function startGithubIdentityLoop(config: CompanionConfig, log: Logger) {
  let stopped = false
  const tick = async () => {
    if (stopped) return
    const auth = await loadAccessToken().catch(() => null)
    if (!auth) return
    try {
      const user = await getAuthedUser(auth.token)
      const repos = await listAccessibleRepos(auth.token)
      await reportGithubIdentity(config, user.login, repos)
      log.debug(
        { login: user.login, repos: repos.length },
        `github identity refreshed`
      )
    } catch (e) {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `github identity refresh failed`
      )
    }
  }
  void tick()
  const timer = setInterval(() => void tick(), 5 * 60_000)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

function startHeartbeatLoop(config: CompanionConfig, log: Logger) {
  let stopped = false
  const tick = async () => {
    if (stopped) return
    await heartbeat(config).catch((e) =>
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `heartbeat failed`
      )
    )
  }
  void tick()
  const timer = setInterval(() => void tick(), 30_000)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

function startControlLoop(
  config: CompanionConfig,
  log: Logger,
  notifier: Notifier,
  state: StateHandle,
  dispatcher: Dispatcher
) {
  let stopped = false
  let lastPairingRequest: string | null = null
  let pairing = false
  // Track the latest notifyJid we've applied so we only log on change.
  let appliedNotifyJid: string | null = null
  let chatsSubscribed = false

  const ensureChatsSubscription = () => {
    if (chatsSubscribed) return
    chatsSubscribed = true
    notifier.subscribeChats(async (chats) => {
      try {
        await reportWhatsappChats(config, chats)
        log.debug({ count: chats.length }, `reported chat list`)
      } catch (e) {
        log.warn(
          { err: e instanceof Error ? e.message : String(e) },
          `chat-list report failed`
        )
      }
    })
  }

  // Subscribe even before pairing so the moment a client appears (either at
  // boot from existing creds or fresh after pairWhatsapp) we start shipping
  // the chat list.
  ensureChatsSubscription()

  const tick = async () => {
    if (stopped) return
    const activityCursor = state.kvGet(ACTIVITY_CURSOR_KEY)
    const control = await pollControl(config, { activityCursor }).catch((e) => {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `control poll failed`
      )
      return null
    })
    if (!control) return

    // Fallback for when the Electric ShapeStream isn't delivering live events
    // (e.g., reverse proxy stripping long-poll headers). Re-emit each updated
    // issue as a dispatcher `updated` event so the REENTRY gate can pick it
    // up. The dispatcher's per-issue dedupe keeps duplicates harmless when the
    // ShapeStream IS working.
    if (control.activity) {
      for (const issue of control.activity.issues) {
        dispatcher.enqueue({
          type: `updated`,
          issueId: issue.id,
          identifier: issue.identifier,
          title: issue.title,
          projectId: issue.projectId,
          assigneeId: issue.assigneeId,
        })
      }
      if (control.activity.issues.length > 0) {
        log.info(
          {
            count: control.activity.issues.length,
            ids: control.activity.issues.map((i) => i.identifier),
          },
          `poll-control activity → dispatcher`
        )
      }
      state.kvSet(ACTIVITY_CURSOR_KEY, control.activity.cursor)
    }


    // Apply runtime notify target. null reverts to self-chat default.
    const desired = control.whatsappNotifyJid ?? null
    if (desired !== appliedNotifyJid) {
      notifier.setRuntimeNotifyJid(desired)
      appliedNotifyJid = desired
      log.info({ notifyJid: desired }, `runtime notifyJid updated`)
    }

    // If a pairing was requested and we haven't acted on it yet, do so.
    if (control.whatsappPairingRequestedAt && !pairing) {
      const requestedAt = String(control.whatsappPairingRequestedAt)
      if (requestedAt !== lastPairingRequest) {
        lastPairingRequest = requestedAt
        pairing = true
        try {
          await notifier.pairWhatsapp({
            onQr: async (qr) => reportWhatsappQr(config, qr),
            onStatus: async (status, error) =>
              reportWhatsappStatus(config, status, error),
            onOwnJid: async (jid) => {
              await reportWhatsappOwnJid(config, jid).catch((e) =>
                log.warn(
                  { err: e instanceof Error ? e.message : String(e) },
                  `own-jid report failed`
                )
              )
            },
          })
          // Re-subscribe to chat updates against the new client.
          chatsSubscribed = false
          ensureChatsSubscription()
        } catch (e) {
          const message = e instanceof Error ? e.message : String(e)
          log.warn({ err: message }, `whatsapp pairing failed`)
          await reportWhatsappStatus(config, `error`, message).catch(() => {})
        } finally {
          pairing = false
        }
      }
    }
  }

  void tick()
  const timer = setInterval(() => void tick(), 5_000)
  return () => {
    stopped = true
    clearInterval(timer)
  }
}

export async function runDaemon() {
  const config = await loadConfig()
  const log = createLogger(config)
  const state = openState()

  log.info({ baseUrl: config.exponential.baseUrl }, `daemon starting`)

  const notifier = await createNotifier({ config, log })

  // If the daemon already has Baileys creds from a previous pairing, the
  // notifier has just reconnected and we know our own JID. Push it to the
  // server so the web UI's "Message yourself" default has a target.
  const existingOwnJid = notifier.getOwnJid()
  if (existingOwnJid) {
    void reportWhatsappOwnJid(config, existingOwnJid).catch((e) =>
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `initial own-jid report failed`
      )
    )
  }

  const dispatcher = startDispatcher({
    config,
    state,
    log,
    pipeline: buildIssuePipeline(),
    notifier,
  })
  const eventSource = await startEventSource({ config, state, log, dispatcher })
  const stopHeartbeat = startHeartbeatLoop(config, log)
  const stopControl = startControlLoop(config, log, notifier, state, dispatcher)
  const stopGithubIdentity = startGithubIdentityLoop(config, log)
  const prPoll = startPrPollLoop({ config, state, log })

  const shutdown = async (signal: string) => {
    log.info({ signal }, `shutting down`)
    stopHeartbeat()
    stopControl()
    stopGithubIdentity()
    prPoll.stop()
    await eventSource.stop()
    await dispatcher.stop()
    await notifier.stop()
    state.close()
    process.exit(0)
  }
  process.on(`SIGINT`, () => void shutdown(`SIGINT`))
  process.on(`SIGTERM`, () => void shutdown(`SIGTERM`))
}
