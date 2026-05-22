import makeWASocket, {
  DisconnectReason,
  useMultiFileAuthState,
  type WASocket,
} from "@whiskeysockets/baileys"
import { mkdir } from "node:fs/promises"
import type { Logger } from "../logger"

export interface WhatsappChat {
  jid: string
  name: string
  isGroup: boolean
}

export interface WhatsappClient {
  sendText(jid: string, text: string): Promise<void>
  getOwnJid(): string | null
  /**
   * Subscribe to chat-list updates. The handler fires with the full chat set
   * known to the daemon, debounced ~5s after the last underlying Baileys
   * event so consumers receive coalesced snapshots rather than a stream.
   */
  onChatsUpdated(handler: (chats: WhatsappChat[]) => void): () => void
  stop(): Promise<void>
}

interface ConnectArgs {
  authStateDir: string
  log: Logger
  waitForConnection?: boolean
  onQr?: (qr: string) => void | Promise<void>
  onStatus?: (
    status: `connected` | `disconnected` | `error`,
    error?: string | null
  ) => void | Promise<void>
}

function chatNameFrom(chat: { id?: string | null; name?: string | null }): string {
  if (chat.name && chat.name.trim().length > 0) return chat.name.trim()
  const id = chat.id ?? ``
  // Render 1:1 JIDs as the phone number, group JIDs as their handle.
  return id.split(`@`)[0] ?? id
}

function isGroupJid(jid: string): boolean {
  return jid.endsWith(`@g.us`)
}

export async function connectWhatsapp(
  args: ConnectArgs
): Promise<WhatsappClient> {
  await mkdir(args.authStateDir, { recursive: true })
  const { state, saveCreds } = await useMultiFileAuthState(args.authStateDir)

  let sock: WASocket = makeWASocket({
    auth: state,
    printQRInTerminal: false,
  })
  sock.ev.on(`creds.update`, saveCreds)

  let connectedResolver: (() => void) | null = null
  const connectedP = new Promise<void>((r) => (connectedResolver = r))
  let stopRequested = false
  let ownJid: string | null = null

  // Chat list state. Keyed by JID so updates merge.
  const chats = new Map<string, WhatsappChat>()
  const chatHandlers = new Set<(chats: WhatsappChat[]) => void>()
  let chatsTimer: ReturnType<typeof setTimeout> | null = null
  const scheduleChatsEmit = () => {
    if (chatsTimer) return
    chatsTimer = setTimeout(() => {
      chatsTimer = null
      if (chatHandlers.size === 0) return
      const snapshot = Array.from(chats.values()).sort((a, b) =>
        a.name.localeCompare(b.name)
      )
      for (const h of chatHandlers) {
        try {
          h(snapshot)
        } catch (e) {
          args.log.warn(
            { err: e instanceof Error ? e.message : String(e) },
            `chat handler threw`
          )
        }
      }
    }, 5_000)
  }

  const upsertChat = (chat: { id?: string | null; name?: string | null }) => {
    const jid = chat.id ?? ``
    if (!jid) return
    chats.set(jid, {
      jid,
      name: chatNameFrom(chat),
      isGroup: isGroupJid(jid),
    })
  }

  const wireEvents = (s: WASocket) => {
    s.ev.on(`connection.update`, (u) => {
      if (u.qr && args.onQr) {
        args.log.info(`whatsapp pairing QR received`)
        void args.onQr?.(u.qr)
      }
      if (u.connection === `open`) {
        ownJid = s.user?.id ?? null
        args.log.info({ ownJid }, `whatsapp connected`)
        void args.onStatus?.(`connected`, null)
        connectedResolver?.()
      } else if (u.connection === `close`) {
        const code = (
          u.lastDisconnect?.error as
            | { output?: { statusCode?: number } }
            | undefined
        )?.output?.statusCode
        const loggedOut = code === DisconnectReason.loggedOut
        if (loggedOut) {
          args.log.error(
            `whatsapp session logged out. Request a new pairing code from the web app.`
          )
          void args.onStatus?.(`disconnected`, `WhatsApp session logged out`)
          return
        }
        if (stopRequested) return
        void args.onStatus?.(`disconnected`, null)
        args.log.warn({ code }, `whatsapp disconnected, reconnecting in 5s`)
        setTimeout(() => {
          sock = makeWASocket({ auth: state, printQRInTerminal: false })
          sock.ev.on(`creds.update`, saveCreds)
          wireEvents(sock)
        }, 5000)
      }
    })

    s.ev.on(`chats.upsert`, (incoming) => {
      for (const chat of incoming) upsertChat(chat)
      scheduleChatsEmit()
    })
    s.ev.on(`chats.update`, (incoming) => {
      for (const chat of incoming) upsertChat(chat)
      scheduleChatsEmit()
    })
    s.ev.on(`messaging-history.set`, ({ chats: history }) => {
      for (const chat of history ?? []) upsertChat(chat)
      scheduleChatsEmit()
    })
  }
  wireEvents(sock)

  if (args.waitForConnection) await connectedP

  return {
    sendText: async (jid, text) => {
      await sock.sendMessage(jid, { text })
    },
    getOwnJid: () => ownJid,
    onChatsUpdated: (handler) => {
      chatHandlers.add(handler)
      // Emit the current snapshot immediately so late subscribers don't wait
      // for the next event before learning what's already known.
      if (chats.size > 0) {
        queueMicrotask(() =>
          handler(
            Array.from(chats.values()).sort((a, b) =>
              a.name.localeCompare(b.name)
            )
          )
        )
      }
      return () => chatHandlers.delete(handler)
    },
    stop: async () => {
      stopRequested = true
      if (chatsTimer) {
        clearTimeout(chatsTimer)
        chatsTimer = null
      }
      sock.end(undefined)
    },
  }
}
