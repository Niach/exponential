// VAPP-91: the app host — the Devices package (installed explicitly) through the public host API
// with the `exp:devices` binding over the (mocked) synced devices, the
// consent gate in front of harness_mcp, and the banners.

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
    teams: [] as unknown[],
    apps: [] as unknown[],
    toast: vi.fn(),
    upstreamRead: vi.fn(async (_input: { source: string }): Promise<unknown> => ({ ok: true, value: { mrr: 1200 } })),
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
    teamCollection: collection(() => mocks.teams),
    // VAPP-122 / VAPP-119: the records and the team sources read these too.
    appCollection: collection(() => mocks.apps),
    appRecordCollection: collection(() => []),
    userCollection: collection(() => []),
    teamMemberCollection: collection(() => []),
    sessionCollection: collection(() => []),
    commentCollection: collection(() => []),
    attachmentCollection: collection(() => []),
    issueEventCollection: collection(() => []),
    sessionIssueCollection: collection(() => []),
  }
})
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { apps: { upstreamRead: { query: (input: { source: string }) => mocks.upstreamRead(input) } } },
}))
vi.mock(`@exp/ui`, async (importOriginal) => {
  const actual = (await importOriginal()) as Record<string, unknown>
  const toast = Object.assign(mocks.toast, { error: mocks.toast })
  return { ...actual, toast }
})

import { ExponentialHost, MemoryTransport, type AppPackage } from "@exponential_app/eui"
import { mediaRequestOf } from "@exponential_app/eui/web"
import { HostSurface, hostPlugin } from "@exponential_app/eui-react"
import appsFixture from "@exp/domain-contract/fixtures/apps.json"
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
    const a1 = `/api/attachments/0b6c1a52-3e1f-4d7a-9c55-2f8e4a1b7c90`
    expect(host.mediaRequest(`${origin}${a1}`)?.url).toBe(`${origin}${a1}`)
    expect(host.mediaRequest(a1)?.url).toBe(`${origin}${a1}`)
    expect(host.mediaRequest(`http://192.168.1.10/cam.jpg`)).toBeNull()
    expect(host.openUrl(`http://169.254.169.254/latest/meta-data/`)).toBe(false)
  })
})

