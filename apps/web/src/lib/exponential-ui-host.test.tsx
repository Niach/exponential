// VAPP-91: the app host — the Devices package through the public host API
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
  })
  return { deviceCollection: collection(() => mocks.devices), issueCollection: collection(() => []), boardCollection: collection(() => []), teamCollection: collection(() => []) }
})
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  const toast = Object.assign(mocks.toast, { error: mocks.toast })
  return { ...actual, toast }
})

import { MemoryTransport } from "@exponential-at/ui"
import { HostSurface } from "@exponential-at/ui-react"
import { appReactExtension } from "@exp/ui"
import { contract } from "@exp/domain-contract"
import { exponentialUiConsentPrompt } from "@/lib/prompts"
import { ConsentCard, HostBanner, LIVENESS_TICK_MS, McpCallError, callMcp, createAppHost, createConsentGate, devicesValue } from "./exponential-ui-host"

/** A fetch that answers each tools/call with `answer(id)` (the request's id). */
const answering = (answer: (id: number) => Response) =>
  vi.fn(async (_url: string, init?: RequestInit) => answer(JSON.parse(String(init?.body)).id as number))
const sse = (events: unknown[]) =>
  new Response(events.map((e) => `event: message\ndata: ${JSON.stringify(e)}\n\n`).join(``), {
    status: 200,
    headers: { "content-type": `text/event-stream` },
  })

describe(`the app host`, () => {
  it(`renders the Devices package from the synced devices and runs a row press through the policy`, async () => {
    const consent = createConsentGate()
    const host = createAppHost({ consent, navigate: () => {} })
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
      const host = createAppHost({ consent: createConsentGate(), navigate: () => {} })
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
