import makeWASocket, {
  DisconnectReason,
  useMultiFileAuthState,
  type WASocket,
} from "@whiskeysockets/baileys"
import { mkdir } from "node:fs/promises"
import type { Logger } from "../logger"

export interface WhatsappClient {
  sendText(jid: string, text: string): Promise<void>
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

  const wireEvents = (s: WASocket) => {
    s.ev.on(`connection.update`, (u) => {
      if (u.qr && args.onQr) {
        args.log.info(`whatsapp pairing QR received`)
        void args.onQr?.(u.qr)
      }
      if (u.connection === `open`) {
        args.log.info(`whatsapp connected`)
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
  }
  wireEvents(sock)

  if (args.waitForConnection) await connectedP

  return {
    sendText: async (jid, text) => {
      await sock.sendMessage(jid, { text })
    },
    stop: async () => {
      stopRequested = true
      sock.end(undefined)
    },
  }
}
