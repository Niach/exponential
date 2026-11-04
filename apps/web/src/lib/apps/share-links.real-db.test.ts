// @vitest-environment node
// VAPP-241 (eApps P1, fixtures/app-share.json) against the REAL migrated
// Postgres: the unauthenticated surface of `/s/<token>`. Enable, rotate, revoke
// and expiry; one `gone` answer for everything but a live link; the field
// allowlist; the honeypot; the caps, in parallel too; validation and quota; the
// sponsor as author with `via=share:<name>`; a view link's narrow snapshot; and
// that no other kind or member data reaches a guest.
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest"
import { TRPCError } from "@trpc/server"
import { and, eq, sql } from "drizzle-orm"
import { APP_SHARE_CAPS, APP_SHARE_COPY } from "@exp/domain-contract/app-share"
import { db } from "@/db/connection"
import { appRecordEvents, appRecords, apps, appShareLinks, appShareSubmissions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

const quota = vi.hoisted(() => ({ full: false }))
vi.mock(`@/lib/billing`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/billing")>()
  return {
    ...real,
    assertCanCreateAppRecord: async (...args: Parameters<typeof real.assertCanCreateAppRecord>) => {
      if (quota.full) throw new TRPCError({ code: `FORBIDDEN`, message: `Your plan allows up to 1 record` })
      return real.assertCanCreateAppRecord(...args)
    },
  }
})
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'app_share_links') AS ok
`)
if (!ready.ok) console.info(`[share-links.real-db] skipped: ${ready.reason}`)

const { enableShareLink, guestPage, handleSharePage, handleShareSubmit, revokeShareLink, rotateShareLink, shareRows } = await import(`@/lib/apps/share-links`)
const { deleteRecord, upsertRecord } = await import(`@/lib/apps/records-service`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const SECRET_NAME = `Sponsor Secretname`

async function user(): Promise<string> {
  const id = `share-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, ${SECRET_NAME}, ${`${id}@share.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const PACKAGE = {
  records: {
    signup: {
      title: `Signup`,
      schema: {
        type: `object`,
        required: [`email`],
        properties: {
          email: { type: `string`, maxLength: 40 },
          note: { type: `string` },
          secret: { type: `string` },
          cv: { type: `string`, format: `attachment` },
          owner: { type: `string` },
          bug: { type: `string` },
          lead: { type: `string` },
        },
      },
      refs: { owner: `member`, bug: `issue`, lead: { kind: `lead` } },
      policy: { read: `all` },
    },
    lead: { schema: { type: `object`, properties: { private: { type: `string` } } } },
  },
  queries: {
    recent: { from: `signup`, sort: `email` },
    expanded: { from: `signup`, expand: [`lead`, `owner`] },
    leads: { from: `lead` },
  },
  share: [
    { name: `join`, mode: `submit`, kind: `signup`, fields: [`email`, `note`] },
    { name: `everything`, mode: `submit`, kind: `signup` },
    { name: `soon`, mode: `submit`, kind: `signup`, fields: [`email`], expires: 1 },
    { name: `wall`, mode: `view`, kind: `signup`, fields: [`note`], query: `recent` },
    { name: `all`, mode: `view`, kind: `signup`, query: `expanded` },
  ],
}

const NOW = new Date()

async function world() {
  const owner = await user()
  const owner2 = await user()
  const member = await user()
  const [row] = await db.insert(teams).values({ name: `Share`, slug: `share-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: owner2, role: `owner` },
    { teamId, userId: member, role: `member` },
  ])
  const [app] = await db.insert(apps).values({ teamId, slug: `forms`, name: `Forms`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner }).returning({ id: apps.id })
  return { owner, owner2, member, teamId, app: { id: app!.id, teamId, package: PACKAGE } }
}
type World = Awaited<ReturnType<typeof world>>

async function tokenOf(w: World, name: string): Promise<string> {
  const [link] = await db.select({ token: appShareLinks.token }).from(appShareLinks).where(and(eq(appShareLinks.appId, w.app.id), eq(appShareLinks.name, name)))
  return link!.token
}
async function linkOn(w: World, name: string): Promise<string> {
  await enableShareLink(w.owner, w.app, name, NOW)
  return tokenOf(w, name)
}

