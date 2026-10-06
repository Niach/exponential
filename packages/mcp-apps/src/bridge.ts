// EXP-1183 — the view half of the MCP Apps protocol (the stable
// `io.modelcontextprotocol/ui` extension, protocol 2026-01-26): JSON-RPC 2.0
// over `postMessage` to the host frame. Hand-rolled on purpose — the views
// need five messages, and the official SDK would double the bundle.
//
//   view → host  ui/initialize (request), ui/notifications/initialized,
//                tools/call (request), ui/open-link (request),
//                ui/notifications/size-changed
//   host → view  ui/notifications/tool-input, ui/notifications/tool-result,
//                ui/notifications/host-context-changed, ui/resource-teardown

export const MCP_APPS_PROTOCOL_VERSION = `2026-01-26`

/** The MCP `CallToolResult` as the host forwards it. */
export interface ToolResult {
  content?: Array<{ type: string; text?: string }>
  structuredContent?: unknown
  isError?: boolean
}

export interface HostContext {
  theme?: `light` | `dark`
  displayMode?: string
  [key: string]: unknown
}

export interface BridgeHandlers {
  onToolInput?: (args: Record<string, unknown>) => void
  onToolResult?: (result: ToolResult) => void
  onHostContext?: (context: HostContext) => void
}

interface Pending {
  resolve: (value: unknown) => void
  reject: (error: Error) => void
}

type Message = {
  jsonrpc?: string
  id?: number | string
  method?: string
  params?: unknown
  result?: unknown
  error?: { message?: string }
}

export class AppBridge {
  private nextId = 1
  private pending = new Map<number | string, Pending>()
  private readonly target: Window
  private readonly onMessage: (event: MessageEvent) => void

  constructor(
    private readonly handlers: BridgeHandlers,
    private readonly self: Window = window
  ) {
    this.target = self.parent
    this.onMessage = (event) => {
      if (event.source !== this.target) return
      this.receive(event.data as Message)
    }
    self.addEventListener(`message`, this.onMessage)
  }

  /** Handshake; resolves with the host context once the host answered. */
  async connect(): Promise<HostContext> {
    const result = (await this.request(`ui/initialize`, {
      appInfo: { name: `Exponential`, version: `1.0.0` },
      appCapabilities: {},
      protocolVersion: MCP_APPS_PROTOCOL_VERSION,
    })) as { hostContext?: HostContext } | undefined
    this.notify(`ui/notifications/initialized`, {})
    return result?.hostContext ?? {}
  }

  callTool(name: string, args: Record<string, unknown>): Promise<ToolResult> {
    return this.request(`tools/call`, {
      name,
      arguments: args,
    }) as Promise<ToolResult>
  }

  openLink(url: string): void {
    // Web links only: a `javascript:`/`data:` url from tool output never
    // reaches the host, nor the fallback tab.
    if (!/^https?:/i.test(url)) return
    void this.request(`ui/open-link`, { url }).catch(() => {
      // A host without the capability: fall back to a plain new tab.
      this.self.open(url, `_blank`, `noopener`)
    })
  }

  sizeChanged(height: number): void {
    this.notify(`ui/notifications/size-changed`, { height })
  }

  dispose(): void {
    this.self.removeEventListener(`message`, this.onMessage)
    for (const pending of this.pending.values()) {
      pending.reject(new Error(`View closed`))
    }
    this.pending.clear()
  }

  private request(method: string, params: unknown): Promise<unknown> {
    const id = this.nextId++
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject })
      this.post({ jsonrpc: `2.0`, id, method, params })
    })
  }

  private notify(method: string, params: unknown): void {
    this.post({ jsonrpc: `2.0`, method, params })
  }

  private post(message: Message): void {
    this.target.postMessage(message, `*`)
  }

  private receive(message: Message): void {
    if (!message || typeof message !== `object`) return
    // A response to one of ours.
    if (message.id !== undefined && !message.method) {
      const pending = this.pending.get(message.id)
      if (!pending) return
      this.pending.delete(message.id)
      if (message.error) {
        pending.reject(new Error(message.error.message ?? `Request failed`))
      } else {
        pending.resolve(message.result)
      }
      return
    }
    switch (message.method) {
      case `ui/notifications/tool-input`: {
        const params = message.params as { arguments?: Record<string, unknown> }
        this.handlers.onToolInput?.(params?.arguments ?? {})
        return
      }
      case `ui/notifications/tool-result`:
        this.handlers.onToolResult?.((message.params ?? {}) as ToolResult)
        return
      case `ui/notifications/host-context-changed`:
        this.handlers.onHostContext?.((message.params ?? {}) as HostContext)
        return
      case `ui/resource-teardown`:
        if (message.id !== undefined) {
          this.post({ jsonrpc: `2.0`, id: message.id, result: {} })
        }
        return
      default:
        // An unknown host request still gets an answer, never a hang.
        if (message.id !== undefined && message.method) {
          this.post({
            jsonrpc: `2.0`,
            id: message.id,
            error: { message: `Method not found` },
          } as Message)
        }
    }
  }
}
