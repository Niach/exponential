import type { CompanionConfig } from "./config"
import type { Logger } from "./logger"
import {
  connectWhatsapp,
  type WhatsappChat,
  type WhatsappClient,
} from "./messaging/whatsapp"

export interface Notifier {
  onPrOpened(args: {
    identifier: string
    title: string
    url: string
  }): Promise<void>
  onPipelineError(args: {
    identifier: string
    title: string
    error: string
  }): Promise<void>
  onPlanReady(args: {
    identifier: string
    title: string
    planSummary: string
  }): Promise<void>
  onQuestionsAsked(args: {
    identifier: string
    title: string
    count: number
  }): Promise<void>
  pairWhatsapp(args: {
    onQr: (qr: string) => Promise<void>
    onStatus: (
      status: `connected` | `disconnected` | `error`,
      error?: string | null
    ) => Promise<void>
    onOwnJid?: (jid: string) => Promise<void>
    onChats?: (chats: WhatsappChat[]) => Promise<void>
  }): Promise<void>
  /**
   * Override the configured notify target at runtime. Pass `null` to revert
   * to the default (self-chat = the daemon's own JID).
   */
  setRuntimeNotifyJid(jid: string | null): void
  getOwnJid(): string | null
  /**
   * Subscribe to chat-list updates from the WhatsApp client. Returns an
   * unsubscribe function. Subscriptions installed before pairing complete
   * are queued and applied to the next client created via pairWhatsapp().
   */
  subscribeChats(handler: (chats: WhatsappChat[]) => void): () => void
  stop(): Promise<void>
}

export async function createNotifier(args: {
  config: CompanionConfig
  log: Logger
}): Promise<Notifier> {
  const { config, log } = args
  const wa = config.messaging?.whatsapp
  let whatsapp: WhatsappClient | null = null
  let runtimeNotifyJid: string | null = null
  const pendingChatHandlers = new Set<(chats: WhatsappChat[]) => void>()
  const activeChatUnsubs: Array<() => void> = []

  const reattachChatHandlers = (client: WhatsappClient) => {
    while (activeChatUnsubs.length > 0) activeChatUnsubs.pop()?.()
    for (const h of pendingChatHandlers) {
      activeChatUnsubs.push(client.onChatsUpdated(h))
    }
  }

  if (wa?.enabled) {
    try {
      whatsapp = await connectWhatsapp({
        authStateDir: wa.authStateDir,
        log,
      })
      reattachChatHandlers(whatsapp)
      log.info({ jid: wa.notifyJid }, `whatsapp client ready`)
    } catch (e) {
      log.warn(
        { err: e instanceof Error ? e.message : String(e) },
        `whatsapp connect failed; notifications disabled`
      )
    }
  }

  const resolveTarget = (): string | null => {
    if (!whatsapp) return null
    if (runtimeNotifyJid !== null) return runtimeNotifyJid
    if (wa?.notifyJid) return wa.notifyJid
    return whatsapp.getOwnJid()
  }

  const send = async (text: string) => {
    const target = resolveTarget()
    if (!whatsapp || !target) return
    try {
      await whatsapp.sendText(target, text)
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
    onPipelineError: async ({ identifier, title, error }) => {
      await send(
        `❌ Agent error on [${identifier}] ${title}\n\n${error.slice(0, 500)}`
      )
    },
    onPlanReady: async ({ identifier, title, planSummary }) => {
      const summary =
        planSummary.length > 400 ? `${planSummary.slice(0, 400)}…` : planSummary
      await send(
        `📝 Plan ready for [${identifier}] ${title}\n\n${summary}\n\nReview & approve in the issue.`
      )
    },
    onQuestionsAsked: async ({ identifier, title, count }) => {
      await send(
        `❓ Agent needs input on [${identifier}] ${title}\n\n${count} question${count === 1 ? `` : `s`} posted — answer in the comments.`
      )
    },
    pairWhatsapp: async ({ onQr, onStatus, onOwnJid, onChats }) => {
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
        onStatus: async (status, err) => {
          await onStatus(status, err)
          if (status === `connected` && whatsapp && onOwnJid) {
            const ownJid = whatsapp.getOwnJid()
            if (ownJid) await onOwnJid(ownJid)
          }
        },
      })
      if (onChats) pendingChatHandlers.add(onChats)
      reattachChatHandlers(whatsapp)
    },
    setRuntimeNotifyJid: (jid) => {
      runtimeNotifyJid = jid
    },
    getOwnJid: () => whatsapp?.getOwnJid() ?? null,
    subscribeChats: (handler) => {
      pendingChatHandlers.add(handler)
      const unsub = whatsapp?.onChatsUpdated(handler)
      if (unsub) activeChatUnsubs.push(unsub)
      return () => {
        pendingChatHandlers.delete(handler)
        unsub?.()
      }
    },
    stop: async () => {
      while (activeChatUnsubs.length > 0) activeChatUnsubs.pop()?.()
      pendingChatHandlers.clear()
      if (whatsapp) await whatsapp.stop().catch(() => {})
    },
  }
}
