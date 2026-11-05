// @vitest-environment node
// VAPP-241: the public surface's guards that answer before the database: a
// token of the wrong shape never reaches Postgres (a NUL in a query would be a
// 500), and every miss is the one `gone` answer. Run with a dummy DATABASE_URL
// (db/connection.ts needs one at import); the real flows live in
// share-links.real-db.test.ts.
import { describe, expect, it, vi } from "vitest"
import { APP_SHARE, APP_SHARE_COPY } from "@exp/domain-contract/app-share"
import { CORE_CATALOG_ID, validatePackage, type PackageShare } from "@exponential_app/eui"

vi.mock(`@/db/connection`, () => ({
  db: new Proxy({}, { get: () => { throw new Error(`the database was touched`) } }),
}))

const { guestPackage, guestPage, handleSharePage, handleShareSubmit } = await import(`@/lib/apps/share-links`)

const BAD_TOKENS = [``, `a\0b`, `a/b`, `a b`, `' OR 1=1 --`, `%00`, `x`.repeat(65)]

describe(`public link guards before the database`, () => {
  for (const token of BAD_TOKENS) {
    it(`a bad token is an unknown token: ${JSON.stringify(token.slice(0, 12))}`, async () => {
      expect(await guestPage(token)).toBeNull()
      const get = await handleSharePage(new Request(`http://x.test/api/share/t`), token)
      expect(get.status).toBe(404)
      expect(await get.json()).toEqual({ error: APP_SHARE_COPY.gone })
      expect(get.headers.get(`cache-control`)).toBe(`no-store`)
      const post = await handleShareSubmit(new Request(`http://x.test/api/share/t`, { method: `POST`, body: `{}` }), token)
      expect(post.status).toBe(404)
      expect(await post.json()).toEqual({ error: APP_SHARE_COPY.gone })
    })
  }

  it(`GET stops at the per-IP cap with the too-many copy`, async () => {
    const request = () => new Request(`http://x.test/api/share/t`, { headers: { "x-forwarded-for": `203.0.113.77` } })
    const now = new Date(`2026-11-04T10:00:00Z`)
    for (let i = 0; i < APP_SHARE.caps.viewPerIpPerHour; i++) expect((await handleSharePage(request(), ``, now)).status).toBe(404)
    const over = await handleSharePage(request(), ``, now)
    expect(over.status).toBe(429)
    expect(await over.json()).toEqual({ error: APP_SHARE_COPY.tooMany })
  })
})

describe(`install refuses a submit link no guest could fill (package-share.json)`, () => {
  const pkg = (refs: unknown, share: unknown) => ({ records: { t: { schema: { type: `object`, required: [`who`], properties: { who: { type: `string` } } }, refs } }, share })
  it.each([`member`, `issue`])(`a required %s ref`, (target) => {
    const issues = validatePackage(pkg({ who: target }, [{ name: `a`, mode: `submit`, kind: `t` }])).filter((i) => i.path.startsWith(`/share`))
    expect(issues).toEqual([{ path: `/share/0/kind`, message: `a submit link cannot fill the required ref field who` }])
  })
})

describe(`a view link's template (VAPP-246)`, () => {
  const kind = { title: `Lead`, schema: { type: `object`, required: [`email`], properties: { email: { type: `string` }, note: { type: `string` } } }, policy: { read: `all` } }
  const table = { id: `t`, component: `RecordTable`, kind: `lead`, editable: false }
  const published = (components: unknown[], bindings: unknown[] = []) => ({
    records: { lead: kind },
    queries: { recent: { from: `lead`, sort: `note`, where: { email: `private@acme.example` } } },
    catalogId: CORE_CATALOG_ID,
    templates: { board: { components, bindings } },
    actions: { hidden: { steps: [] } },
  })
  const share: PackageShare = { name: `board`, mode: `view`, kind: `lead`, fields: [`note`], query: `recent`, template: `board` }
  const page = [{ id: `root`, component: `Page`, title: `Board`, children: [`t`] }, table]
  const build = (pkg: Record<string, unknown>) => guestPackage(pkg, share, kind as never, [`note`], `Lead`)

  it(`passes the host, narrowed: no query, no other field, no action`, () => {
    const pkg = build(published(page, [{ path: `/list`, source: `records:lead?query=recent` }]))!
    expect(validatePackage(pkg)).toEqual([])
    expect(pkg.templates.main!.bindings).toEqual([{ path: `/list`, source: `records:lead` }])
    expect(Object.keys(pkg.records!.lead!.schema.properties)).toEqual([`note`])
    expect(pkg.functions).toEqual([])
    expect(pkg.sources).toEqual([`records:lead`])
    const text = JSON.stringify(pkg)
    for (const leak of [`private@acme.example`, `email`, `hidden`, `queries`, `actions`, `policy`]) expect(text).not.toContain(leak)
  })

  it(`a template that skipped install is gone: an exp: binding, a write, a press`, () => {
    expect(build(published(page, [{ path: `/m`, source: `exp:members` }]))).toBeNull()
    expect(build(published([page[0], { id: `t`, component: `RecordForm`, kind: `lead` }]))).toBeNull()
    expect(build(published([page[0], { ...table, on: { press: { functionCall: { call: `actions.hidden` } } } }]))).toBeNull()
    expect(guestPackage(published(page), { ...share, template: `nope` }, kind as never, [`note`], `Lead`)).toBeNull()
  })
})
