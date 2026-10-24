// VAPP-91: the app host — the Devices package (installed explicitly) through the public host API
// with the `exp:devices` binding over the (mocked) synced devices, the
// consent gate in front of harness.mcp, and the banners.

import { describe, expect, it, vi } from "vitest"
import { act, fireEvent, render } from "@testing-library/react"

const mocks = vi.hoisted(() => {
  const now = new Date().toISOString()
  return {
    devices: [
      { id: `d2`, label: `Studio`, kind: `server`, platform: `linux`, version: `0.14.1`, icon: null, lastSeenAt: `2020-01-01T00:00:00.000Z` },
      { id: `d1`, label: `MacBook`, kind: `desktop`, platform: `macos`, version: `0.14.40`, icon: null, lastSeenAt: now },
    ],
    listeners: [] as (() => void)[],
    toast: vi.fn(),
  }
})

vi.mock(`@/lib/collections`, () => {
  const collection = (rows: () => unknown[]) => ({
    get toArray() {
      return rows()
    },
    subscribeChanges: (cb: () => void) => {
      mocks.listeners.push(cb)
      return { unsubscribe: () => {} }
    },
    preload: () => Promise.resolve(),
    utils: { awaitTxId: () => Promise.resolve() },
  })
  return {
    deviceCollection: collection(() => mocks.devices),
    issueCollection: collection(() => []),
    boardCollection: collection(() => []),
    teamCollection: collection(() => []),
    // VAPP-122 / VAPP-119: the records and the team sources read these too.
    appCollection: collection(() => []),
    appRecordCollection: collection(() => []),
    userCollection: collection(() => []),
    teamMemberCollection: collection(() => []),
    sessionCollection: collection(() => []),
    commentCollection: collection(() => []),
    attachmentCollection: collection(() => []),
  }
})
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  const toast = Object.assign(mocks.toast, { error: mocks.toast })
  return { ...actual, toast }
})

import { MemoryTransport, type AppPackage } from "@exponential_app/ui"
import { HostSurface } from "@exponential_app/ui-react"
import { appReactExtension, devicesPackage } from "@exp/ui"
import { contract } from "@exp/domain-contract"
import { exponentialUiConsentPrompt } from "@/lib/prompts"
import { ConsentCard, HostBanner, LIVENESS_TICK_MS, McpCallError, callMcp, createAppHost, createConsentGate, devicesValue, ownPrivateHosts } from "./exponential-ui-host"

/** The Devices template, the SDK's example package (no client preinstalls it). */
const DEVICES = [devicesPackage as unknown as AppPackage]

/** A fetch that answers each tools/call with `answer(id)` (the request's id). */
const answering = (answer: (id: number) => Response) =>
  vi.fn(async (_url: string, init?: RequestInit) => answer(JSON.parse(String(init?.body)).id as number))
const sse = (events: unknown[]) =>
  new Response(events.map((e) => `event: message\ndata: ${JSON.stringify(e)}\n\n`).join(``), {
    status: 200,
    headers: { "content-type": `text/event-stream` },
  })

describe(`VAPP-107: the app opts only its own private origin in`, () => {
  it(`localhost in dev, nothing in the cloud, never the LAN`, () => {
    expect(ownPrivateHosts(`http://localhost:3000`)).toEqual([`localhost`])
    expect(ownPrivateHosts(`http://127.0.0.1:5173`)).toEqual([`127.0.0.1`])
    expect(ownPrivateHosts(`https://app.exponential.at`)).toEqual([])
    expect(ownPrivateHosts(`nope`)).toEqual([])
    const host = createAppHost({ consent: createConsentGate(), navigate: () => {} })
    const origin = window.location.origin
    expect(host.mediaRequest(`${origin}/api/attachments/a1`)?.url).toBe(`${origin}/api/attachments/a1`)
    expect(host.mediaRequest(`/api/attachments/a1`)?.url).toBe(`${origin}/api/attachments/a1`)
    expect(host.mediaRequest(`http://192.168.1.10/cam.jpg`)).toBeNull()
    expect(host.openUrl(`http://169.254.169.254/latest/meta-data/`)).toBe(false)
  })
})