describe(`the app host`, () => {
  it(`renders the Devices package from the synced devices and runs a row press through the policy`, async () => {
    const consent = createConsentGate()
    const host = createAppHost({ consent, navigate: () => {}, packages: DEVICES })
    act(() => {
      host.applyTemplate({ surfaceId: `devices`, templateId: `devices` })
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

  it(`harness_mcp waits for the consent card`, async () => {
    const consent = createConsentGate()
    const host = createAppHost({ consent, navigate: () => {} })
    const fetchMock = answering((id) => new Response(JSON.stringify({ jsonrpc: `2.0`, id, result: { content: [] } }), { status: 200 }))
    vi.stubGlobal(`fetch`, fetchMock)
    const view = render(<ConsentCard gate={consent} />)
    let outcome: Promise<unknown> | undefined
    act(() => {
      outcome = host.callFunction({ surfaceId: `s`, componentId: `b`, name: `harness_mcp`, args: { tool: `exponential_issues_list`, arguments: {} } })
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
    act(() => transport.feed({ version: `1.0`, createSurface: { surfaceId: `x`, catalogId: `https://ui.exponential.at/catalogs/core/v9` } }))
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
    const host = createAppHost({ consent: { ask } as unknown as ReturnType<typeof createConsentGate>, navigate: () => {}, grantedTools: () => [`exponential_issues_get`] })
    const granted = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential_issues_get`, args: { id: `APP-5`, op: `delete` } })
    expect(granted.decision).toBe(`allow`)
    expect(ask).not.toHaveBeenCalled()
    expect(bodies[0]).toMatchObject({ name: `exponential_issues`, arguments: { id: `APP-5`, op: `get` } })
    // An ungranted action asks; a refusal is a deny and no call.
    const denied = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential_issues_delete`, args: { id: `APP-5` } })
    expect(denied.decision).toBe(`deny`)
    expect(ask).toHaveBeenCalledTimes(1)
    expect(bodies).toHaveLength(1)
    vi.unstubAllGlobals()
  })

  it(`a granted send still asks before it stops a session (prompts.json stop-run)`, async () => {
    vi.stubGlobal(`fetch`, answering((id) => new Response(JSON.stringify({ jsonrpc: `2.0`, id, result: { content: [] } }), { status: 200, headers: { "content-type": `application/json` } })))
    const ask = vi.fn(async () => false)
    const host = createAppHost({ consent: { ask } as unknown as ReturnType<typeof createConsentGate>, navigate: () => {}, grantedTools: () => [`exponential_sessions_send`] })
    const sent = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential_sessions_send`, args: { sessionId: `s1`, text: `hi` } })
    expect(sent.decision).toBe(`allow`)
    expect(ask).not.toHaveBeenCalled()
    const stopped = await host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential_sessions_send`, args: { sessionId: `s1`, stop: true } })
    expect(stopped.decision).toBe(`deny`)
    expect(ask).toHaveBeenCalledTimes(1)
    vi.unstubAllGlobals()
  })
})

describe(`VAPP-158: owner-only actions`, () => {
  it(`a stored grant of an owner-only action is ignored: the call still asks`, async () => {
    const fetchMock = vi.fn()
    vi.stubGlobal(`fetch`, fetchMock)
    const ask = vi.fn(async () => false)
    const host = createAppHost({ consent: { ask } as unknown as ReturnType<typeof createConsentGate>, navigate: () => {}, grantedTools: () => [`exponential_teams_update`, `exponential_people_add`] })
    for (const name of [`exponential_teams_update`, `exponential_people_add`]) {
      const outcome = await host.callFunction({ surfaceId: `s`, componentId: `c`, name, args: {} })
      expect(outcome.decision, name).toBe(`deny`)
    }
    expect(ask).toHaveBeenCalledTimes(2)
    expect(fetchMock).not.toHaveBeenCalled()
    vi.unstubAllGlobals()
  })
})

describe(`the via-app claim (VAPP-231, via-app.json claim)`, () => {
  const call = (host: ReturnType<typeof createAppHost>) => host.callFunction({ surfaceId: `s`, componentId: `c`, name: `exponential_issues_get`, args: { id: `APP-5` } })
  const claimHeaders = () => {
    const sent: (string | null)[] = []
    vi.stubGlobal(
      `fetch`,
      vi.fn(async (_url: string, init: RequestInit) => {
        const body = JSON.parse(String(init.body)) as { id: number }
        sent.push(new Headers(init.headers).get(`X-Exp-Via-App`))
        return new Response(JSON.stringify({ jsonrpc: `2.0`, id: body.id, result: { content: [] } }), { status: 200, headers: { "content-type": `application/json` } })
      })
    )
    return sent
  }
  const options = { consent: { ask: async () => true } as unknown as ReturnType<typeof createConsentGate>, navigate: () => {}, appId: `a1`, grantedTools: () => [`exponential_issues_get`] }

  it(`an MCP call of a published app's host carries the header`, async () => {
    mocks.apps = [{ id: `a1`, publishedVersion: 3 }]
    const sent = claimHeaders()
    await call(createAppHost(options))
    expect(sent).toEqual([`a1@3`])
    mocks.apps = []
    vi.unstubAllGlobals()
  })

  it(`a never-published app claims nothing`, async () => {
    mocks.apps = [{ id: `a1`, publishedVersion: 0 }]
    const sent = claimHeaders()
    await call(createAppHost(options))
    expect(sent).toEqual([null])
    mocks.apps = []
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
        host.applyTemplate({ surfaceId: `devices`, templateId: `devices` })
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
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness_mcp`, args: { tool: `exponential_issues`, arguments: { op: `get`, id: `EXP-12` } } } as never)
    })
    const block = await view.findByTestId(`exponential-ui-consent-arguments`)
    expect(block.textContent).toBe(`{"op":"get","id":"EXP-12"}`)
    expect(view.getByRole(`button`, { name: `Allow` }).getAttribute(`data-role`)).toBe(`default`)
    expect(view.queryByTestId(`exponential-ui-consent-show-all`)).toBeNull()
    // A long argument list previews and discloses.
    const long = { op: `create`, boardId: `b`.repeat(40), title: `t`.repeat(120), description: `d`.repeat(300) }
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness_mcp`, args: { tool: `exponential_issues`, arguments: long } } as never)
    })
    const preview = await view.findByTestId(`exponential-ui-consent-arguments`)
    expect(preview.textContent).toBe(`${JSON.stringify(long).slice(0, 160)}…`)
    fireEvent.click(view.getByTestId(`exponential-ui-consent-show-all`))
    // The CodeBlock paints one element per line: the text joins without newlines.
    expect(view.getByTestId(`exponential-ui-consent-arguments`).textContent).toBe(JSON.stringify(long, null, 2).replace(/\n/g, ``))
    // A destructive op: the destructive entry (Allow destructive, Deny still focused).
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness_mcp`, args: { tool: `exponential_issues`, arguments: { op: `delete`, id: `EXP-12` } } } as never)
    })
    await vi.waitFor(() => expect(view.getByTestId(`exponential-ui-consent-arguments`).textContent).toBe(`{"op":"delete","id":"EXP-12"}`))
    expect(view.getByRole(`button`, { name: `Allow` }).getAttribute(`data-role`)).toBe(`destructive`)
    expect(document.activeElement).toBe(view.getByRole(`button`, { name: `Deny` }))
    // No arguments: no block. A plain host function shows its own args.
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness_toast`, args: { message: `EXP-1` } } as never)
    })
    expect((await view.findByTestId(`exponential-ui-consent-arguments`)).textContent).toBe(`{"message":"EXP-1"}`)
    act(() => {
      gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness_mcp`, args: { tool: `exponential_devices` } } as never)
    })
    await view.findByText(`Allow this surface to run exponential_devices?`)
    expect(view.queryByTestId(`exponential-ui-consent-arguments`)).toBeNull()
  })

  it(`reads the contract prompt: Deny focused (Enter denies), Allow a plain default answer`, async () => {
    const gate = createConsentGate()
    const view = render(<ConsentCard gate={gate} />)
    let answer: Promise<boolean> | undefined
    act(() => {
      answer = gate.ask({ surfaceId: `s`, componentId: `b`, name: `harness_mcp`, args: { tool: `exponential_issues_list` } } as never)
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

// VAPP-157: the app host narrows the SDK's media policy to the fixture's
// rule (a data: image/audio/video or the instance's own attachment), on the
// raw src, through the very path every renderer media load takes
// (`hostPlugin` → `mediaRequestOf`); a plain host keeps the SDK default.
describe(`VAPP-157: the app host's media rule`, () => {
  it(`replays fixtures/apps.json mediaPolicy.cases through the host's media path`, () => {
    const origin = appsFixture.mediaPolicy.origin
    const host = createAppHost({ consent: createConsentGate(), navigate: () => {}, origin })
    const plugin = hostPlugin(host)
    for (const c of appsFixture.mediaPolicy.cases) {
      expect(host.mediaRequest(c.src) !== null, c.src).toBe(c.expected)
      expect(mediaRequestOf(plugin, c.src) !== null, c.src).toBe(c.expected)
    }
  })

  // #EXP-1674: `~/x` = the app's team; a link at another app needs its contract.
  it(`expands ~/ against the app's team and gates a link at another app`, () => {
    mocks.teams = [{ id: `t1`, slug: `acme team` }]
    mocks.apps = [
      { id: `a1`, teamId: `t1`, slug: `hours`, publishedVersion: 1, package: {} },
      { id: `a2`, teamId: `t1`, slug: `crm`, publishedVersion: 1, package: { screens: [{ template: `home`, title: `Home` }, { template: `deal`, title: `Deal`, nav: false, exposed: { description: `A deal`, params: { id: { required: true } } } }] } },
      { id: `a3`, teamId: `t1`, slug: `draft`, publishedVersion: 0, package: null },
    ]
    try {
      const pushed: string[] = []
      const origin = `https://app.exponential.at`
      const host = createAppHost({ consent: createConsentGate(), navigate: (href) => void pushed.push(href), origin, appId: `a1`, teamId: `t1` })
      mocks.toast.mockClear()
      expect(host.openUrl(`~/issues/EXP-1`)).toBe(true)
      expect(host.openUrl(`~/../x`)).toBe(false)
      expect(host.openUrl(`~/apps/crm/deal?id=a%20b&admin=1`)).toBe(true)
      expect(host.openUrl(`~/apps/hours/anything`)).toBe(true)
      expect(pushed).toEqual([`/t/acme%20team/issues/EXP-1`, `/t/acme%20team/apps/crm/deal?id=a%20b`, `/t/acme%20team/apps/hours/anything`])
      expect(mocks.toast).not.toHaveBeenCalled()
      for (const refused of [`~/apps/crm/deal`, `~/apps/crm/home`, `~/apps/gone/x`, `~/apps/draft`]) {
        host.openUrl(refused)
        expect(mocks.toast).toHaveBeenLastCalledWith(`Not found or no access`)
      }
      expect(pushed).toHaveLength(3)
      const noTeam = createAppHost({ consent: createConsentGate(), navigate: () => {}, origin })
      expect(noTeam.openUrl(`~/issues/EXP-1`)).toBe(false)
    } finally {
      mocks.teams = []
      mocks.apps = []
    }
  })

  it(`a plain host sharing the SDK keeps its default (an https image loads)`, () => {
    const plain = new ExponentialHost({ policy: { media: { baseUrl: `https://app.exponential.at` } } })
    expect(plain.mediaRequest(`https://images.example/cat.png`)).not.toBeNull()
    const app = createAppHost({ consent: createConsentGate(), navigate: () => {}, origin: `https://app.exponential.at` })
    expect(app.mediaRequest(`https://images.example/cat.png`)).toBeNull()
  })

  it(`#VAPP-230: an upstream binding reads through the broker; disconnected keeps the default and shows Connect <server>`, async () => {
    const pkg = {
      formatVersion: 6,
      id: `acme.console`,
      name: `Console`,
      version: `1.0.0`,
      catalogId: `https://ui.exponential.at/catalogs/core/v1`,
      sources: [`upstream:*`],
      upstreams: [`crm`, `stripe`],
      templates: {
        main: {
          data: { mrr: { total: 0 }, charges: { rows: [] } },
          bindings: [
            { path: `/mrr`, source: `upstream:crm/mrr?months=3` },
            { path: `/charges`, source: `upstream:stripe/list_charges` },
          ],
          components: [{ id: `root`, component: `Text`, text: { "@path": `/mrr/total` } }],
        },
      },
    } as unknown as AppPackage
    mocks.upstreamRead.mockImplementation(async ({ source }) =>
      source.startsWith(`upstream:crm/`) ? { ok: true, value: { total: 1200 } } : { ok: false, error: `upstreamDisconnected`, server: `stripe` }
    )
    const host = createAppHost({ consent: createConsentGate(), navigate: () => {}, packages: [pkg], appId: `a1` })
    await act(async () => {
      host.applyTemplate({ surfaceId: `main`, templateId: `main`, packageId: `acme.console` })
      await new Promise((r) => setTimeout(r, 0))
    })
    expect(mocks.upstreamRead).toHaveBeenCalledWith({ id: `a1`, source: `upstream:crm/mrr?months=3` })
    expect(host.surface(`main`)!.data.mrr).toEqual({ total: 1200 })
    expect(host.surface(`main`)!.data.charges).toEqual({ rows: [] })
    const view = render(<HostBanner host={host} />)
    expect(view.getByTestId(`exponential-ui-upstream-connect`).textContent).toContain(`Connect stripe`)
    act(() => host.close())
    expect(view.queryByTestId(`exponential-ui-upstream-connect`)).toBeNull()
  })

  it(`#VAPP-239: an offline device keeps the default and shows its banner without Connect`, async () => {
    const pkg = {
      formatVersion: 6,
      id: `acme.console`,
      name: `Console`,
      version: `1.0.0`,
      catalogId: `https://ui.exponential.at/catalogs/core/v1`,
      sources: [`upstream:*`],
      upstreams: [`crm`, `stripe`],
      templates: {
        main: {
          data: { mrr: { total: 0 }, charges: { rows: [] } },
          bindings: [
            { path: `/mrr`, source: `upstream:crm/mrr?months=3` },
            { path: `/charges`, source: `upstream:stripe/list_charges` },
          ],
          components: [{ id: `root`, component: `Text`, text: { "@path": `/mrr/total` } }],
        },
      },
    } as unknown as AppPackage
    mocks.upstreamRead.mockImplementation(async ({ source }) =>
      source.startsWith(`upstream:crm/`)
        ? { ok: false, error: `upstreamFailed`, server: `crm`, offline: `homelab` }
        : { ok: false, error: `upstreamDisconnected`, server: `stripe` }
    )
    const host = createAppHost({ consent: createConsentGate(), navigate: () => {}, packages: [pkg], appId: `a1` })
    await act(async () => {
      host.applyTemplate({ surfaceId: `main`, templateId: `main`, packageId: `acme.console` })
      await new Promise((r) => setTimeout(r, 0))
    })
    expect(mocks.upstreamRead).toHaveBeenCalledWith({ id: `a1`, source: `upstream:crm/mrr?months=3` })
    expect(host.surface(`main`)!.data.mrr).toEqual({ total: 0 })
    expect(host.surface(`main`)!.data.charges).toEqual({ rows: [] })
    const view = render(<HostBanner host={host} />)
    const banners = view.getAllByTestId(`exponential-ui-upstream-connect`)
    expect(banners.map((b) => b.textContent)).toEqual([`crm runs on homelab, offline`, `Connect stripe`])
    expect(banners[0]!.querySelector(`button`)).toBeNull()
    act(() => host.close())
  })
})
