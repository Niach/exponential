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
  reportWhatsappQr,
  reportWhatsappStatus,
} from "./exponential-api"
import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"

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
  notifier: Notifier
) {
  let stopped = false
  let lastPairingRequest: string | null = null
  let pairing = false

  const tick = async () => {
    if (stopped || pairing) return
    const control = await pollControl(config).catch((e) => {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `control poll failed`
      )
      return null
    })
    if (!control?.whatsappPairingRequestedAt) return
    const requestedAt = String(control.whatsappPairingRequestedAt)
    if (requestedAt === lastPairingRequest) return

    lastPairingRequest = requestedAt
    pairing = true
    await notifier
      .pairWhatsapp({
        onQr: async (qr) => reportWhatsappQr(config, qr),
        onStatus: async (status, error) =>
          reportWhatsappStatus(config, status, error),
      })
      .catch(async (e) => {
        const message = e instanceof Error ? e.message : String(e)
        log.warn({ err: message }, `whatsapp pairing failed`)
        await reportWhatsappStatus(config, `error`, message).catch(() => {})
      })
      .finally(() => {
        pairing = false
      })
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
  const dispatcher = startDispatcher({
    config,
    state,
    log,
    pipeline: buildIssuePipeline(),
    notifier,
  })
  const eventSource = await startEventSource({ config, state, log, dispatcher })
  const stopHeartbeat = startHeartbeatLoop(config, log)
  const stopControl = startControlLoop(config, log, notifier)

  const shutdown = async (signal: string) => {
    log.info({ signal }, `shutting down`)
    stopHeartbeat()
    stopControl()
    await eventSource.stop()
    await dispatcher.stop()
    await notifier.stop()
    state.close()
    process.exit(0)
  }
  process.on(`SIGINT`, () => void shutdown(`SIGINT`))
  process.on(`SIGTERM`, () => void shutdown(`SIGTERM`))
}
