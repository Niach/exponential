// #VAPP-230: the broker's upstream read bindings (@exp/emcp upstream.json
// `binding`): declared upstreams only, the viewer's own connection, the
// read-only check, the 60 s cache per member.
import { beforeEach, describe, expect, it, vi } from "vitest"

type Row = { id: string; transport: string; device?: { id: string; ownerId: string; label: string; steerDeviceId: string } | null }
const servers = vi.hoisted(() => ({ rows: [{ id: `s1`, transport: `http` }] as Row[] }))
const device = vi.hoisted(() => ({ calls: [] as unknown[], out: { result: { structuredContent: { temp: 21 } } } as Record<string, unknown> }))
const launch = vi.hoisted(() => ({ url: `https://crm.example/mcp` as string | null }))
const calls = vi.hoisted(() => ({ n: 0, annotations: { readOnlyHint: true } as Record<string, unknown> | null, result: { structuredContent: { mrr: 1200 } } as Record<string, unknown> }))

vi.mock(`@/db/connection`, () => ({ db: {} }))
// #VAPP-239: the row with its device; the device call itself is stubbed.
vi.mock(`@/lib/emcp/upstream/device-call`, async () => {
  const { upstreamRoute } = await import(`@exp/emcp/upstream`)
  return {
    loadRoutedServer: async (teamId: string, name: string) =>
      servers.rows[0] ? { ...servers.rows[0], name, teamId, device: servers.rows[0].device ?? null } : null,
    routeServer: (server: { name: string; transport: string; device: { id: string; ownerId: string; label: string } | null }, caller: string) =>
      upstreamRoute(server, caller),
    callOnDevice: async (input: unknown) => {
      device.calls.push(input)
      return device.out
    },
  }
})
vi.mock(`@/lib/emcp/upstream/credentials`, () => ({
  resolveForLaunch: async () => ({ servers: launch.url ? [{ url: launch.url, headers: [{ name: `authorization`, value: `Bearer t` }] }] : [] }),
}))
vi.mock(`@/lib/emcp/upstream/mcp-client`, () => ({
  callUpstreamTool: async (_url: string, _h: unknown, _tool: string, _args: unknown, check: (a: unknown) => string | null) => {
    calls.n++
    const vetoed = check(calls.annotations)
    if (vetoed) return { ok: false, reason: `vetoed`, error: vetoed }
    return { ok: true, annotations: calls.annotations, result: calls.result }
  },
}))

const { clearUpstreamReadCache, readUpstreamBinding } = await import(`@/lib/emcp/upstream-reads`)

const pkg = { upstreams: [`crm`], sources: [`upstream:crm/*`] }
const app = { id: `a1`, slug: `console`, teamId: `t1`, package: pkg, draftPackage: null } as never

beforeEach(() => {
  clearUpstreamReadCache()
  servers.rows = [{ id: `s1`, transport: `http` }]
  launch.url = `https://crm.example/mcp`
  calls.n = 0
  calls.annotations = { readOnlyHint: true }
  calls.result = { structuredContent: { mrr: 1200 } }
  device.calls = []
  device.out = { result: { structuredContent: { temp: 21 } } }
})

