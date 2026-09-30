// EXP-1153: the MCP Apps bridge (SEP-1865), hand-rolled. The view is an MCP
// client and the host its server; the transport is postMessage JSON-RPC to
// `window.parent`. Forty lines cover everything the views need, and it keeps
// the bundle free of the SDK's 2.x peer set the web app is not on yet.

export interface HostContext {
  theme?: `light` | `dark`
  styles?: {
    variables?: Record<string, string | undefined>
    css?: { fonts?: string }
  }
  displayMode?: string
  availableDisplayModes?: string[]
  toolInfo?: { tool?: { name?: string } }
  locale?: string
  platform?: string
}

export interface CallToolResult {
  content?: Array<{ type: string; text?: string }>
  structuredContent?: unknown
  isError?: boolean
}

type Handler = (params: Record<string, unknown>) => unknown

interface JsonRpcMessage {
  jsonrpc?: string
  id?: number | string
  method?: string
  params?: Record<string, unknown>
  result?: unknown
  error?: { code?: number; message?: string }
}

export class Bridge {
  private nextId = 1
  private readonly pending = new Map<
    number | string,
    { resolve: (v: unknown) => void; reject: (e: Error) => void }
  >()
  private readonly handlers = new Map<string, Handler>()

  constructor() {
    window.addEventListener(`message`, (event: MessageEvent<JsonRpcMessage>) => {
      const msg = event.data
      if (!msg || msg.jsonrpc !== `2.0`) return
      if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
        const p = this.pending.get(msg.id)
        if (!p) return
        this.pending.delete(msg.id)
        if (msg.error) p.reject(new Error(msg.error.message ?? `error`))
        else p.resolve(msg.result)
        return
      }
      if (!msg.method) return
      const handler = this.handlers.get(msg.method)
      if (!handler) return
      const out = handler(msg.params ?? {})
      // A request (it has an id) wants an answer: ui/resource-teardown.
      if (msg.id !== undefined) {
        const id = msg.id
        Promise.resolve(out).then((result) =>
          window.parent.postMessage({ jsonrpc: `2.0`, id, result: result ?? {} }, `*`)
        )
      }
    })
  }

  request<T = unknown>(method: string, params?: Record<string, unknown>): Promise<T> {
    const id = this.nextId++
    window.parent.postMessage({ jsonrpc: `2.0`, id, method, params: params ?? {} }, `*`)
    return new Promise<T>((resolve, reject) =>
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject })
    )
  }

  notify(method: string, params?: Record<string, unknown>): void {
    window.parent.postMessage({ jsonrpc: `2.0`, method, params: params ?? {} }, `*`)
  }

  on(method: string, handler: Handler): void {
    this.handlers.set(method, handler)
  }

  // ---- the calls the views make ------------------------------------------

  callTool<T>(name: string, args: Record<string, unknown>): Promise<CallToolResult & { structuredContent?: T }> {
    return this.request(`tools/call`, { name, arguments: args })
  }

  openLink(url: string): Promise<unknown> {
    return this.request(`ui/open-link`, { url })
  }

  /** A follow-up typed as the user; the model answers with the tools. */
  sendMessage(text: string): Promise<unknown> {
    return this.request(`ui/message`, { role: `user`, content: { type: `text`, text } })
  }

  /** What is on screen, for the model's next turn (each call replaces the last). */
  updateModelContext(text: string): Promise<unknown> {
    return this.request(`ui/update-model-context`, { content: [{ type: `text`, text }] })
  }

  reportSize(): void {
    const rect = document.body.getBoundingClientRect()
    this.notify(`ui/notifications/size-changed`, {
      width: Math.ceil(rect.width),
      height: Math.ceil(rect.height),
    })
  }
}

/** Parse the JSON a tool returned as text when it sent no structuredContent. */
export function structuredOf<T>(result: CallToolResult): T | null {
  if (result.structuredContent !== undefined) return result.structuredContent as T
  const text = result.content?.find((c) => c.type === `text`)?.text
  if (!text) return null
  try {
    return JSON.parse(text) as T
  } catch {
    return null
  }
}
