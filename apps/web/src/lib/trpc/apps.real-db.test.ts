// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appRecords, appVersions, apps, devices, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

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
    id: `t.hours`,
    name: `Hours`,
    version: `1.0.0`,
    catalogId: `https://ui.exponential.at/catalogs/core/v1`,
    templates: { m: { components: [{ id: `root`, component: `Column` }] } },
    sources: [`exp:records`],
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
    expect(row!.appDeletedAt).toBeNull()
    const listed = await api.list({ id: appId, kind: `entry` })
    expect(listed.records.map((r) => r.id)).toEqual([first.id])
    // Trashing the app mirrors onto its records (the shape's static predicate).
    await caller(w.owner).delete({ id: appId })
    const [after] = await db.select().from(appRecords).where(eq(appRecords.id, first.id))
    expect(after!.appDeletedAt).not.toBeNull()
    await expect(api.delete({ id: appId, recordId: first.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
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
  it(`grants: update keeps exact declared actions, refuses anything else, only for a manager; a new version prunes`, async () => {
    const w = await world()
    const declaring = (functions: string[]) => ({ ...RECORDS_PACKAGE, functions })
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: declaring([`exponential.comments.create`, `exponential.issues.*`]) })
    const updated = await caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.comments.create`, `exponential.issues.update_status`] })
    expect(updated.app.grantedTools).toEqual([`exponential.comments.create`, `exponential.issues.update_status`])
    // A pattern is not a grant; an offered but undeclared action is refused too (review of #1222).
    await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.issues.*`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.boards.delete`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(w.mate).update({ id: created.app.id, grantedTools: [] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // A version that drops an action drops its grant; re-declaring it later starts ungranted.
    await caller(w.mate).upsertPackage({ id: created.app.id, package: declaring([`exponential.comments.create`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([`exponential.comments.create`])
    await caller(w.mate).upsertPackage({ id: created.app.id, package: declaring([`exponential.comments.create`, `exponential.issues.update_status`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([`exponential.comments.create`])
  })
})
