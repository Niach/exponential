// #VAPP-230: the broker's upstream read bindings (@exp/emcp upstream.json
// `binding`): declared upstreams only, the viewer's own connection, the
// read-only check, the 60 s cache per member.
import { beforeEach, describe, expect, it, vi } from "vitest"

const servers = vi.hoisted(() => ({ rows: [{ id: `s1`, transport: `http` }] as { id: string; transport: string }[] }))
const launch = vi.hoisted(() => ({ url: `https://crm.example/mcp` as string | null }))
const calls = vi.hoisted(() => ({ n: 0, annotations: { readOnlyHint: true } as Record<string, unknown> | null, result: { structuredContent: { mrr: 1200 } } as Record<string, unknown> }))

vi.mock(`@/db/connection`, () => ({
  db: { select: () => ({ from: () => ({ where: () => ({ limit: async () => servers.rows }) }) }) },
}))
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
})
