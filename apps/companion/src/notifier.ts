import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"
import { connectWhatsapp, type WhatsappClient } from "./messaging/whatsapp"

export interface Notifier {
  onPrOpened(args: {
    identifier: string
    title: string
    url: string
  }): Promise<void>
  onTestsFailed(args: {
    identifier: string
    title: string
    tail: string
  }): Promise<void>
  onPipelineError(args: {
    identifier: string
    title: string
    error: string
  }): Promise<void>
  pairWhatsapp(args: {
    onQr: (qr: string) => Promise<void>
    onStatus: (
      status: `connected` | `disconnected` | `error`,
      error?: string | null
    ) => Promise<void>
  }): Promise<void>
  stop(): Promise<void>
}

export async function createNotifier(args: {
  config: CompanionConfig
  log: Logger
}): Promise<Notifier> {
  const { config, log } = args
  const wa = config.messaging?.whatsapp
  let whatsapp: WhatsappClient | null = null
  if (wa?.enabled) {
    try {
      whatsapp = await connectWhatsapp({
        authStateDir: wa.authStateDir,
        log,
      })
      log.info({ jid: wa.notifyJid }, `whatsapp client ready`)
    } catch (e) {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `whatsapp connect failed; notifications disabled`
      )
    }
  }

  const send = async (text: string) => {
    if (!whatsapp || !wa?.notifyJid) return
    try {
      await whatsapp.sendText(wa.notifyJid, text)
    } catch (e) {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `whatsapp send failed`
      )
    }
  }

  return {
    onPrOpened: async ({ identifier, title, url }) => {
      await send(`✅ [${identifier}] ${title}\n${url}`)
    },
    onTestsFailed: async ({ identifier, title, tail }) => {
      const snippet = tail.length > 500 ? `${tail.slice(0, 500)}…` : tail
      await send(`⚠️ Tests failed for [${identifier}] ${title}\n\n${snippet}`)
    },
    onPipelineError: async ({ identifier, title, error }) => {
      await send(
        `❌ Agent error on [${identifier}] ${title}\n\n${error.slice(0, 500)}`
      )
    },
    pairWhatsapp: async ({ onQr, onStatus }) => {
      if (!wa?.enabled) {
        throw new Error(`WhatsApp is not enabled in companion config`)
      }
      if (whatsapp) {
        await whatsapp.stop().catch(() => {})
        whatsapp = null
      }
      whatsapp = await connectWhatsapp({
        authStateDir: wa.authStateDir,
        log,
        waitForConnection: true,
        onQr,
        onStatus,
      })
    },
    stop: async () => {
      if (whatsapp) await whatsapp.stop().catch(() => {})
    },
  }
}
