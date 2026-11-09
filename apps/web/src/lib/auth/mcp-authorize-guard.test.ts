// @vitest-environment node
import { describe, expect, it, vi } from "vitest"

const h = vi.hoisted(() => ({ rows: [] as Array<{ redirectUris: string[]; disabled: boolean | null }> }))

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => ({ from: () => ({ where: () => ({ limit: async () => h.rows }) }) }),
  },
}))
const { guardMcpAuthorize } = await import(`./mcp-authorize-guard`)
const { isAppsResource } = await import(`./mcp-resource`)

const authorize = (query: string) => new Request(`https://exp.test/api/auth/oauth2/authorize?${query}`)

describe(`mcp authorize: the apps resource (VAPP-182)`, () => {
  it(`recognizes the apps endpoint only`, () => {
    expect(isAppsResource(`https://exp.test/api/mcp/apps`)).toBe(true)
    expect(isAppsResource(`https://exp.test/api/mcp/apps/`)).toBe(true)
    expect(isAppsResource(`https://exp.test/api/mcp`)).toBe(false)
    expect(isAppsResource(`not a url`)).toBe(false)
    expect(isAppsResource(null)).toBe(false)
  })
})

describe(`guardMcpAuthorize`, () => {
  it(`forces prompt=consent on a known client`, async () => {
    h.rows = [{ redirectUris: [`http://localhost:1/cb`], disabled: false }]
    const guarded = await guardMcpAuthorize(authorize(`client_id=c&redirect_uri=${encodeURIComponent(`http://localhost:1/cb`)}`))
    if (!(`request` in guarded)) throw new Error(`expected a request`)
    expect(new URL(guarded.request.url).searchParams.get(`prompt`)).toBe(`consent`)
  })

  it(`answers an unknown client with a page for the human`, async () => {
    h.rows = []
    const guarded = await guardMcpAuthorize(authorize(`client_id=gone`))
    if (!(`response` in guarded)) throw new Error(`expected a response`)
    expect(guarded.response.status).toBe(400)
    expect(await guarded.response.text()).toContain(`isn't registered here`)
  })

  it(`answers a redirect_uri the client never registered`, async () => {
    h.rows = [{ redirectUris: [`http://localhost:1/cb`], disabled: false }]
    const guarded = await guardMcpAuthorize(authorize(`client_id=c&redirect_uri=${encodeURIComponent(`http://evil.test/cb`)}`))
    if (!(`response` in guarded)) throw new Error(`expected a response`)
    expect(await guarded.response.text()).toContain(`Sign-in link out of date`)
  })
})
