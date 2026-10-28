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

  // Release train 2026-10-25: a patch merges onto the row locked in its own
  // transaction, so concurrent patches of different fields both land.
  it(`records: concurrent patches of one record never lose a field`, async () => {
    const w = await world()
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: RECORDS_PACKAGE })
    const appId = created.app.id
    const first = await caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { date: `2026-10-05`, hours: 3 } })
    await Promise.all([
      caller(w.owner).records.upsert({ id: appId, kind: `entry`, data: { hours: 4 }, recordId: first.id }),
      caller(w.mate).records.upsert({ id: appId, kind: `entry`, data: { note: `meet` }, recordId: first.id }),
    ])
    const [row] = await db.select().from(appRecords).where(eq(appRecords.id, first.id))
    expect(row!.data).toEqual({ date: `2026-10-05`, hours: 4, note: `meet` })
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
    const updated = await caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.comments.create`, `exponential.issues.update_status`] })
    expect(updated.app.grantedTools).toEqual([`exponential.comments.create`, `exponential.issues.update_status`])
    // A pattern is not a grant; an offered but undeclared action is refused too (review of #1222).
    await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.issues.*`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.boards.delete`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller(w.mate).update({ id: created.app.id, grantedTools: [] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    // The 2026-10-24 ruling: a destructive action is never grantable, even when declared.
    await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.issues.delete`] })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    // …and the creator gets no grant power from creating the app: owners only.
    const mine = await caller(w.mate).create({ teamId: w.teamId, name: `Mine`, package: declaring([`exponential.comments.create`]) })
    await expect(caller(w.mate).update({ id: mine.app.id, grantedTools: [`exponential.comments.create`] })).rejects.toMatchObject({ code: `FORBIDDEN` })
    expect((await caller(w.mate).update({ id: mine.app.id, description: `still mine to edit` })).app.description).toBe(`still mine to edit`)
    expect((await caller(w.owner).update({ id: mine.app.id, grantedTools: [`exponential.comments.create`] })).app.grantedTools).toEqual([`exponential.comments.create`])
    // An owner's version that drops an action drops its grant; re-declaring it later starts ungranted.
    await caller(w.owner).upsertPackage({ id: created.app.id, package: declaring([`exponential.comments.create`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([`exponential.comments.create`])
    await caller(w.owner).upsertPackage({ id: created.app.id, package: declaring([`exponential.comments.create`, `exponential.issues.update_status`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([`exponential.comments.create`])
    // Release train 2026-10-25: a member's code never runs on an owner's grant: their push clears every grant.
    await caller(w.mate).upsertPackage({ id: created.app.id, package: declaring([`exponential.comments.create`]) })
    expect((await caller(w.owner).get({ id: created.app.id })).app.grantedTools).toEqual([])
  })

  it(`grants: an owner's new version keeps the still-declared grants; a member's upsert or restore clears them`, async () => {
    const w = await world()
    const pkg = { ...RECORDS_PACKAGE, functions: [`exponential.comments.create`, `exponential.issues.update_status`] }
    const granted = [`exponential.comments.create`, `exponential.issues.update_status`]
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: pkg })
    const grants = async () => (await caller(w.owner).get({ id: created.app.id })).app.grantedTools
    await caller(w.owner).update({ id: created.app.id, grantedTools: granted })
    await caller(w.owner).upsertPackage({ id: created.app.id, package: pkg })
    expect(await grants()).toEqual(granted)
    await caller(w.owner).restore({ id: created.app.id, number: 1 })
    expect(await grants()).toEqual(granted)
    // A member's restore of the very package the owner granted still clears them.
    await caller(w.mate).restore({ id: created.app.id, number: 1 })
    expect(await grants()).toEqual([])
    await caller(w.owner).update({ id: created.app.id, grantedTools: granted })
    await caller(w.mate).upsertPackage({ id: created.app.id, package: pkg })
    expect(await grants()).toEqual([])
  })
  // VAPP-158: an owner-only action is never grantable: the update refuses
  // it, and a grant stored before the rule never survives a new head.
  it(`grants: an owner-only action is refused and a stored one is dropped on upsert and restore`, async () => {
    const w = await world()
    const pkg = { ...RECORDS_PACKAGE, functions: [`exponential.comments.create`, `exponential.teams.update`, `exponential.people.*`] }
    const created = await caller(w.owner).create({ teamId: w.teamId, name: `Hours`, package: pkg })
    for (const name of [`exponential.teams.update`, `exponential.people.add`, `exponential.people.invites_create`]) {
      await expect(caller(w.owner).update({ id: created.app.id, grantedTools: [`exponential.comments.create`, name] }), name).rejects.toMatchObject({ code: `BAD_REQUEST` })
    }
    const grants = async () => (await caller(w.owner).get({ id: created.app.id })).app.grantedTools
    expect(await grants()).toEqual([])
    // A row written before the rule: an owner's new version drops the owner-only grant.
    const stale = [`exponential.comments.create`, `exponential.teams.update`, `exponential.people.add`]
    await db.update(apps).set({ grantedTools: stale }).where(eq(apps.id, created.app.id))
    await caller(w.owner).upsertPackage({ id: created.app.id, package: pkg })
    expect(await grants()).toEqual([`exponential.comments.create`])
    await db.update(apps).set({ grantedTools: stale }).where(eq(apps.id, created.app.id))
    await caller(w.owner).restore({ id: created.app.id, number: 1 })
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
    await caller(w.owner).update({ id: source.app.id, grantedTools: [`exponential.comments.create`] })
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