describe(`the app host`, () => {
  it(`renders the Devices package from the synced devices and runs a row press through the policy`, async () => {
    const consent = createConsentGate()
    const host = createAppHost({ consent, navigate: () => {}, packages: DEVICES })
    act(() => {
      host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `devices`, templateId: `devices` } })
    })
    const view = render(<HostSurface host={host} surfaceId="devices" extensions={[appReactExtension]} theme="exponential" />)
    expect(host.surface(`devices`)!.data.devices).toMatchObject({ count: 2, online: 1 })
    const labels = Array.from(view.container.querySelectorAll(`[data-xui-id^="label."]`)).map((e) => e.textContent)
    expect(labels).toEqual([`MacBook`, `Studio`])
    expect(view.getByText(`Desktop · macos · v0.14.40`)).toBeTruthy()
    await act(async () => {
      fireEvent.click(view.container.querySelector(`[data-xui-id="row.0"]`)!)
    })
    expect(mocks.toast).toHaveBeenCalledWith(`MacBook`)
    // A live change re-emits.
    mocks.devices = mocks.devices.slice(0, 1)
    act(() => mocks.listeners.forEach((l) => l()))
    expect(host.surface(`devices`)!.data.devices).toMatchObject({ count: 1 })
  })

  it(`harness.mcp waits for the consent card`, async () => {
    const consent = createConsentGate()
    const host = createAppHost({ consent, navigate: () => {} })
    const fetchMock = answering((id) => new Response(JSON.stringify({ jsonrpc: `2.0`, id, result: { content: [] } }), { status: 200 }))
    vi.stubGlobal(`fetch`, fetchMock)
    const view = render(<ConsentCard gate={consent} />)
    let outcome: Promise<unknown> | undefined
    act(() => {
      outcome = host.callFunction({ surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: { tool: `exponential_issues_list`, arguments: {} } })
    })
    expect(await view.findByText(`Allow this surface to run exponential_issues_list?`)).toBeTruthy()
    expect(fetchMock).not.toHaveBeenCalled()
    await act(async () => {
      fireEvent.click(view.getByRole(`button`, { name: `Allow` }))
    })
    expect(await outcome).toMatchObject({ decision: `allow`, result: { content: [] } })
    expect(fetchMock).toHaveBeenCalledOnce()
    vi.unstubAllGlobals()
  })

  it(`shows host_offline only with a transport, and the catalog-update banner`, () => {
    const transport = new MemoryTransport()
    const local = createAppHost({ consent: createConsentGate(), navigate: () => {} })
    const remote = createAppHost({ consent: createConsentGate(), navigate: () => {}, transport })
    const a = render(<HostBanner host={local} />)
    expect(a.container.textContent).toBe(``)
    const b = render(<HostBanner host={remote} />)
    expect(b.getByTestId(`exponential-ui-host-offline`)).toBeTruthy()
    act(() => remote.connect())
    expect(b.queryByTestId(`exponential-ui-host-offline`)).toBeNull()
    act(() => transport.feed({ version: `v0.9`, createSurface: { surfaceId: `x`, catalogId: `https://ui.exponential.at/catalogs/core/v9` } }))
    expect(b.getByTestId(`exponential-ui-catalog-update`)).toBeTruthy()
  })
})

describe(`VAPP-120: typed actions`, () => {
  it(`a granted action runs without the card and the name's op wins over a forged one`, async () => {
    const bodies: Record<string, unknown>[] = []
    vi.stubGlobal(
      `fetch`,
      vi.fn(async (_url: string, init?: RequestInit) => {
        const body = JSON.parse(String(init?.body)) as { id: number; params: Record<string, unknown> }
        bodies.push(body.params)
        return new Response(JSON.stringify({ jsonrpc: `2.0`, id: body.id, result: { content: [] } }), { status: 200, headers: { "content-type": `application/json` } })
      })
    )
    const ask = vi.fn(async () => false)
    const host = createAppHost({ consent: { ask } as unknown as ReturnType<typeof createConsentGate>, navigate: () => {}, grantedTools: () => [`exponential.issues.get`] })
    const granted = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential.issues.get`, args: { id: `APP-5`, op: `delete` } })
    expect(granted.decision).toBe(`allow`)
    expect(ask).not.toHaveBeenCalled()
    expect(bodies[0]).toMatchObject({ name: `exponential_issues`, arguments: { id: `APP-5`, op: `get` } })
    // An ungranted action asks; a refusal is a deny and no call.
    const denied = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential.issues.delete`, args: { id: `APP-5` } })
    expect(denied.decision).toBe(`deny`)
    expect(ask).toHaveBeenCalledTimes(1)
    expect(bodies).toHaveLength(1)
    vi.unstubAllGlobals()
  })

  it(`a granted send still asks before it stops a session (prompts.json stop-run)`, async () => {
    vi.stubGlobal(`fetch`, answering((id) => new Response(JSON.stringify({ jsonrpc: `2.0`, id, result: { content: [] } }), { status: 200, headers: { "content-type": `application/json` } })))
    const ask = vi.fn(async () => false)
    const host = createAppHost({ consent: { ask } as unknown as ReturnType<typeof createConsentGate>, navigate: () => {}, grantedTools: () => [`exponential.sessions.send`] })
    const sent = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential.sessions.send`, args: { sessionId: `s1`, text: `hi` } })
    expect(sent.decision).toBe(`allow`)
    expect(ask).not.toHaveBeenCalled()
    const stopped = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential.sessions.send`, args: { sessionId: `s1`, stop: true } })
    expect(stopped.decision).toBe(`deny`)
    expect(ask).toHaveBeenCalledTimes(1)
    vi.unstubAllGlobals()
  })
})

