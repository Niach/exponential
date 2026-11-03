// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appAccess, appRecords, appVersions, apps, boards, devices, issues, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { grantVersion } from "@exp/domain-contract/apps"

// Release train 2026-10-23 (VAPP-110 review): the apps router against the
// REAL migrated Postgres. A deleted account leaves its apps behind (creator
// SET NULL, owner-only to manage); inputs are strip-mode; a trashed app frees
// its slug (partial unique index); a CONFLICT names a derived slug; a version
// always names its author and keeps only the caller's own editor session.

// The package format is VAPP-107's concern: any object passes here.
vi.mock(`@/lib/apps/package`, async (importOriginal) => ({ ...((await importOriginal()) as Record<string, unknown>), validateAppPackage: (pkg: unknown) => pkg }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'uniq_apps_team_slug') AS ok
`)
if (!ready.ok) console.info(`[apps.real-db] skipped: ${ready.reason}`)

const { appsRouter } = await import(`@/lib/trpc/apps`)

/** VAPP-156: the head version a client saw (a grant names it). */
// eApps V1 (#VAPP-210): a new version that RUNS = the draft + its publish
// by the same member (the grants follow the published package).
const ship = async (who: string, input: { id: string; package: Record<string, unknown> }) => {
  await caller(who).upsertPackage(input)
  await caller(who).publish({ id: input.id })
}
const shipRestore = async (who: string, id: string, number: number) => {
  await caller(who).restore({ id, number })
  await caller(who).publish({ id })
}
const seen = async (appId: string) => grantVersion((await db.select({ headVersion: apps.headVersion, publishedVersion: apps.publishedVersion }).from(apps).where(eq(apps.id, appId)))[0]!)
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `apps-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@apps.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const caller = (userId: string) =>
  appsRouter.createCaller({
    session: { user: { id: userId } },
    db,
    request: new Request(`http://x.test`),
  } as never)

async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Apps`, slug: `apps-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  return { owner, mate, teamId }
}