function post(token: string, body: unknown, opts: { ip?: string; now?: Date; raw?: boolean } = {}) {
  const request = new Request(`http://x.test/api/share/${token}`, { method: `POST`, headers: { "content-type": `application/json`, "x-forwarded-for": opts.ip ?? `198.51.100.1` }, body: opts.raw ? (body as string) : JSON.stringify(body) })
  return handleShareSubmit(request, token, opts.now ?? NOW)
}
const submit = (token: string, data: Record<string, unknown>, opts: { ip?: string; now?: Date } = {}) => post(token, { data, website: `` }, opts)

const records = (w: World, kind = `signup`) => db.select().from(appRecords).where(and(eq(appRecords.appId, w.app.id), eq(appRecords.kind, kind)))
const audit = (w: World) => db.select().from(appShareSubmissions).where(eq(appShareSubmissions.appId, w.app.id))
const outcomes = async (w: World) => (await audit(w)).map((a) => a.outcome).sort()
const seedAudit = (w: World, linkId: string, n: number, outcome: `created` | `rate_limited`, createdAt = NOW) =>
  db.insert(appShareSubmissions).values(Array.from({ length: n }, (_, i) => ({ linkId, appId: w.app.id, teamId: w.teamId, ipHash: `seed-${i}`, outcome, createdAt })))
const linkIdOf = async (token: string) => (await db.select({ id: appShareLinks.id }).from(appShareLinks).where(eq(appShareLinks.token, token)))[0]!.id

beforeEach(() => {
  quota.full = false
})

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

const GONE = { error: APP_SHARE_COPY.gone }
async function expectGone(token: string, now = NOW) {
  expect(await guestPage(token, now)).toBeNull()
  const answer = await handleSharePage(new Request(`http://x.test/`, { headers: { "x-forwarded-for": `198.51.100.9` } }), token, now)
  expect(answer.status).toBe(404)
  expect(await answer.json()).toEqual(GONE)
  const sent = await submit(token, { email: `a@b.c` }, { now })
  expect(sent.status).toBe(404)
  expect(await sent.json()).toEqual(GONE)
}