describe(`readUpstreamBinding`, () => {
  it(`answers the value with provenance`, async () => {
    const out = await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)
    expect(out).toMatchObject({ ok: true, value: { mrr: 1200 }, meta: { "at.exponential/app": `console`, "at.exponential/tool": `crm/mrr`, "at.exponential/via": `upstream`, "at.exponential/upstream": `crm` } })
  })

  it(`caches 60 s per member, server, tool and args`, async () => {
    await readUpstreamBinding(`u1`, app, `upstream:crm/mrr?a=1`, 0)
    await readUpstreamBinding(`u1`, app, `upstream:crm/mrr?a=1`, 59_000)
    expect(calls.n).toBe(1)
    await readUpstreamBinding(`u2`, app, `upstream:crm/mrr?a=1`, 59_000)
    await readUpstreamBinding(`u1`, app, `upstream:crm/mrr?a=2`, 59_000)
    expect(calls.n).toBe(3)
    await readUpstreamBinding(`u1`, app, `upstream:crm/mrr?a=1`, 60_000)
    expect(calls.n).toBe(4)
  })

  it(`refuses an upstream the app does not declare`, async () => {
    expect(await readUpstreamBinding(`u1`, app, `upstream:stripe/list`)).toMatchObject({ ok: false, error: `notDeclared`, server: `stripe` })
    const strict = { ...(app as object), package: { upstreams: [`crm`], sources: [`records:*`] } } as never
    expect(await readUpstreamBinding(`u1`, strict, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `notDeclared` })
  })

  it(`reads through the draft's declaration too (the editor preview)`, async () => {
    const draft = { ...(app as object), package: { upstreams: [], sources: [] }, draftPackage: pkg } as never
    expect(await readUpstreamBinding(`u1`, draft, `upstream:crm/mrr`)).toMatchObject({ ok: true })
  })

  it(`a member without a connection is disconnected, never cached`, async () => {
    launch.url = null
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `upstreamDisconnected`, server: `crm` })
  })

  it(`a removed server is missing; a stdio one fails`, async () => {
    servers.rows = []
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `upstreamMissing` })
    servers.rows = [{ id: `s1`, transport: `stdio` }]
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `upstreamFailed` })
  })

  it(`only read-only tools run`, async () => {
    calls.annotations = { readOnlyHint: false }
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `notReadOnly` })
    calls.annotations = null
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `notReadOnly` })
    const vouched = { ...(app as object), package: { upstreams: [`crm`], sources: [`upstream:crm/mrr`] } } as never
    expect(await readUpstreamBinding(`u1`, vouched, `upstream:crm/mrr`)).toMatchObject({ ok: true })
  })

  it(`an error answer is upstreamFailed and not cached`, async () => {
    calls.result = { isError: true, content: [{ type: `text`, text: `rate limited` }] }
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: false, error: `upstreamFailed` })
    calls.result = { structuredContent: { mrr: 1 } }
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/mrr`)).toMatchObject({ ok: true, value: { mrr: 1 } })
  })

  it(`#VAPP-239: a device-routed server reads on its owner's device, cached like http`, async () => {
    servers.rows = [{ id: `s1`, transport: `stdio`, device: { id: `d1`, ownerId: `u1`, label: `homelab`, steerDeviceId: `dev-1` } }]
    const out = await readUpstreamBinding(`u1`, app, `upstream:crm/temp?room=kitchen`, 0)
    expect(out).toMatchObject({ ok: true, value: { temp: 21 }, meta: { "at.exponential/device": `homelab`, "at.exponential/via": `upstream` } })
    expect(device.calls).toEqual([expect.objectContaining({ userId: `u1`, tool: `temp`, args: { room: `kitchen` }, veto: { kind: `read`, sources: [`upstream:crm/*`] } })])
    await readUpstreamBinding(`u1`, app, `upstream:crm/temp?room=kitchen`, 30_000)
    expect(device.calls).toHaveLength(1)
    expect(calls.n).toBe(0)
  })

  it(`#VAPP-239: another member's device is refused; an offline one carries its label`, async () => {
    servers.rows = [{ id: `s1`, transport: `http`, device: { id: `d1`, ownerId: `u1`, label: `homelab`, steerDeviceId: `dev-1` } }]
    expect(await readUpstreamBinding(`u2`, app, `upstream:crm/temp`)).toMatchObject({ ok: false, error: `deviceNotOwned`, message: `crm runs on homelab: only its owner's calls reach it` })
    expect(device.calls).toHaveLength(0)
    device.out = { error: `upstreamFailed`, message: `crm runs on homelab, offline`, offline: `homelab` }
    expect(await readUpstreamBinding(`u1`, app, `upstream:crm/temp`)).toEqual({ ok: false, error: `upstreamFailed`, server: `crm`, message: `crm runs on homelab, offline`, offline: `homelab` })
    // An error is never cached.
    await readUpstreamBinding(`u1`, app, `upstream:crm/temp`)
    expect(device.calls).toHaveLength(2)
  })
})