async function editorSession(teamId: string, userId: string, appId: string): Promise<string> {
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId, deviceId, label: `box`, kind: `server` })
  const id = crypto.randomUUID()
  await db.insert(sessions).values({ id, rootId: id, teamId, userId, deviceId, appId })
  return id
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe.skipIf(!ready.ok)(`apps router (real db)`, () => {
  it(`a deleted creator leaves the app, its versions and editor sessions; owner-only after`, async () => {
    const w = await world()
    const third = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: third, role: `member` })
    const { app } = await caller(w.mate).create({ teamId: w.teamId, name: `Board`, package: { id: `p` } })
    const editor = await editorSession(w.teamId, w.owner, app.id)

    await db.execute(sql`DELETE FROM users WHERE id = ${w.mate}`)

    const [kept] = await db.select().from(apps).where(eq(apps.id, app.id))
    expect(kept?.createdBy).toBeNull()
    expect(await db.select().from(appVersions).where(eq(appVersions.appId, app.id))).toHaveLength(1)
    expect(await db.select().from(sessions).where(eq(sessions.id, editor))).toHaveLength(1)

    await expect(caller(third).update({ id: app.id, name: `Mine` })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(caller(third).delete({ id: app.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    const renamed = await caller(w.owner).update({ id: app.id, name: `Renamed` })
    expect(renamed.app.name).toBe(`Renamed`)
  })

  it(`VAPP-176: New app = an "Untitled app" draft; a draft's rename re-derives the slug, a published app's never`, async () => {
    const w = await world()
    const first = await caller(w.mate).create({ teamId: w.teamId })
    expect(first.app).toMatchObject({ name: `Untitled app`, slug: `untitled-app`, headVersion: 0 })
    const second = await caller(w.mate).create({ teamId: w.teamId })
    expect(second.app.slug).toBe(`untitled-app-2`)

    const renamed = await caller(w.mate).update({ id: second.app.id, name: `Untitled app` })
    expect(renamed.app.slug).toBe(`untitled-app-2`)
    const named = await caller(w.mate).update({ id: second.app.id, name: `Working hours`, icon: `clock` })
    expect(named.app).toMatchObject({ slug: `working-hours`, icon: `clock` })

    await caller(w.mate).upsertPackage({ id: second.app.id, package: { id: `p` } })
    const published = await caller(w.mate).update({ id: second.app.id, name: `Hours` })
    expect(published.app).toMatchObject({ name: `Hours`, slug: `working-hours` })
  })

  it(`VAPP-176: discardDraft deletes only the creator's EMPTY draft`, async () => {
    const w = await world()
    const empty = await caller(w.mate).create({ teamId: w.teamId })
    expect((await caller(w.owner).discardDraft({ id: empty.app.id })).discarded).toBe(false)
    expect((await caller(w.mate).discardDraft({ id: empty.app.id })).discarded).toBe(true)
    expect(await db.select().from(apps).where(eq(apps.id, empty.app.id))).toHaveLength(0)

    const chatted = await caller(w.mate).create({ teamId: w.teamId })
    await editorSession(w.teamId, w.mate, chatted.app.id)
    expect((await caller(w.mate).discardDraft({ id: chatted.app.id })).discarded).toBe(false)

    const published = await caller(w.mate).create({ teamId: w.teamId, name: `Pub`, package: { id: `p` } })
    expect((await caller(w.mate).discardDraft({ id: published.app.id })).discarded).toBe(false)
    expect(await db.select().from(apps).where(eq(apps.id, published.app.id))).toHaveLength(1)
  })

  it(`strips unknown keys on create and update`, async () => {
    const w = await world()
    const { app } = await caller(w.owner).create({ teamId: w.teamId, name: `Strip`, future: 1 } as never)
    expect(app.slug).toBe(`strip`)
    const { app: updated } = await caller(w.owner).update({ id: app.id, name: `Strip 2`, future: 1 } as never)
    expect(updated.name).toBe(`Strip 2`)
  })

  it(`a trashed app frees its slug`, async () => {
    const w = await world()
    const { app } = await caller(w.owner).create({ teamId: w.teamId, name: `Dash`, slug: `dash` })
    await expect(caller(w.owner).create({ teamId: w.teamId, name: `Dash`, slug: `dash` })).rejects.toMatchObject({
      code: `CONFLICT`,
      message: expect.stringContaining(`"dash"`),
    })
    await caller(w.owner).delete({ id: app.id })
    const again = await caller(w.owner).create({ teamId: w.teamId, name: `Dash`, slug: `dash` })
    expect(again.app.id).not.toBe(app.id)
    // A derived slug ignores the trashed row too.
    await caller(w.owner).delete({ id: again.app.id })
    const derived = await caller(w.owner).create({ teamId: w.teamId, name: `Dash` })
    expect(derived.app.slug).toBe(`dash`)
  })

  it(`a version names its author and keeps only the caller's own editor session`, async () => {
    const w = await world()
    const { app } = await caller(w.owner).create({ teamId: w.teamId, name: `Ver` })
    const { app: other } = await caller(w.owner).create({ teamId: w.teamId, name: `Other` })
    const own = await editorSession(w.teamId, w.mate, app.id)
    const ownersEditor = await editorSession(w.teamId, w.owner, app.id)
    const elsewhere = await editorSession(w.teamId, w.mate, other.id)

    const write = async (sessionId?: string) => {
      const { version } = await caller(w.mate).upsertPackage({ id: app.id, package: { v: 1 }, sessionId })
      const [row] = await db
        .select()
        .from(appVersions)
        .where(sql`${appVersions.appId} = ${app.id} AND ${appVersions.number} = ${version}`)
      return row!
    }
    expect(await write(own)).toMatchObject({ sessionId: own, createdBy: w.mate })
    expect(await write(ownersEditor)).toMatchObject({ sessionId: null, createdBy: w.mate })
    expect(await write(elsewhere)).toMatchObject({ sessionId: null, createdBy: w.mate })
    expect(await write(crypto.randomUUID())).toMatchObject({ sessionId: null, createdBy: w.mate })
    expect(await write()).toMatchObject({ sessionId: null, createdBy: w.mate })
  })
  // VAPP-122: records (fixtures/app-records.json): validated against the
  // head package's kind schema, patched by id, trashed with the app.
  const RECORDS_PACKAGE = {
    formatVersion: 3,
    id: `t.hours`,
    name: `Hours`,
    version: `1.0.0`,
    catalogId: `https://ui.exponential.at/catalogs/core/v1`,
    templates: { m: { components: [{ id: `root`, component: `Stack`, children: [] }] } },
    sources: [`records:*`],
    records: {
      entry: {
        schema: {
          type: `object`,
          properties: { date: { type: `string`, format: `date` }, hours: { type: `number`, minimum: 0, maximum: 24 }, note: { type: `string`, maxLength: 20 } },
          required: [`date`, `hours`],
        },
      },
    },
  }

  // eApps V1 (#VAPP-210, apps.json DRAFT VS PUBLISHED): a version write is
  // a draft; members, the broker and the record policies run the published
  // package until a Publish moves the pointer (an older number = a rollback).
  it(`V1: upserts are drafts, publish moves the pointer, a rollback publishes an older version`, async () => {
    const w = await world()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    expect(created.app).toMatchObject({ headVersion: 1, publishedVersion: 1 })
    const row = async () => (await db.select().from(apps).where(eq(apps.id, created.app.id)))[0]!
    const draft = { ...RECORDS_PACKAGE, version: `1.1.0`, records: { ...RECORDS_PACKAGE.records, task: { schema: { type: `object`, properties: { title: { type: `string` } }, required: [`title`] } } } }
    await caller(w.mate).upsertPackage({ id: created.app.id, package: draft })
    expect(await row()).toMatchObject({ headVersion: 2, publishedVersion: 1, package: RECORDS_PACKAGE, draftPackage: draft })
    // get() = the head; get({version: published}) = the published package.
    expect((await caller(w.mate).get({ id: created.app.id })).package).toEqual(draft)
    expect((await caller(w.mate).get({ id: created.app.id, version: 1 })).package).toEqual(RECORDS_PACKAGE)
    // A draft's new kind takes records before it is published (an editor seeds it).
    await caller(w.mate).records.upsert({ id: created.app.id, kind: `task`, data: { title: `seed` } })
    // A read-only member cannot publish.
    await db.insert(appAccess).values({ appId: created.app.id, teamId: w.teamId, userId: w.mate, level: `read` })
    await expect(caller(w.mate).publish({ id: created.app.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await db.delete(appAccess).where(eq(appAccess.appId, created.app.id))
    expect(await caller(w.mate).publish({ id: created.app.id })).toMatchObject({ published: 2, head: 2 })
    expect(await row()).toMatchObject({ headVersion: 2, publishedVersion: 2, package: draft, draftPackage: null })
    // Rollback: publish v1 again; the head (v2) stays the draft.
    expect(await caller(w.owner).publish({ id: created.app.id, number: 1 })).toMatchObject({ published: 1, head: 2 })
    expect(await row()).toMatchObject({ headVersion: 2, publishedVersion: 1, package: RECORDS_PACKAGE, draftPackage: draft })
    await expect(caller(w.owner).publish({ id: created.app.id, number: 9 })).rejects.toMatchObject({ code: `NOT_FOUND` })
    // Nothing to publish on an empty draft app.
    const empty = await caller(w.owner).create({ teamId: w.teamId })
    await expect(caller(w.owner).publish({ id: empty.app.id })).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`records: a write follows the kind's schema, patches by id, and leaves with the app`, async () => {
    const w = await world()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const appId = created.app.id
    const api = caller(w.mate).records
    await expect(api.upsert({ id: appId, kind: `todo`, data: {} })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(api.upsert({ id: appId, kind: `entry`, data: { date: `5.10.`, hours: 30 } })).rejects.toThrow(/\/date: a date \(YYYY-MM-DD\); \/hours: at most 24/)
    const first = await api.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3, note: `meet` } })
    expect(first.created).toBe(true)
    const patched = await api.upsert({ id: appId, kind: `entry`, data: { hours: 3.5 }, recordId: first.id })
    expect(patched).toMatchObject({ id: first.id, created: false })
    const [row] = await db.select().from(appRecords).where(eq(appRecords.id, first.id))
    expect(row!.data).toEqual({ date: `2026-10-05`, hours: 3.5, note: `meet` })
    expect(row!.createdBy).toBe(w.mate)
    expect(row!.authorId).toBe(w.mate)
    expect(row!.updatedBy).toBe(w.mate)
    expect(row!.appDeletedAt).toBeNull()
    const listed = await api.list({ id: appId, kind: `entry` })
    expect(listed.records.map((r) => r.id)).toEqual([first.id])
    // Trashing the app mirrors onto its records (the shape's static predicate).
    await caller(w.owner).delete({ id: appId })
    const [after] = await db.select().from(appRecords).where(eq(appRecords.id, first.id))
    expect(after!.appDeletedAt).not.toBeNull()
    await expect(api.delete({ id: appId, recordId: first.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  // eApps R5 (#VAPP-219): refs resolve on every write (a record of the
  // target kind of the app, an issue / a member of the team: else
  // BAD_REQUEST naming the field); a query's expand inlines them; a
  // deleted target dangles (null), never cascades.
  it(`R5: refs are checked on write and followed by expand`, async () => {
    const w = await world()
    const pkg = {
      ...RECORDS_PACKAGE,
      id: `t.crm`,
      records: {
        company: { schema: { type: `object`, properties: { name: { type: `string` } } } },
        deal: { schema: { type: `object`, properties: { title: { type: `string` }, customer: { type: `string` }, owner: { type: `string` }, task: { type: `string` } } }, refs: { customer: { kind: `company` }, owner: `member`, task: `issue` } },
      },
    }
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `CRM`, package: pkg })
    const appId = created.app.id
    const api = caller(w.mate).records
    const [board] = await db.insert(boards).values({ teamId: w.teamId, name: `Ops`, slug: `ops-${id8()}`, prefix: `OPS` }).returning({ id: boards.id })
    const [issue] = await db.insert(issues).values({ boardId: board!.id, teamId: w.teamId, title: `Ship it`, number: 7, identifier: `OPS-7` }).returning({ id: issues.id, identifier: issues.identifier })
    const acme = await api.upsert({ id: appId, kind: `company`, data: { name: `Acme` } })
    const deal = await api.upsert({ id: appId, kind: `deal`, data: { title: `Big`, customer: acme.id, owner: w.owner, task: issue!.identifier } })
    await expect(api.upsert({ id: appId, kind: `deal`, data: { customer: deal.id } })).rejects.toThrow(/\/customer: no company record/)
    await expect(api.upsert({ id: appId, kind: `deal`, data: { customer: `not-a-uuid` } })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(api.upsert({ id: appId, kind: `deal`, data: { owner: `nobody` } })).rejects.toThrow(/\/owner: no member nobody in the team/)
    await expect(api.upsert({ id: appId, kind: `deal`, data: { task: `OPS-99` }, recordId: deal.id })).rejects.toThrow(/\/task: no issue OPS-99 in the team/)
    await expect(api.batch({ id: appId, writes: [{ op: `upsert`, kind: `deal`, data: { title: `ok` } }, { op: `upsert`, kind: `deal`, data: { owner: `nobody` } }] })).rejects.toThrow(/^write 1: /)
    const q = await api.query({ id: appId, kind: `deal`, query: { expand: [`customer`, `owner`, `task`] } })
    expect(q.rows[0]).toMatchObject({ title: `Big`, customer: { id: acme.id, name: `Acme` }, owner: { id: w.owner, name: `Tester` }, task: { id: issue!.id, identifier: `OPS-7`, title: `Ship it` } })
    await expect(api.query({ id: appId, kind: `deal`, query: { expand: [`title`] } })).rejects.toThrow(/\/expand\/0: title is not a ref of deal/)
    // A deleted target: the ref dangles and expands to null.
    await api.delete({ id: appId, recordId: acme.id })
    const after = await api.query({ id: appId, kind: `deal`, query: { expand: [`customer`] } })
    expect(after.rows[0]).toMatchObject({ customer: null })
    const [stored] = await db.select().from(appRecords).where(eq(appRecords.id, deal.id))
    expect(stored!.data).toMatchObject({ customer: acme.id })
  })

  // eApps R6 (#VAPP-222, record-transitions.json): the records service
  // refuses an undeclared edge (BAD_REQUEST naming it) and a mover the
  // edge's `who` excludes (FORBIDDEN), on upsert, batch and restore.
  it(`R6: transitions are enforced on every write`, async () => {
    const w = await world()
    const approver = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: approver, role: `member` })
    const pkg = {
      ...RECORDS_PACKAGE,
      id: `t.expenses`,
      records: {
        expense: {
          schema: { type: `object`, properties: { title: { type: `string` }, status: { type: `string`, enum: [`requested`, `approved`, `rejected`, `paid`] }, approver: { type: `string` } } },
          policy: { update: `member` },
          refs: { approver: `member` },
          transitions: { field: `status`, from: { requested: [`approved`, `rejected`], approved: [`paid`] }, who: { "requested->approved": `role:approver`, paid: `owners` } },
        },
      },
    }
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Expenses`, package: pkg })
    const appId = created.app.id
    const mate = caller(w.mate).records
    await expect(mate.upsert({ id: appId, kind: `expense`, data: { title: `Laptop`, status: `approved` } })).rejects.toThrow(/status: a new record starts in requested/)
    const e = await mate.upsert({ id: appId, kind: `expense`, data: { title: `Laptop`, status: `requested`, approver } })
    await expect(mate.upsert({ id: appId, kind: `expense`, data: { status: `paid` }, recordId: e.id })).rejects.toMatchObject({ code: `BAD_REQUEST`, message: `status: requested → paid is not a declared transition` })
    await expect(mate.upsert({ id: appId, kind: `expense`, data: { status: `approved` }, recordId: e.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(mate.batch({ id: appId, writes: [{ op: `upsert`, kind: `expense`, id: e.id, data: { status: `approved`, approver: w.mate } }] })).rejects.toThrow(/^write 0: Only the record's approver/)
    await mate.upsert({ id: appId, kind: `expense`, data: { title: `Laptop 2` }, recordId: e.id })
    await caller(approver).records.upsert({ id: appId, kind: `expense`, data: { status: `approved` }, recordId: e.id })
    await expect(caller(approver).records.upsert({ id: appId, kind: `expense`, data: { status: `paid` }, recordId: e.id })).rejects.toThrow(/Only team owners can move status to paid/)
    await caller(w.owner).records.upsert({ id: appId, kind: `expense`, data: { status: `paid` }, recordId: e.id })
    const [row] = await db.select().from(appRecords).where(eq(appRecords.id, e.id))
    expect(row!.data).toMatchObject({ status: `paid` })
  })

  // eApps R8 (#VAPP-233, record-migrations.json): a major bump that changes
  // a published kind incompatibly needs a migration that covers it.
  it(`R8: an uncovered incompatible major bump is refused at upsert, naming the kind`, async () => {
    const w = await world()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const id = created.app.id
    const entry = RECORDS_PACKAGE.records.entry
    const v2 = { ...RECORDS_PACKAGE, version: `2.0.0`, records: { entry: { schema: { ...entry.schema, properties: { date: entry.schema.properties.date, hours: entry.schema.properties.hours } } } } }
    await expect(caller(w.owner).upsertPackage({ id, package: v2 })).rejects.toMatchObject({ code: `BAD_REQUEST`, message: `entry drops note in 2.0.0: add a migration from 1.0.0 that covers it` })
    // The same change on a minor bump, or covered by a migration, is a draft.
    await caller(w.owner).upsertPackage({ id, package: { ...v2, version: `1.1.0` } })
    const covered = await caller(w.owner).upsertPackage({ id, package: { ...v2, migrations: [{ from: `^1`, kind: `entry`, ops: [{ drop: { field: `note` } }] }] } })
    expect(covered.version).toBeGreaterThan(1)
  })

  // Release train 2026-10-25: a patch merges onto the row locked in its own
  // transaction, so concurrent patches of different fields both land.
  it(`records: concurrent patches of one record never lose a field`, async () => {
    const w = await world()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const appId = created.app.id
    // VAPP-187: the mate's own row, so both the author and an owner may patch it.
    const first = await caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3 } })
    await Promise.all([
      caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { hours: 4 }, recordId: first.id }),
      caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { note: `meet` }, recordId: first.id }),
    ])
    const [row] = await db.select().from(appRecords).where(eq(appRecords.id, first.id))
    expect(row!.data).toEqual({ date: `2026-10-05`, hours: 4, note: `meet` })
  })

  // VAPP-215 (R4, app-records.json `history`): every write appends its event
  // in the write's transaction (under the eapps role); history reads them
  // newest first; restore + undo are new writes; reading needs read policy.
  it(`records: history, restore and undo`, async () => {
    const w = await world()
    const third = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: third, role: `member` })
    const appId = (await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })).app.id
    const api = caller(w.mate).records
    const made = await api.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3 }, via: `editor` })
    const saved = await api.upsert({ id: appId, kind: `entry`, data: { hours: 4, note: `meet` }, recordId: made.id, via: `editor` })
    let { events } = await api.history({ id: appId, recordId: made.id })
    expect(events.map((e) => [e.op, e.via, e.actor])).toEqual([
      [`update`, `editor`, w.mate],
      [`create`, `editor`, w.mate],
    ])
    expect(events[0]).toMatchObject({ id: saved.eventId, patch: { hours: 4, note: `meet` }, before: { date: `2026-10-05`, hours: 3 } })
    // Undo the save = restore before.
    await api.undo({ id: appId, eventIds: [saved.eventId] })
    let [row] = await db.select().from(appRecords).where(eq(appRecords.id, made.id))
    expect(row!.data).toEqual({ date: `2026-10-05`, hours: 3 })
    // Delete, then undo = live again with its data.
    const gone = await api.delete({ id: appId, recordId: made.id, via: `editor` })
    await api.undo({ id: appId, eventIds: [gone.eventId] })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, made.id))
    expect(row!.deletedAt).toBeNull()
    // Restore the save's state from History.
    await api.restore({ id: appId, recordId: made.id, eventId: saved.eventId })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, made.id))
    expect(row!.data).toEqual({ date: `2026-10-05`, hours: 4, note: `meet` })
    ;({ events } = await api.history({ id: appId, recordId: made.id }))
    expect(events.map((e) => e.op)).toEqual([`restore`, `restore`, `delete`, `restore`, `update`, `create`])
    // Another member reads the history (read: member) but may not restore (update: author).
    expect((await caller(third).records.history({ id: appId, recordId: made.id })).events).toHaveLength(6)
    await expect(caller(third).records.restore({ id: appId, recordId: made.id, eventId: saved.eventId })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // Undo a create = delete; a package action's batch logs `action`.
    const batch = await api.batch({ id: appId, writes: [{ op: `upsert`, kind: `entry`, data: { date: `2026-10-06`, hours: 1 } }] })
    expect(batch.eventIds).toHaveLength(1)
    expect((await api.history({ id: appId, recordId: batch.ids[0]! })).events[0]).toMatchObject({ op: `create`, via: `action` })
    await api.undo({ id: appId, eventIds: batch.eventIds })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, batch.ids[0]!))
    expect(row!.deletedAt).not.toBeNull()
  })

  // VAPP-235 (O1, app-records.json `outbox`): a native outbox replays a write
  // with its client write id; the server answers the FIRST result and writes
  // nothing again (one history event), per app and actor; a create brings its
  // client id, a taken id = CONFLICT.
  it(`records: an outbox replay is deduped on its write id`, async () => {
    const w = await world()
    const appId = (await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })).app.id
    const api = caller(w.mate).records
    const newId = crypto.randomUUID()
    const writeId = crypto.randomUUID()
    const first = await api.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3 }, newId, writeId })
    expect(first).toMatchObject({ id: newId, created: true })
    const again = await api.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3 }, newId, writeId })
    expect(again).toMatchObject({ id: newId, created: true, eventId: first.eventId })
    // A taken client id without the write id = CONFLICT.
    await expect(api.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 1 }, newId })).rejects.toMatchObject({ code: `CONFLICT` })
    // An update replayed twice patches once.
    const patchId = crypto.randomUUID()
    const patched = await api.upsert({ id: appId, kind: `entry`, data: { hours: 4 }, recordId: newId, writeId: patchId })
    await caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { hours: 9 }, recordId: newId })
    expect((await api.upsert({ id: appId, kind: `entry`, data: { hours: 4 }, recordId: newId, writeId: patchId })).eventId).toBe(patched.eventId)
    let [row] = await db.select().from(appRecords).where(eq(appRecords.id, newId))
    expect(row!.data).toMatchObject({ hours: 9 })
    // A batch replays its ids and events; a delete replays its event.
    const batchId = crypto.randomUUID()
    const batchNew = crypto.randomUUID()
    const batch = await api.batch({ id: appId, writes: [{ op: `upsert`, kind: `entry`, data: { date: `2026-10-06`, hours: 1 }, newId: batchNew }, { op: `upsert`, kind: `entry`, data: { note: `x` }, id: newId }], writeId: batchId })
    expect(batch.ids).toEqual([batchNew, newId])
    expect(await api.batch({ id: appId, writes: [{ op: `upsert`, kind: `entry`, data: { date: `2026-10-06`, hours: 1 }, newId: batchNew }], writeId: batchId })).toMatchObject({ ids: batch.ids, eventIds: batch.eventIds })
    const deleteId = crypto.randomUUID()
    const gone = await api.delete({ id: appId, recordId: batchNew, writeId: deleteId })
    expect((await api.delete({ id: appId, recordId: batchNew, writeId: deleteId })).eventId).toBe(gone.eventId)
    // A plain second delete (no write id) is NOT_FOUND, as before.
    await expect(api.delete({ id: appId, recordId: batchNew })).rejects.toMatchObject({ code: `NOT_FOUND` })
    // One event per first write: create, update (mate), update (owner), batch update.
    const { events } = await api.history({ id: appId, recordId: newId })
    expect(events.map((e) => e.op)).toEqual([`update`, `update`, `update`, `create`])
    // Another member's same write id is their own write (the key is per actor).
    const theirs = await caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-07`, hours: 2 }, writeId })
    expect(theirs.id).not.toBe(newId)
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, theirs.id))
    expect(row!.createdBy).toBe(w.owner)
  })

  // VAPP-187 (app-records.json `ownership`): only the record's author or a
  // team owner patches or deletes it; anyone else = FORBIDDEN, row untouched.
  it(`records: only the author or a team owner may patch or delete a record`, async () => {
    const w = await world()
    const third = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: third, role: `member` })
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const appId = created.app.id
    const mine = await caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3 } })
    const other = caller(third).records
    await expect(other.upsert({ id: appId, kind: `entry`, data: { hours: 9 }, recordId: mine.id })).rejects.toMatchObject({
      code: `FORBIDDEN`,
      message: `Only the record's author or a team owner can change it`,
    })
    await expect(other.delete({ id: appId, recordId: mine.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    let [row] = await db.select().from(appRecords).where(eq(appRecords.id, mine.id))
    expect(row!.data).toEqual({ date: `2026-10-05`, hours: 3 })
    expect(row!.deletedAt).toBeNull()
    // Any member still creates (their own row).
    expect((await other.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-06`, hours: 1 } })).created).toBe(true)
    // A team owner always may; the last writer is recorded, the author stays.
    await caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { hours: 4 }, recordId: mine.id })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, mine.id))
    expect(row).toMatchObject({ createdBy: w.mate, authorId: w.mate, updatedBy: w.owner })
    // A legacy row (no author_id) reads its creator.
    await db.update(appRecords).set({ authorId: null }).where(eq(appRecords.id, mine.id))
    await expect(other.delete({ id: appId, recordId: mine.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await caller(w.mate).records.delete({ id: appId, recordId: mine.id })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, mine.id))
    expect(row!.deletedAt).not.toBeNull()
    expect(row!.updatedBy).toBe(w.mate)
  })

  // eApps R3 (record-policy.json): per-kind policies and the member's app
  // access, enforced by the records service, the apps router and the shapes.
  it(`records: per-kind policies and app access`, async () => {
    const { appShapeRules, appRecordClauses, invalidateAppShapeRules } = await import(`@/lib/apps/records-shape`)
    const w = await world()
    const third = await user()
    await db.insert(teamMembers).values({ teamId: w.teamId, userId: third, role: `member` })
    const kind = (policy?: Record<string, string>) => ({ schema: { type: `object`, properties: { note: { type: `string` } } }, ...(policy ? { policy } : {}) })
    const pkg = { ...RECORDS_PACKAGE, records: { ...RECORDS_PACKAGE.records, secret: kind({ read: `author` }), salary: kind({ read: `owners`, create: `owners` }) } }
    const appId = (await caller(w.owner).create({ teamId: w.teamId, name: `Private`, package: pkg })).app.id
    const kinds = async (u: string) => (await caller(u).records.list({ id: appId, limit: 100, offset: 0 })).records.map((r) => `${r.kind}:${r.data.note ?? ``}`).sort()
    // read: author: the author and owners read it, another member does not; a write to it is NOT_FOUND.
    const secret = await caller(w.mate).records.upsert({ id: appId, kind: `secret`, data: { note: `m` } })
    expect(await kinds(w.mate)).toEqual([`secret:m`])
    expect(await kinds(third)).toEqual([])
    expect(await kinds(w.owner)).toEqual([`secret:m`])
    await expect(caller(third).records.upsert({ id: appId, kind: `secret`, data: { note: `x` }, recordId: secret.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(caller(third).records.delete({ id: appId, recordId: secret.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
    // create: owners / read: owners.
    await expect(caller(w.mate).records.upsert({ id: appId, kind: `salary`, data: { note: `s` } })).rejects.toMatchObject({ code: `FORBIDDEN`, message: `Only team owners can add these records` })
    await caller(w.owner).records.upsert({ id: appId, kind: `salary`, data: { note: `s` } })
    expect(await kinds(w.mate)).toEqual([`secret:m`])
    expect(await kinds(w.owner)).toEqual([`salary:s`, `secret:m`])
    // An action batch is judged write by write, all or nothing.
    await expect(
      caller(w.mate).records.batch({ id: appId, writes: [{ op: `upsert`, kind: `secret`, data: { note: `n` } }, { op: `upsert`, kind: `salary`, data: { note: `t` } }] })
    ).rejects.toMatchObject({ code: `FORBIDDEN`, message: `write 1: Only team owners can add these records` })
    expect(await kinds(w.mate)).toEqual([`secret:m`])
    // The shape rules: a non-owner's private kinds, nothing for an owner.
    invalidateAppShapeRules()
    const rules = await appShapeRules(third, [w.teamId])
    expect(rules.privateKinds.find((k) => k.appId === appId)).toEqual({ appId, owners: [`salary`], author: [`secret`] })
    expect(appRecordClauses(third, rules).join(` `)).toContain(`"kind" IN ('secret')`)
    expect((await appShapeRules(w.owner, [w.teamId])).privateKinds.find((k) => k.appId === appId)).toBeUndefined()
    // Access read: reads, never writes (records or the package).
    await db.insert(appAccess).values({ appId, teamId: w.teamId, userId: w.mate, level: `read` })
    expect(await kinds(w.mate)).toEqual([`secret:m`])
    await expect(caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 1 } })).rejects.toMatchObject({ code: `FORBIDDEN`, message: `You have read-only access to this app` })
    await expect(caller(w.mate).records.delete({ id: appId, recordId: secret.id })).rejects.toMatchObject({ code: `FORBIDDEN` })
    await expect(caller(w.mate).upsertPackage({ id: appId, package: pkg })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // Access none: the app is gone for them (router, records, shapes).
    await db.update(appAccess).set({ level: `none` }).where(eq(appAccess.userId, w.mate))
    await expect(caller(w.mate).records.list({ id: appId, limit: 10, offset: 0 })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(caller(w.mate).get({ id: appId })).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect((await caller(w.mate).list({ teamId: w.teamId })).apps.map((a) => a.id)).not.toContain(appId)
    expect((await caller(third).list({ teamId: w.teamId })).apps.map((a) => a.id)).toContain(appId)
    invalidateAppShapeRules()
    expect((await appShapeRules(w.mate, [w.teamId])).hiddenAppIds).toEqual([appId])
  })

  // VAPP-188 (app-records.json `onBehalf`): an owner writes for another
  // member (created_by/updated_by = the owner, author_id = the member) and
  // may reassign; a member writing for someone else is FORBIDDEN, a
  // stranger as the author is BAD_REQUEST.
  it(`records: on-behalf writes are an owner's, with the real writer kept`, async () => {
    const w = await world()
    const stranger = await user()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const appId = created.app.id
    const forMate = await caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 2 }, author: w.mate })
    let [row] = await db.select().from(appRecords).where(eq(appRecords.id, forMate.id))
    expect(row).toMatchObject({ createdBy: w.owner, authorId: w.mate, updatedBy: w.owner })
    // The mate owns it now: they may patch it, as themself.
    await caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { hours: 3 }, recordId: forMate.id, author: w.mate })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, forMate.id))
    expect(row).toMatchObject({ createdBy: w.owner, authorId: w.mate, updatedBy: w.mate })
    // A member may not write for anyone else, on create or update.
    await expect(caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-06`, hours: 1 }, author: w.owner })).rejects.toMatchObject({
      code: `FORBIDDEN`,
      message: `Only a team owner can write for another member`,
    })
    await expect(caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { hours: 4 }, recordId: forMate.id, author: w.owner })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // A stranger can't be an author (the owner's create and reassignment).
    await expect(caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-06`, hours: 1 }, author: stranger })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: `The author must be a member of the team`,
    })
    await expect(caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { hours: 5 }, recordId: forMate.id, author: stranger })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, forMate.id))
    expect(row).toMatchObject({ authorId: w.mate, data: { date: `2026-10-05`, hours: 3 } })
    // An owner reassigns the row back to themself.
    await caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: {}, recordId: forMate.id, author: w.owner })
    ;[row] = await db.select().from(appRecords).where(eq(appRecords.id, forMate.id))
    expect(row).toMatchObject({ createdBy: w.owner, authorId: w.owner, updatedBy: w.owner })
    const live = await db.select({ id: appRecords.id }).from(appRecords).where(eq(appRecords.appId, appId))
    expect(live).toHaveLength(1)
  })

  // VAPP-188: an imported record's `author` (onBehalf.importCases).
  it(`import: record authors resolve against the importing team`, async () => {
    const w = await world()
    const stranger = await user()
    const { buildAppExport, appExportText } = await import(`@exp/domain-contract/app-export`)
    const file = JSON.parse(appExportText(buildAppExport({ app: { name: `Hours` }, package: RECORDS_PACKAGE, includeRecords: false })))
    file.records = [
      { kind: `entry`, data: { date: `2026-10-05`, hours: 1 }, author: w.mate },
      { kind: `entry`, data: { date: `2026-10-06`, hours: 2 }, author: stranger },
      { kind: `entry`, data: { date: `2026-10-07`, hours: 3 } },
    ]
    const text = JSON.stringify(file)
    // VAPP-192: a member importing another member's rows (a teammate's
    // export) keeps them as their own; nothing is refused.
    const forOwner = JSON.stringify({ ...file, records: [{ kind: `entry`, data: { date: `2026-10-05`, hours: 1 }, author: w.owner }] })
    expect((await caller(w.mate).validateImport({ teamId: w.teamId, text: forOwner })).issues).toEqual([])
    const asMine = await caller(w.mate).import({ teamId: w.teamId, text: forOwner })
    const [kept] = await db.select().from(appRecords).where(eq(appRecords.appId, asMine.app.id))
    expect(kept).toMatchObject({ createdBy: w.mate, authorId: w.mate })
    // An owner keeps a member's author; a stranger and none = the importer.
    expect((await caller(w.owner).validateImport({ teamId: w.teamId, text })).issues).toEqual([])
    const imported = await caller(w.owner).import({ teamId: w.teamId, text })
    const rows = await db.select().from(appRecords).where(eq(appRecords.appId, imported.app.id))
    const byHours = Object.fromEntries(rows.map((r) => [(r.data as { hours: number }).hours, r]))
    expect(byHours[1]).toMatchObject({ createdBy: w.owner, authorId: w.mate, updatedBy: w.owner })
    expect(byHours[2]).toMatchObject({ createdBy: w.owner, authorId: w.owner })
    expect(byHours[3]).toMatchObject({ createdBy: w.owner, authorId: w.owner })
    // A member importing a stranger's rows keeps them as their own.
    const theirs = await caller(w.mate).import({ teamId: w.teamId, text: JSON.stringify({ ...file, records: [file.records[1]] }) })
    const [mine] = await db.select().from(appRecords).where(eq(appRecords.appId, theirs.app.id))
    expect(mine).toMatchObject({ createdBy: w.mate, authorId: w.mate })
  })

  it(`records: delete trashes one row and a second delete is NOT_FOUND`, async () => {
    const w = await world()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const api = caller(w.owner).records
    const made = await api.upsert({ id: created.app.id, kind: `entry`, data: { date: `2026-10-06`, hours: 1 } })
    await api.delete({ id: created.app.id, recordId: made.id })
    const [row] = await db.select().from(appRecords).where(eq(appRecords.id, made.id))
    expect(row!.deletedAt).not.toBeNull()
    await expect(api.delete({ id: created.app.id, recordId: made.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
    expect((await api.list({ id: created.app.id })).records).toEqual([])
  })
  // VAPP-120: the grants are exact action names the HEAD package declares,
  // written by a manager; a new version prunes what it no longer declares.
  it(`grants: update keeps exact declared non-destructive actions, refuses anything else, owners only; a new version prunes`, async () => {
    const w = await world()
    const declaring = (functions: string[]) => ({ ...RECORDS_PACKAGE, functions })
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: declaring([`exponential.comments.create`, `exponential.issues.*`]) })
    const updated = await caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [`exponential.comments.create`, `exponential.issues.update_status`] })
    expect(updated.app.grantedTools).toEqual([`exponential.comments.create`, `exponential.issues.update_status`])
    // A pattern is not a grant; an offered but undeclared action is refused too (review of #1222).
    await expect(caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [`exponential.issues.*`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [`exponential.boards.delete`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(w.mate).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // The 2026-10-24 ruling: a destructive action is never grantable, even when declared.
    await expect(caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [`exponential.issues.delete`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // …and the creator gets no grant power from creating the app: owners only.
    const mine = await caller(w.mate).create({ teamId: w.teamId, name: `Mine`, package: declaring([`exponential.comments.create`]) })
    await expect(caller(w.mate).update({ id: mine.app.id, headVersion: await seen(mine.app.id), grantedTools: [`exponential.comments.create`] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect((await caller(w.mate).update({ id: mine.app.id, description: `still mine to edit` })).app.description).toBe(`still mine to edit`)
    expect((await caller(w.owner).update({ id: mine.app.id, headVersion: await seen(mine.app.id), grantedTools: [`exponential.comments.create`] })).app.grantedTools).toEqual([`exponential.comments.create`])
    // An owner's version that drops an action drops its grant; re-declaring it later starts ungranted.
    await ship(w.owner, { id: created.app.id, package: declaring([`exponential.comments.create`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([`exponential.comments.create`])
    await ship(w.owner, { id: created.app.id, package: declaring([`exponential.comments.create`, `exponential.issues.update_status`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([`exponential.comments.create`])
    // Release train 2026-10-25: a member's code never runs on an owner's grant: their push clears every grant.
    await ship(w.mate, { id: created.app.id, package: declaring([`exponential.comments.create`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([])
  })

  it(`grants: an owner's new version keeps the still-declared grants; a member's upsert or restore clears them`, async () => {
    const w = await world()
    const pkg = { ...RECORDS_PACKAGE, functions: [`exponential.comments.create`, `exponential.issues.update_status`] }
    const granted = [`exponential.comments.create`, `exponential.issues.update_status`]
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: pkg })
    const grants = async () => (await caller(w.owner).get({ id: created.app.id })).app.grantedTools
    await caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: granted })
    await ship(w.owner, { id: created.app.id, package: pkg })
    expect(await grants()).toEqual(granted)
    await shipRestore(w.owner, created.app.id, 1)
    expect(await grants()).toEqual(granted)
    // A member's restore of the very package the owner granted still clears them.
    await shipRestore(w.mate, created.app.id, 1)
    expect(await grants()).toEqual([])
    await caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: granted })
    await ship(w.mate, { id: created.app.id, package: pkg })
    expect(await grants()).toEqual([])
  })
  // VAPP-156: a grant lands only on the version the owner saw. A member
  // publishing between the owner's read and the grant = CONFLICT, nothing
  // written; a grant without the version is refused at the input.
  it(`grants: a version published between read and grant refuses with CONFLICT`, async () => {
    const w = await world()
    const pkg = { ...RECORDS_PACKAGE, functions: [`exponential.comments.create`] }
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: pkg })
    const sawVersion = await seen(created.app.id)
    await ship(w.mate, { id: created.app.id, package: pkg })
    await expect(caller(w.owner).update({ id: created.app.id, headVersion: sawVersion, grantedTools: [`exponential.comments.create`] })).rejects.toMatchObject({ code: `CONFLICT` })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([])
    await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.comments.create`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // Reloaded: the owner grants on the head they now see.
    const granted = await caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [`exponential.comments.create`] })
    expect(granted.app.grantedTools).toEqual([`exponential.comments.create`])
    // A rename never needs the version.
    expect((await caller(w.owner).update({ id: created.app.id, name: `Hours 2` })).app.name).toBe(`Hours 2`)
  })

  // VAPP-158: an owner-only action is never grantable: the update refuses
  // it, and a grant stored before the rule never survives a new head.
  it(`grants: an owner-only action is refused and a stored one is dropped on upsert and restore`, async () => {
    const w = await world()
    const pkg = { ...RECORDS_PACKAGE, functions: [`exponential.comments.create`, `exponential.teams.update`, `exponential.people.*`] }
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: pkg })
    for (const name of [`exponential.teams.update`, `exponential.people.add`, `exponential.people.invites_create`]) {
      await expect(caller(w.owner).update({ id: created.app.id, headVersion: await seen(created.app.id), grantedTools: [`exponential.comments.create`, name] }), name).rejects.toMatchObject({ code: `BAD_REQUEST` })
    }
    const grants = async () => (await caller(w.owner).get({ id: created.app.id })).app.grantedTools
    expect(await grants()).toEqual([])
    // A row written before the rule: an owner's new version drops the owner-only grant.
    const stale = [`exponential.comments.create`, `exponential.teams.update`, `exponential.people.add`]
    await db.update(apps).set({ grantedTools: stale }).where(eq(apps.id, created.app.id))
    await ship(w.owner, { id: created.app.id, package: pkg })
    expect(await grants()).toEqual([`exponential.comments.create`])
    await db.update(apps).set({ grantedTools: stale }).where(eq(apps.id, created.app.id))
    await shipRestore(w.owner, created.app.id, 1)
    expect(await grants()).toEqual([`exponential.comments.create`])
  })

  // VAPP-121: an app file round-trips: the import lists issues as rows, then
  // creates the app as v1 "Imported" with its records as the importer's and
  // no grants; a bare package imports under its own name.
  it(`import: issues as rows, an exported file round-trips, a bare package imports`, async () => {
    const w = await world()
    const { buildAppExport, appExportText } = await import(`@exp/domain-contract/app-export`)
    const declaring = { ...RECORDS_PACKAGE, functions: [`exponential.comments.create`] }
    const source = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, icon: `clock`, color: `#10b981`, description: `Mine`, package: declaring })
    await caller(w.owner).update({ id: source.app.id, headVersion: await seen(source.app.id), grantedTools: [`exponential.comments.create`] })
    await caller(w.owner).records.upsert({ id: source.app.id, kind: `entry`, data: { date: `2026-10-05`, hours: 3 } })
    const records = await db.select().from(appRecords).where(eq(appRecords.appId, source.app.id))
    const text = appExportText(
      buildAppExport({ app: source.app, package: declaring, records: records.map((r) => ({ ...r, data: r.data as Record<string, unknown> })), includeRecords: true })
    )

    // Issues: a record off its kind's schema, an undeclared kind, a broken package.
    const bad = JSON.parse(text)
    bad.records.push({ kind: `entry`, data: { date: `2026-10-06`, hours: 99 } }, { kind: `nope`, data: {} })
    const checked = await caller(w.mate).validateImport({ teamId: w.teamId, text: JSON.stringify(bad) })
    expect(checked.issues.map((i: { path: string }) => i.path)).toEqual([`/records/1/data/hours`, `/records/2/kind`])
    const broken = await caller(w.mate).validateImport({ teamId: w.teamId, text: JSON.stringify({ ...bad, package: { id: `x` } }) })
    expect(broken.issues.length).toBeGreaterThan(0)
    expect(broken.issues.every((i: { path: string }) => i.path.startsWith(`/package`))).toBe(true)
    await expect(caller(w.mate).import({ teamId: w.teamId, text: JSON.stringify(bad) })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // Release train 2026-10-25: a NUL (jsonb refuses it) is an issue row and a BAD_REQUEST, never a 500.
    const nul = JSON.parse(text)
    nul.app.name = `Hours\u0000`
    nul.records[0].data.note = `x\u0000`
    expect((await caller(w.mate).validateImport({ teamId: w.teamId, text: JSON.stringify(nul) })).issues).toEqual([
      { path: `/app/name`, message: `must not contain NUL` },
      { path: `/records/0/data/note`, message: `must not contain NUL` },
    ])
    await expect(caller(w.mate).import({ teamId: w.teamId, text: JSON.stringify(nul) })).rejects.toMatchObject({ code: `BAD_REQUEST` })

    // The round trip: a member imports; the name's slug is taken, so `-2`.
    expect(await caller(w.mate).validateImport({ teamId: w.teamId, text })).toEqual({ issues: [], name: `Hours`, records: 1 })
    const imported = await caller(w.mate).import({ teamId: w.teamId, text })
    expect(imported.app).toMatchObject({ slug: `hours-2`, name: `Hours`, icon: `clock`, color: `#10b981`, description: `Mine`, headVersion: 1, createdBy: w.mate, grantedTools: [] })
    const [v1] = await db.select().from(appVersions).where(eq(appVersions.appId, imported.app.id))
    expect(v1).toMatchObject({ number: 1, note: `Imported`, createdBy: w.mate })
    const copies = await db.select().from(appRecords).where(eq(appRecords.appId, imported.app.id))
    expect(copies.map((r) => ({ kind: r.kind, data: r.data, createdBy: r.createdBy }))).toEqual([{ kind: `entry`, data: { date: `2026-10-05`, hours: 3 }, createdBy: w.mate }])

    // A bare package, named by its name.
    const bare = await caller(w.mate).import({ teamId: w.teamId, text: JSON.stringify(RECORDS_PACKAGE) })
    expect(bare.app).toMatchObject({ name: `Hours`, slug: `hours-3`, icon: null, headVersion: 1 })
    await expect(caller(await user()).validateImport({ teamId: w.teamId, text })).rejects.toBeTruthy()
  })

  // VAPP-154: same-name imports at once race on the slug; the loser retries
  // with the next free one (like a sequential duplicate), never a 500.
  it(`import: concurrent same-name imports each land on their own slug`, async () => {
    const w = await world()
    const text = JSON.stringify({ ...RECORDS_PACKAGE, name: `Race` })
    const results = await Promise.allSettled([1, 2, 3, 4].map(() => caller(w.mate).import({ teamId: w.teamId, text })))
    const failures = results.flatMap((r) => (r.status === `rejected` ? [r.reason as { code?: string }] : []))
    expect(failures.every((e) => e.code === `CONFLICT`)).toBe(true)
    const slugs = results.flatMap((r) => (r.status === `fulfilled` ? [r.value.app.slug] : []))
    expect(new Set(slugs).size).toBe(slugs.length)
    expect(slugs.length).toBeGreaterThanOrEqual(1)
    expect(slugs).toContain(`race`)
  })
})
