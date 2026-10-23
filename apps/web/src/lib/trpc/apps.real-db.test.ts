// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appVersions, apps, devices, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// Release train 2026-10-23 (VAPP-110 review): the apps router against the
// REAL migrated Postgres. A deleted account leaves its apps behind (creator
// SET NULL, owner-only to manage); inputs are strip-mode; a trashed app frees
// its slug (partial unique index); a CONFLICT names a derived slug; a version
// always names its author and keeps only the caller's own editor session.

// The package format is VAPP-107's concern: any object passes here.
vi.mock(`@/lib/apps/package`, () => ({ validateAppPackage: (pkg: unknown) => pkg }))

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
})