describe.skipIf(!ready.ok)(`public links (real db)`, () => {
  it(`enable, rotate, revoke: the old URL stops at once, the expiry and the sponsor stay`, async () => {
    const w = await world()
    expect((await shareRows(w.app, NOW)).find((r) => r.name === `soon`)).toMatchObject({ state: `off`, url: null })
    await expectGone(`no-such-token`)

    const first = await linkOn(w, `soon`)
    expect(first).toMatch(/^[A-Za-z0-9_-]{43}$/)
    const row = (await shareRows(w.app, NOW)).find((r) => r.name === `soon`)!
    expect(row.state).toBe(`on`)
    expect(row.url).toContain(`/s/${first}`)
    expect(await guestPage(first, NOW)).not.toBeNull()
    const [before] = await db.select().from(appShareLinks).where(eq(appShareLinks.token, first))
    expect(before!.expiresAt!.getTime()).toBe(NOW.getTime() + 86_400_000)

    await rotateShareLink(w.app, `soon`, NOW)
    const second = await tokenOf(w, `soon`)
    expect(second).not.toBe(first)
    await expectGone(first)
    expect(await guestPage(second, NOW)).not.toBeNull()
    const [after] = await db.select().from(appShareLinks).where(eq(appShareLinks.token, second))
    expect(after!.expiresAt).toEqual(before!.expiresAt)
    expect(after!.enabledBy).toBe(w.owner)
    expect(after!.rotatedAt).not.toBeNull()

    await revokeShareLink(w.app, `soon`, NOW)
    await expectGone(second)
    // A revoked link cannot come back through rotate; enabling mints a new token.
    await expect(rotateShareLink(w.app, `soon`, NOW)).rejects.toThrow(/off/)
    expect(await tokenOf(w, `soon`)).toBe(second)
    await enableShareLink(w.owner2, w.app, `soon`, NOW)
    const third = await tokenOf(w, `soon`)
    expect(third).not.toBe(second)
    await expectGone(second)
    expect((await db.select().from(appShareLinks).where(eq(appShareLinks.token, third)))[0]!.enabledBy).toBe(w.owner2)
    await expect(enableShareLink(w.owner, w.app, `nope`, NOW)).rejects.toThrow(/no public link/)
  })

  it(`expires at the instant, and that is the same answer as an unknown token`, async () => {
    const w = await world()
    const token = await linkOn(w, `soon`)
    const almost = new Date(NOW.getTime() + 86_400_000 - 1)
    const at = new Date(NOW.getTime() + 86_400_000)
    expect(await guestPage(token, almost)).not.toBeNull()
    expect((await submit(token, { email: `a@b.c` }, { now: almost })).status).toBe(200)
    await expectGone(token, at)
    expect((await shareRows(w.app, at)).find((r) => r.name === `soon`)?.state).toBe(`expired`)
  })

  it(`a link closes with its sponsor, its app and its declaration; a draft's share is not live`, async () => {
    const w = await world()
    const demoted = await linkOn(w, `join`)
    await db.update(teamMembers).set({ role: `member` }).where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.owner)))
    await expectGone(demoted)
    await db.update(teamMembers).set({ role: `owner` }).where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.owner)))
    expect(await guestPage(demoted, NOW)).not.toBeNull()
    await db.delete(teamMembers).where(and(eq(teamMembers.teamId, w.teamId), eq(teamMembers.userId, w.owner)))
    await expectGone(demoted)

    const v = await world()
    const orphan = await linkOn(v, `join`)
    const { share: _share, ...withoutShare } = PACKAGE
    await db.update(apps).set({ package: withoutShare as never, draftPackage: PACKAGE as never }).where(eq(apps.id, v.app.id))
    await expectGone(orphan)
    expect((await shareRows({ id: v.app.id, package: withoutShare }, NOW)).find((r) => r.name === `join`)?.state).toBe(`orphaned`)
    await expect(enableShareLink(v.owner, { ...v.app, package: withoutShare }, `join`, NOW)).rejects.toThrow(/no public link/)
    await db.update(apps).set({ package: PACKAGE as never, draftPackage: null }).where(eq(apps.id, v.app.id))
    expect(await guestPage(orphan, NOW)).not.toBeNull()
    await db.update(apps).set({ deletedAt: new Date() }).where(eq(apps.id, v.app.id))
    await expectGone(orphan)
  })

  it(`a submit page carries the one kind's named fields, nothing else`, async () => {
    const w = await world()
    const page = (await guestPage(await linkOn(w, `join`), NOW))!
    expect(page.mode).toBe(`submit`)
    expect(page.value).toBeUndefined()
    const pkg = page.package as unknown as Record<string, Record<string, Record<string, unknown>>>
    expect(Object.keys(pkg.records!)).toEqual([`signup`])
    const kind = pkg.records!.signup! as { schema: { properties: object; required: string[] }; refs?: unknown; policy?: unknown }
    expect(Object.keys(kind.schema.properties)).toEqual([`email`, `note`])
    expect(kind.schema.required).toEqual([`email`])
    expect(kind.refs).toBeUndefined()
    expect(kind.policy).toBeUndefined()
    expect(pkg.queries).toBeUndefined()
    expect(pkg.share).toBeUndefined()
    const text = JSON.stringify(page)
    for (const leak of [`secret`, `cv`, `owner`, `private`, `"lead"`, w.owner, SECRET_NAME, w.teamId]) expect(text).not.toContain(leak)
  })

  it(`the default field list drops files and refs`, async () => {
    const w = await world()
    const page = (await guestPage(await linkOn(w, `everything`), NOW))!
    const kind = (page.package as unknown as { records: Record<string, { schema: { properties: object } }> }).records.signup!
    expect(Object.keys(kind.schema.properties)).toEqual([`email`, `note`, `secret`])
  })

  it(`a submission: only the allowed fields, the sponsor as author, via share:<name>, audited by a hash`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    const answer = await submit(token, { email: `a@b.c`, note: `hi`, secret: `x`, owner: w.member, cv: `/api/attachments/1`, id: `not-an-id`, authorId: w.member, createdBy: w.member, __proto__x: 1 }, { ip: `198.51.100.77` })
    expect(answer.status).toBe(200)
    expect(await answer.json()).toEqual({ ok: true })
    const [record] = await records(w)
    expect(record!.data).toEqual({ email: `a@b.c`, note: `hi` })
    expect(record!.authorId).toBe(w.owner)
    expect(record!.createdBy).toBe(w.owner)
    expect(record!.shareName).toBe(`join`)
    const [event] = await db.select().from(appRecordEvents).where(eq(appRecordEvents.recordId, record!.id))
    expect(event).toMatchObject({ via: `share:join`, actor: w.owner, op: `create` })
    const [row] = await audit(w)
    expect(row).toMatchObject({ outcome: `created`, recordId: record!.id })
    expect(row!.ipHash).toMatch(/^[0-9a-f]{64}$/)
    expect(JSON.stringify(row)).not.toContain(`198.51.100.77`)
  })

  it(`a posted ref is ignored even on the default field list: no probe of members or issues`, async () => {
    const w = await world()
    const token = await linkOn(w, `everything`)
    for (const data of [{ owner: `nobody` }, { owner: w.member }, { bug: `EXP-1` }, { lead: crypto.randomUUID() }]) {
      const answer = await submit(token, { email: `a@b.c`, ...data })
      expect(answer.status).toBe(200)
    }
    for (const r of await records(w)) expect(r.data).toEqual({ email: `a@b.c` })
  })

  it(`a view link cannot be written to, a submit link's page has no rows`, async () => {
    const w = await world()
    const wall = await linkOn(w, `wall`)
    expect((await submit(wall, { note: `x` })).status).toBe(404)
    expect(await records(w)).toEqual([])
  })

  it(`the honeypot answers like a success, stores nothing, and still does over a cap`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    const trapped = await post(token, { data: { email: `bot@b.c` }, website: `http://spam` })
    expect(trapped.status).toBe(200)
    expect(await trapped.json()).toEqual({ ok: true })
    expect(await records(w)).toEqual([])
    expect(await outcomes(w)).toEqual([`honeypot`])
    for (let i = 0; i < APP_SHARE_CAPS.perIpPerHour - 1; i++) expect((await post(token, { data: { email: `bot@b.c` }, website: `x` })).status).toBe(200)
    expect((await post(token, { data: { email: `bot@b.c` }, website: `x` })).status).toBe(200)
    expect(await records(w)).toEqual([])
    // Honeypots count against the IP: this one is over its cap now.
    expect((await submit(token, { email: `a@b.c` })).status).toBe(429)
  })

  it(`the per-IP cap: the eleventh is refused, another IP is not, and the hour passes`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    for (let i = 0; i < APP_SHARE_CAPS.perIpPerHour; i++) expect((await submit(token, { email: `a@b.c` })).status).toBe(200)
    const over = await submit(token, { email: `a@b.c` })
    expect(over.status).toBe(429)
    expect(await over.json()).toEqual({ error: APP_SHARE_COPY.tooMany })
    expect((await submit(token, { email: `a@b.c` }, { ip: `198.51.100.2` })).status).toBe(200)
    expect(await records(w)).toHaveLength(APP_SHARE_CAPS.perIpPerHour + 1)
    expect((await submit(token, { email: `a@b.c` }, { now: new Date(NOW.getTime() + 3_600_001) })).status).toBe(200)
  })

  it(`a flood of refusals neither fills the audit nor closes the link for everyone`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    for (let i = 0; i < APP_SHARE_CAPS.perIpPerHour; i++) await submit(token, { email: `a@b.c` })
    for (let i = 0; i < 25; i++) expect((await submit(token, { email: `a@b.c` })).status).toBe(429)
    expect((await outcomes(w)).filter((o) => o === `rate_limited`)).toHaveLength(1)
    await seedAudit(w, await linkIdOf(token), APP_SHARE_CAPS.perLinkPerHour, `rate_limited`)
    expect((await submit(token, { email: `a@b.c` }, { ip: `198.51.100.3` })).status).toBe(200)
  })

  it(`the per-link cap closes every IP`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    await seedAudit(w, await linkIdOf(token), APP_SHARE_CAPS.perLinkPerHour - 1, `created`)
    expect((await submit(token, { email: `a@b.c` }, { ip: `198.51.100.4` })).status).toBe(200)
    expect((await submit(token, { email: `a@b.c` }, { ip: `198.51.100.5` })).status).toBe(429)
    expect(await records(w)).toHaveLength(1)
  })

  it(`parallel requests cannot pass a cap`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    const answers = await Promise.all(Array.from({ length: 30 }, () => submit(token, { email: `a@b.c` })))
    expect(answers.filter((a) => a.status === 200)).toHaveLength(APP_SHARE_CAPS.perIpPerHour)
    expect(answers.filter((a) => a.status === 429)).toHaveLength(30 - APP_SHARE_CAPS.perIpPerHour)
    expect(await records(w)).toHaveLength(APP_SHARE_CAPS.perIpPerHour)
  })

  it(`schema validation: 400 with the field issue, audited invalid, nothing stored`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    const missing = await submit(token, { note: `x` })
    expect(missing.status).toBe(400)
    expect((await missing.json() as { error: string }).error).toMatch(/email/)
    const wrongType = await submit(token, { email: 12 })
    expect(wrongType.status).toBe(400)
    const tooLong = await submit(token, { email: `x`.repeat(41) })
    expect(tooLong.status).toBe(400)
    expect(await records(w)).toEqual([])
    expect(await outcomes(w)).toEqual([`invalid`, `invalid`, `invalid`])
  })

  it(`a body that is not a JSON object is a 400, never a 500`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    for (const raw of [`null`, `[]`, `1`, `"x"`, `{bad`, ``]) expect((await post(token, raw, { raw: true })).status).toBe(400)
    // No `data`, or the wrong shape: the schema's issue.
    for (const raw of [`{}`, `{"data":null}`, `{"data":[1]}`, `{"data":"x"}`]) expect((await post(token, raw, { raw: true })).status).toBe(400)
    expect(await records(w)).toEqual([])
  })

  it(`a value Postgres refuses is a 400, audited invalid`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    const answer = await submit(token, { email: `a\u0000b` })
    expect(answer.status).toBe(400)
    expect(await records(w)).toEqual([])
    expect(await outcomes(w)).toEqual([`invalid`])
  })

  it(`an oversized body is 413, with or without a content-length`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    const big = JSON.stringify({ data: { email: `a@b.c`, note: `x`.repeat(APP_SHARE_CAPS.maxBodyBytes) } })
    expect((await post(token, big, { raw: true })).status).toBe(413)
    const stream = new ReadableStream({
      start(c) {
        c.enqueue(new TextEncoder().encode(big))
        c.close()
      },
    })
    const chunked = new Request(`http://x.test/`, { method: `POST`, body: stream, duplex: `half` } as RequestInit)
    expect(chunked.headers.get(`content-length`)).toBeNull()
    expect((await handleShareSubmit(chunked, token, NOW)).status).toBe(413)
    expect(await audit(w)).toEqual([])
  })

  it(`the quota closes the form, audited quota`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    quota.full = true
    const answer = await submit(token, { email: `a@b.c` })
    expect(answer.status).toBe(403)
    expect(await answer.json()).toEqual({ error: APP_SHARE_COPY.closed })
    expect(await records(w)).toEqual([])
    expect(await outcomes(w)).toEqual([`quota`])
  })

  it(`the audit keeps auditDays days`, async () => {
    const w = await world()
    const token = await linkOn(w, `join`)
    await seedAudit(w, await linkIdOf(token), 3, `created`, new Date(NOW.getTime() - (APP_SHARE_CAPS.auditDays + 1) * 86_400_000))
    await seedAudit(w, await linkIdOf(token), 1, `created`, new Date(NOW.getTime() - 2 * 3_600_000))
    await submit(token, { email: `a@b.c` })
    expect(await audit(w)).toHaveLength(2)
  })

  describe(`a view link`, () => {
    async function seeded() {
      const w = await world()
      const lead = await upsertRecord(w.owner, { id: w.app.id }, { kind: `lead`, data: { private: `TOP-SECRET-LEAD` } })
      await upsertRecord(w.owner, { id: w.app.id }, { kind: `signup`, data: { email: `b@x.y`, note: `second`, secret: `S3CRET`, owner: w.member, lead: lead.id } })
      await upsertRecord(w.member, { id: w.app.id }, { kind: `signup`, data: { email: `a@x.y`, note: `first`, secret: `S3CRET` } })
      const { id: gone } = await upsertRecord(w.owner, { id: w.app.id }, { kind: `signup`, data: { email: `c@x.y`, note: `deleted` } })
      await deleteRecord(w.owner, { id: w.app.id }, gone)
      return { w, leadId: lead.id }
    }

    it(`shows only id, updatedAt and the named fields, in the frozen query's order`, async () => {
      const { w } = await seeded()
      const page = (await guestPage(await linkOn(w, `wall`), NOW))!
      expect(page.mode).toBe(`view`)
      const rows = page.value!.rows
      expect(rows.map((r) => r.note)).toEqual([`first`, `second`])
      for (const row of rows) expect(Object.keys(row).sort()).toEqual([`id`, `note`, `updatedAt`])
      expect(page.value!.count).toBe(2)
      const text = JSON.stringify(page)
      for (const leak of [`S3CRET`, `b@x.y`, `email`, `TOP-SECRET-LEAD`, `deleted`, w.owner, w.member, SECRET_NAME, `authorId`, `createdBy`]) expect(text).not.toContain(leak)
      const node = (page.package as unknown as { templates: { main: { components: { component: string; editable?: boolean; add?: boolean }[] } } }).templates.main.components.find((c) => c.component === `RecordTable`)
      expect(node).toMatchObject({ editable: false, add: false })
    })

    it(`the default fields drop refs and files; a query's expand reads no other kind and no member`, async () => {
      const { w, leadId } = await seeded()
      const page = (await guestPage(await linkOn(w, `all`), NOW))!
      expect(page.value!.rows).toHaveLength(2)
      for (const row of page.value!.rows) expect(Object.keys(row).sort()).toEqual([`email`, `id`, `note`, `secret`, `updatedAt`])
      const text = JSON.stringify(page)
      for (const leak of [`TOP-SECRET-LEAD`, leadId, w.owner, w.member, SECRET_NAME]) expect(text).not.toContain(leak)
    })

    it(`caps at viewRows`, async () => {
      const w = await world()
      await db.transaction(async (tx) => {
        await tx.execute(sql`SELECT set_config('eapps.write', 'on', true)`)
        await tx.insert(appRecords).values(Array.from({ length: APP_SHARE_CAPS.viewRows + 5 }, (_, i) => ({ appId: w.app.id, teamId: w.teamId, kind: `signup`, data: { email: `e${i}`, note: `n` }, createdBy: w.owner, authorId: w.owner, updatedBy: w.owner, schemaVersion: 1 })))
      })
      const page = (await guestPage(await linkOn(w, `wall`), NOW))!
      expect(page.value!.rows).toHaveLength(APP_SHARE_CAPS.viewRows)
      expect(page.value!.count).toBe(APP_SHARE_CAPS.viewRows)
    })

    it(`GET answers no-store`, async () => {
      const w = await world()
      const token = await linkOn(w, `wall`)
      const answer = await handleSharePage(new Request(`http://x.test/`, { headers: { "x-forwarded-for": `198.51.100.8` } }), token, NOW)
      expect(answer.status).toBe(200)
      expect(answer.headers.get(`cache-control`)).toBe(`no-store`)
    })
  })
})