describe(`callMcp`, () => {
  it(`rejects a JSON-RPC error with its message and code`, async () => {
    vi.stubGlobal(`fetch`, answering((id) => new Response(JSON.stringify({ jsonrpc: `2.0`, id, error: { code: -32602, message: `Unknown tool: nope` } }), { status: 200, headers: { "content-type": `application/json` } })))
    const outcome = callMcp(`nope`, {})
    await expect(outcome).rejects.toBeInstanceOf(McpCallError)
    await expect(outcome).rejects.toMatchObject({ message: `Unknown tool: nope`, code: -32602 })
    vi.unstubAllGlobals()
  })

  it(`reads the SSE event whose id is the request's, skipping notifications and other ids`, async () => {
    vi.stubGlobal(
      `fetch`,
      answering((id) =>
        sse([
          { jsonrpc: `2.0`, method: `notifications/progress`, params: { progress: 1 } },
          { jsonrpc: `2.0`, id: id + 100, result: { content: [{ type: `text`, text: `not ours` }] } },
          { jsonrpc: `2.0`, id, result: { content: [{ type: `text`, text: `ours` }] } },
        ])
      )
    )
    expect(await callMcp(`exponential_issues_list`, {})).toEqual({ content: [{ type: `text`, text: `ours` }] })
    vi.unstubAllGlobals()
  })

  it(`rejects an SSE error event and a body with no answer for the id`, async () => {
    vi.stubGlobal(`fetch`, answering((id) => sse([{ jsonrpc: `2.0`, id, error: { code: -32000, message: `boom` } }])))
    await expect(callMcp(`x`, {})).rejects.toMatchObject({ message: `boom`, code: -32000 })
    vi.stubGlobal(`fetch`, answering((id) => sse([{ jsonrpc: `2.0`, id: id + 1, result: {} }])))
    await expect(callMcp(`x`, {})).rejects.toThrow(`MCP: no answer`)
    vi.unstubAllGlobals()
  })
})

describe(`exp:devices liveness`, () => {
  const device = (lastSeenAt: string | null) => ({ id: `d`, label: `Mac`, kind: `desktop`, platform: `macos`, version: null, icon: null, lastSeenAt })

  it(`words "Last seen" with the readiness helper (floored)`, () => {
    const now = new Date(`2026-10-09T12:00:00.000Z`)
    const ago = (ms: number) => devicesValue([device(new Date(now.getTime() - ms).toISOString())], now).rows[0].status
    expect(ago(contract.device.onlineWindowSeconds * 1000 + 1)).toBe(`Last seen 1 min ago`)
    expect(ago(119 * 60_000)).toBe(`Last seen 1 h ago`)
    expect(ago(47 * 3_600_000)).toBe(`Last seen 1 d ago`)
    expect(devicesValue([device(null)], now).rows[0].status).toBe(`Offline`)
  })

  it(`re-derives on the liveness tick with no collection change`, () => {
    vi.useFakeTimers()
    try {
      vi.setSystemTime(new Date(`2026-10-09T12:00:00.000Z`))
      mocks.devices = [{ ...mocks.devices[0], id: `d1`, label: `MacBook`, lastSeenAt: new Date().toISOString() }]
      const host = createAppHost({ consent: createConsentGate(), navigate: () => {}, packages: DEVICES })
      act(() => {
        host.receive({ version: `v0.9`, applyTemplate: { surfaceId: `devices`, templateId: `devices` } })
      })
      expect(host.surface(`devices`)!.data.devices).toMatchObject({ online: 1 })
      act(() => {
        vi.advanceTimersByTime(contract.device.onlineWindowSeconds * 1000 + LIVENESS_TICK_MS)
      })
      expect(host.surface(`devices`)!.data.devices).toMatchObject({ online: 0, rows: [{ tone: `idle`, status: `Last seen 2 min ago` }] })
    } finally {
      vi.useRealTimers()
    }
  })
})

