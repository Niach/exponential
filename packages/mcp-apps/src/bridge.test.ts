import { AppBridge, MCP_APPS_PROTOCOL_VERSION } from "./bridge"

// A host frame stand-in: `parent` records what the view posts, and `deliver`
// plays a host message into the view.
function fakeWindow() {
  const posted: Array<Record<string, unknown>> = []
  const listeners: Array<(event: MessageEvent) => void> = []
  const parent = {
    postMessage: (message: Record<string, unknown>) => posted.push(message),
  }
  const self = {
    parent,
    addEventListener: (_: string, listener: (event: MessageEvent) => void) =>
      listeners.push(listener),
    removeEventListener: () => {},
    open: vi.fn(),
  } as unknown as Window
  const deliver = (data: unknown) =>
    listeners.forEach((listener) =>
      listener({ source: parent, data } as unknown as MessageEvent)
    )
  return { self, posted, deliver }
}

describe(`AppBridge`, () => {
  it(`handshakes, then forwards tool results`, async () => {
    const { self, posted, deliver } = fakeWindow()
    const onToolResult = vi.fn()
    const bridge = new AppBridge({ onToolResult }, self)
    const connected = bridge.connect()
    expect(posted[0]).toMatchObject({
      method: `ui/initialize`,
      params: { protocolVersion: MCP_APPS_PROTOCOL_VERSION },
    })
    deliver({ jsonrpc: `2.0`, id: posted[0].id, result: { hostContext: { theme: `light` } } })
    await expect(connected).resolves.toEqual({ theme: `light` })
    expect(posted[1]).toMatchObject({ method: `ui/notifications/initialized` })
    deliver({ jsonrpc: `2.0`, method: `ui/notifications/tool-result`, params: { content: [] } })
    expect(onToolResult).toHaveBeenCalledWith({ content: [] })
  })

  it(`answers a teardown and rejects a failed call`, async () => {
    const { self, posted, deliver } = fakeWindow()
    const bridge = new AppBridge({}, self)
    deliver({ jsonrpc: `2.0`, id: 9, method: `ui/resource-teardown`, params: {} })
    expect(posted.at(-1)).toMatchObject({ id: 9, result: {} })
    const call = bridge.callTool(`exponential_issues_get`, { id: `x` })
    const request = posted.at(-1) as { id: number }
    deliver({ jsonrpc: `2.0`, id: request.id, error: { message: `denied` } })
    await expect(call).rejects.toThrow(`denied`)
  })

  it(`opens only http(s) links, on the host path and the fallback`, async () => {
    const { self, posted, deliver } = fakeWindow()
    const bridge = new AppBridge({}, self)
    for (const url of [`javascript:alert(1)`, `data:text/html,x`, `/relative`]) {
      bridge.openLink(url)
    }
    expect(posted).toHaveLength(0)

    bridge.openLink(`https://exponential.at/x`)
    const request = posted.at(-1) as { id: number }
    expect(request).toMatchObject({
      method: `ui/open-link`,
      params: { url: `https://exponential.at/x` },
    })
    // A host without the capability: the fallback tab takes the same url.
    deliver({ jsonrpc: `2.0`, id: request.id, error: { message: `Method not found` } })
    await vi.waitFor(() =>
      expect(self.open).toHaveBeenCalledWith(
        `https://exponential.at/x`,
        `_blank`,
        `noopener`
      )
    )
    expect(self.open).toHaveBeenCalledTimes(1)
  })
})
