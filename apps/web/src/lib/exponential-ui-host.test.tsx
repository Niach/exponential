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
import { ConsentCard, HostBanner, createAppHost, createConsentGate } from "./exponential-ui-host"

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
    const fetchMock = vi.fn(async () => new Response(JSON.stringify({ jsonrpc: `2.0`, id: 1, result: { content: [] } }), { status: 200 }))
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