describe(`the consent card`, () => {
  it(`VAPP-107: shows the arguments as compact JSON, a disclosure past the preview, the destructive role for a destructive op`, async () => {
    const gate = createConsentGate()
    const view = render(<ConsentCard gate={gate} />)
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: { tool: `exponential_issues`, arguments: { op: `get`, id: `EXP-12` } } } as never)
    })
    const block = await view.findByTestId(`exponential-ui-consent-arguments`)
    expect(block.textContent).toBe(`{"op":"get","id":"EXP-12"}`)
    expect(view.getByRole(`button`, { name: `Allow` }).getAttribute(`data-role`)).toBe(`default`)
    expect(view.queryByTestId(`exponential-ui-consent-show-all`)).toBeNull()
    // A long argument list previews and discloses.
    const long = { op: `create`, boardId: `b`.repeat(40), title: `t`.repeat(120), description: `d`.repeat(300) }
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: { tool: `exponential_issues`, arguments: long } } as never)
    })
    const preview = await view.findByTestId(`exponential-ui-consent-arguments`)
    expect(preview.textContent).toBe(`${JSON.stringify(long).slice(0, 160)}…`)
    fireEvent.click(view.getByTestId(`exponential-ui-consent-show-all`))
    // The CodeBlock paints one element per line: the text joins without newlines.
    expect(view.getByTestId(`exponential-ui-consent-arguments`).textContent).toBe(JSON.stringify(long, null, 2).replace(/\n/g, ``))
    // A destructive op: the destructive entry (Allow destructive, Deny still focused).
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: { tool: `exponential_issues`, arguments: { op: `delete`, id: `EXP-12` } } } as never)
    })
    await vi.waitFor(() => expect(view.getByTestId(`exponential-ui-consent-arguments`).textContent).toBe(`{"op":"delete","id":"EXP-12"}`))
    expect(view.getByRole(`button`, { name: `Allow` }).getAttribute(`data-role`)).toBe(`destructive`)
    expect(document.activeElement).toBe(view.getByRole(`button`, { name: `Deny` }))
    // No arguments: no block. A plain host function shows its own args.
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness.openIssue`, args: { identifier: `EXP-1` } } as never)
    })
    expect((await view.findByTestId(`exponential-ui-consent-arguments`)).textContent).toBe(`{"identifier":"EXP-1"}`)
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: { tool: `exponential_devices` } } as never)
    })
    await view.findByText(`Allow this surface to run exponential_devices?`)
    expect(view.queryByTestId(`exponential-ui-consent-arguments`)).toBeNull()
  })

  it(`reads the contract prompt: Deny focused (Enter denies), Allow a plain default answer`, async () => {
    const gate = createConsentGate()
    const view = render(<ConsentCard gate={gate} />)
    let answer: Promise<boolean> | undefined
    act(() => {
      answer = gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness.mcp`, args: { tool: `exponential_issues_list` } } as never)
    })
    const deny = await view.findByRole(`button`, { name: `Deny` })
    const allow = view.getByRole(`button`, { name: `Allow` })
    expect(document.activeElement).toBe(deny)
    expect(allow).toBeTruthy()
    expect(exponentialUiConsentPrompt(`t`).actions.map((a) => [a.id, a.role])).toEqual([[`deny`, `cancel`], [`allow`, `default`]])
    expect(view.getByText(`It acts as you, with your access to this team.`)).toBeTruthy()
    await act(async () => {
      fireEvent.click(deny)
    })
    expect(await answer).toBe(false)
  })
})
