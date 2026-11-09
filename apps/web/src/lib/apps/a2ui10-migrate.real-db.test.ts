// @vitest-environment node
import { readFileSync, readdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { HostRouter, availableExtensions, CORE_CATALOG_ID, cardProblems, cardV10, reduceNested, reduceSurface, validatePackage, type AppPackage, type NestedNode } from "@exponential_app/eui"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import fixture from "./a2ui10-migrate.fixture.json"

// VAPP-289 (round 9 §14.1): the migration against the real migrated Postgres.
// Each case rolls back and walks its own seeded team only (`teamId`): the
// real-DB files run side by side on one database, and a table-wide walk read
// their committed rows (a foreign failed row turned `ok` false) and, applying,
// row-locked them until the rollback (their writes and cleanup deadlocked).

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'refresh_conversation_digests_update') AS ok`)
if (!ready.ok) console.info(`[a2ui10-migrate.real-db] skipped: ${ready.reason}`)

const { runA2ui10Migration } = await import(`@/lib/apps/a2ui10-migrate`)
const { APP_CATALOG_IDS, appExtension } = await import(`@/lib/apps/package`)
const { stripUngrantableGrants } = await import(`@/lib/bootstrap-cloud`)

const ROOT = join(dirname(fileURLToPath(import.meta.url)), `..`, `..`, `..`, `..`, `..`)
const id8 = () => crypto.randomUUID().slice(0, 8)
const FIXED = `2026-01-02T03:04:05.000Z`

interface Seed {
  teamId: string
  conversationId: string
  sessionId: string
  app: (slug: string, values: { package?: unknown; draft?: unknown; grants?: unknown }) => Promise<string>
  version: (appId: string, number: number, pkg: unknown) => Promise<string>
  message: (card: unknown) => Promise<string>
}

async function seed(tx: RealDbTx): Promise<Seed> {
  const userId = `a2ui-test-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@a2ui.test`}, true, false, now(), now())`)
  const team = await tx.execute<{ id: string }>(sql`INSERT INTO teams (name, slug) VALUES ('A2UI', ${`a2ui-${id8()}`}) RETURNING id`)
  const teamId = team.rows[0]!.id
  const sessionId = crypto.randomUUID()
  await tx.execute(sql`INSERT INTO sessions (id, team_id, root_id, user_id, device_id) VALUES (${sessionId}, ${teamId}, ${sessionId}, ${userId}, 'dev')`)
  const conv = await tx.execute<{ id: string }>(sql`SELECT id FROM conversations WHERE session_id = ${sessionId}`)
  const conversationId = conv.rows[0]!.id
  return {
    teamId,
    conversationId,
    sessionId,
    app: async (slug, { package: pkg = null, draft = null, grants = [] }) => {
      const json = (value: unknown) => (value === null ? null : JSON.stringify(value))
      const row = await tx.execute<{ id: string }>(sql`
        INSERT INTO apps (team_id, slug, name, package, draft_package, granted_tools, created_at, updated_at)
        VALUES (${teamId}, ${slug}, ${slug}, ${json(pkg)}::jsonb, ${json(draft)}::jsonb, ${JSON.stringify(grants)}::jsonb, ${FIXED}::timestamptz, ${FIXED}::timestamptz)
        RETURNING id`)
      return row.rows[0]!.id
    },
    version: async (appId, number, pkg) => {
      const row = await tx.execute<{ id: string }>(sql`
        INSERT INTO app_versions (app_id, team_id, number, package) VALUES (${appId}, ${teamId}, ${number}, ${JSON.stringify(pkg)}::jsonb) RETURNING id`)
      return row.rows[0]!.id
    },
    message: async (card) => {
      const id = crypto.randomUUID()
      await tx.execute(sql`
        INSERT INTO messages (id, root_id, team_id, conversation_id, author_kind, body, card, created_at, updated_at)
        VALUES (${id}, ${sessionId}, ${teamId}, ${conversationId}, 'agent', '', ${JSON.stringify(card)}::jsonb, ${FIXED}::timestamptz, ${FIXED}::timestamptz)`)
      return id
    },
  }
}

type Client = Parameters<typeof runA2ui10Migration>[0]
const appRow = async (tx: RealDbTx, id: string) =>
  (await tx.execute<{ package: Record<string, unknown> | null; draft_package: Record<string, unknown> | null; granted_tools: string[]; updated_at: string; name: string }>(sql`SELECT * FROM apps WHERE id = ${id}::uuid`)).rows[0]!
const versionPackage = async (tx: RealDbTx, id: string) =>
  (await tx.execute<{ package: Record<string, unknown> }>(sql`SELECT package FROM app_versions WHERE id = ${id}::uuid`)).rows[0]!.package
const messageRow = async (tx: RealDbTx, id: string) =>
  (await tx.execute<{ card: { surface?: { version?: string; root: NestedNode } }; updated_at: string }>(sql`SELECT card, updated_at FROM messages WHERE id = ${id}::uuid`)).rows[0]!
/** Every row of the team's tables as text: a changed bit anywhere shows. */
const snapshot = async (tx: RealDbTx, teamId: string) =>
  JSON.stringify(
    (await tx.execute(sql`SELECT
      (SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM apps a WHERE a.team_id = ${teamId}::uuid) AS apps,
      (SELECT jsonb_agg(to_jsonb(v) ORDER BY v.id) FROM app_versions v WHERE v.team_id = ${teamId}::uuid) AS versions,
      (SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM messages m WHERE m.team_id = ${teamId}::uuid) AS messages,
      (SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM conversations c WHERE c.team_id = ${teamId}::uuid) AS conversations`)).rows
  )
/** The physical versions of the rows: a write of the same content still moves them. */
const xmins = async (tx: RealDbTx, teamId: string) =>
  JSON.stringify((await tx.execute(sql`SELECT array_agg(xmin::text ORDER BY id) AS x FROM apps WHERE team_id = ${teamId}::uuid UNION ALL SELECT array_agg(xmin::text ORDER BY id) FROM app_versions WHERE team_id = ${teamId}::uuid UNION ALL SELECT array_agg(xmin::text ORDER BY id) FROM messages WHERE team_id = ${teamId}::uuid`)).rows)

const CAT = fixture.packages
const router = () => new HostRouter({ extensionIds: [appExtension.id], extensions: availableExtensions([appExtension]) })
/** A stored package through the client's load with the bridge OFF. */
function expectPackageLoads(pkg: unknown) {
  expect(validatePackage(pkg, APP_CATALOG_IDS)).toEqual([])
  const host = router()
  expect(host.installPackage(pkg as AppPackage)).toEqual([])
  const p = pkg as AppPackage
  for (const templateId of Object.keys(p.templates)) {
    const ops = host.route({ version: `1.0`, applyTemplate: { surfaceId: `s_${id8()}`, packageId: p.id, templateId } }, { trusted: true, acceptV09: false })
    expect(ops.filter((op) => op.op === `send`)).toEqual([])
    const painted = ops.find((op) => op.op === `components`)
    if (painted?.op !== `components`) throw new Error(`template ${templateId} paints nothing`)
    expect(reduceSurface(painted.components, { catalogId: p.catalogId, extensions: [appExtension] }).issues).toEqual([])
  }
}
function expectCardLoads(card: { surface?: unknown }) {
  const entry = cardV10(card.surface, false)
  if (`error` in entry) throw new Error(entry.error)
  expect(cardProblems(entry.card.root)).toEqual([])
  expect(reduceNested(entry.card.root, { catalogId: CORE_CATALOG_ID }).issues).toEqual([])
}

async function seedAll(s: Seed) {
  const app = await s.app(`visits-${id8()}`, { package: CAT.fieldVisits, draft: CAT.fieldVisitsDraft, grants: fixture.grants.dotted })
  const version = await s.version(app, 1, CAT.fieldVisits)
  const done = await s.app(`done-${id8()}`, { package: CAT.alreadyV6, grants: fixture.grants.flat })
  const cards = {
    surface: await s.message(fixture.cards.surface),
    table: await s.message(fixture.cards.surfaceTable),
    plan: await s.message(fixture.cards.plan),
    question: await s.message(fixture.cards.question),
    already10: await s.message(fixture.cards.already10),
    sealed: await s.message(fixture.cards.sealed),
    sealedAsk: await s.message(fixture.cards.sealedAsk),
    noSurface: await s.message(fixture.cards.noSurface),
  }
  return { app, version, done, cards }
}

describe.skipIf(!ready.ok)(`a2ui10-migrate on a real database`, () => {
  it(`a dry run counts and changes nothing`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const base = await runA2ui10Migration(tx as Client, { apply: false, teamId: s.teamId })
      await seedAll(s)
      const before = await snapshot(tx, s.teamId)
      const xmin = await xmins(tx, s.teamId)
      const report = await runA2ui10Migration(tx as Client, { apply: false, teamId: s.teamId })
      expect(await snapshot(tx, s.teamId)).toBe(before)
      expect(await xmins(tx, s.teamId)).toBe(xmin)
      expect(report.mode).toBe(`dry-run`)
      const delta = (name: string) => {
        const [now, was] = [report.columns[name]!, base.columns[name]!]
        return { total: now.total - was.total, alreadyV10: now.alreadyV10 - was.alreadyV10, toRewrite: now.toRewrite - was.toRewrite, rewritten: now.rewritten, sealedSkipped: now.sealedSkipped - was.sealedSkipped }
      }
      expect(delta(`apps.package`)).toEqual({ total: 2, alreadyV10: 1, toRewrite: 1, rewritten: 0, sealedSkipped: 0 })
      expect(delta(`apps.draft_package`)).toEqual({ total: 1, alreadyV10: 0, toRewrite: 1, rewritten: 0, sealedSkipped: 0 })
      expect(delta(`apps.granted_tools`)).toEqual({ total: 2, alreadyV10: 1, toRewrite: 1, rewritten: 0, sealedSkipped: 0 })
      expect(delta(`app_versions.package`)).toEqual({ total: 1, alreadyV10: 0, toRewrite: 1, rewritten: 0, sealedSkipped: 0 })
      expect(delta(`messages.card`)).toEqual({ total: 7, alreadyV10: 1, toRewrite: 4, rewritten: 0, sealedSkipped: 2 })
      expect(report.columns[`app_versions.package`]!.synced).toBe(false)
      expect(report.electric.rows - base.electric.rows).toBe(5)
      expect(report.electric.bytes).toBeGreaterThan(base.electric.bytes)
    })
  })

  it(`apply rewrites, keeps timestamps and the digest, and every row loads with the bridge off`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const seeded = await seedAll(s)
      const digest = await tx.execute(sql`SELECT digest, updated_at FROM conversations WHERE id = ${s.conversationId}::uuid`)
      const dry = await runA2ui10Migration(tx as Client, { apply: false, teamId: s.teamId })
      const report = await runA2ui10Migration(tx as Client, { apply: true, batchSize: 3, teamId: s.teamId })
      expect(report.ok).toBe(true)
      for (const [name, column] of Object.entries(report.columns)) expect(column.rewritten, name).toBe(dry.columns[name]!.toRewrite)

      const app = await appRow(tx, seeded.app)
      expect(app.package!.formatVersion).toBe(6)
      expect(app.draft_package!.formatVersion).toBe(6)
      expect(app.draft_package!.version).toBe(`2.4.0-draft`)
      expect(app.granted_tools).toEqual([`exponential_issues_update`, `exponential_mcp_servers_list`, `exponential_issues_create`])
      expect(new Date(app.updated_at).toISOString()).toBe(FIXED)
      expectPackageLoads(app.package)
      expectPackageLoads(app.draft_package)
      expectPackageLoads(await versionPackage(tx, seeded.version))
      expectPackageLoads((await appRow(tx, seeded.done)).package)

      for (const id of [seeded.cards.surface, seeded.cards.table, seeded.cards.plan, seeded.cards.question, seeded.cards.already10]) {
        const row = await messageRow(tx, id)
        expect(row.card.surface!.version).toBe(`1.0`)
        expect(new Date(row.updated_at).toISOString()).toBe(FIXED)
        expectCardLoads(row.card)
      }
      expect((await messageRow(tx, seeded.cards.noSurface)).card).toEqual(fixture.cards.noSurface)

      const after = await tx.execute(sql`SELECT digest, updated_at FROM conversations WHERE id = ${s.conversationId}::uuid`)
      expect(after.rows).toEqual(digest.rows)
    })
  })

  it(`a second run rewrites nothing, and its rows are untouched`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      await seedAll(s)
      await runA2ui10Migration(tx as Client, { apply: true, teamId: s.teamId })
      const before = await snapshot(tx, s.teamId)
      const xmin = await xmins(tx, s.teamId)
      const again = await runA2ui10Migration(tx as Client, { apply: true, teamId: s.teamId })
      expect(again.ok).toBe(true)
      for (const column of Object.values(again.columns)) expect(column).toMatchObject({ toRewrite: 0, rewritten: 0, failed: [], conflicted: [] })
      expect(again.electric).toEqual({ rows: 0, bytes: 0 })
      expect(await snapshot(tx, s.teamId)).toBe(before)
      expect(await xmins(tx, s.teamId)).toBe(xmin)
    })
  })

  it(`sealed cards are skipped and counted, byte for byte`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const sealed = await s.message(fixture.cards.sealed)
      const ask = await s.message(fixture.cards.sealedAsk)
      const report = await runA2ui10Migration(tx as Client, { apply: true, teamId: s.teamId })
      expect(report.columns[`messages.card`]!.sealedSkipped).toBeGreaterThanOrEqual(2)
      expect((await messageRow(tx, sealed)).card).toEqual(fixture.cards.sealed)
      expect((await messageRow(tx, ask)).card).toEqual(fixture.cards.sealedAsk)
    })
  })

  it(`a broken row is reported and left whole`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const broken = await s.app(`broken-${id8()}`, { package: CAT.fieldVisits, draft: CAT.brokenTemplate, grants: fixture.grants.dotted })
      const format4 = await s.version(broken, 1, CAT.formatFour)
      const card = await s.message(fixture.cards.brokenComponent)
      const good = await s.message(fixture.cards.plan)
      const report = await runA2ui10Migration(tx as Client, { apply: true, teamId: s.teamId })
      expect(report.ok).toBe(false)
      expect(report.columns[`apps.draft_package`]!.failed).toEqual([{ id: broken, reason: expect.stringContaining(`no component has the id root`) }])
      expect(report.columns[`apps.package`]!.heldBack).toBe(1)
      expect(report.columns[`apps.granted_tools`]!.heldBack).toBe(1)
      expect(report.columns[`app_versions.package`]!.failed).toEqual([{ id: format4, reason: `format version 4 has no upgrade` }])
      expect(report.columns[`messages.card`]!.failed).toEqual([{ id: card, reason: expect.stringContaining(`NoSuchComponent`) }])
      const app = await appRow(tx, broken)
      expect(app.package).toEqual(CAT.fieldVisits)
      expect(app.draft_package).toEqual(CAT.brokenTemplate)
      expect(app.granted_tools).toEqual(fixture.grants.dotted)
      expect((await messageRow(tx, card)).card).toEqual(fixture.cards.brokenComponent)
      expect((await messageRow(tx, good)).card.surface!.version).toBe(`1.0`)
    })
  })

  it(`a row changed after its read is read again, its change kept`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const moved = { ...CAT.fieldVisits, version: `9.9.9` }
      const id = await s.app(`race-${id8()}`, { package: CAT.fieldVisits, grants: fixture.grants.dotted })
      const message = await s.message(fixture.cards.plan)
      let raced = false
      const report = await runA2ui10Migration(tx as Client, {
        apply: true,
        teamId: s.teamId,
        afterRead: async (table) => {
          if (raced) return
          if (table === `apps`) {
            raced = true
            await tx.execute(sql`UPDATE apps SET package = ${JSON.stringify(moved)}::jsonb, name = 'moved' WHERE id = ${id}::uuid`)
          }
        },
      })
      expect(report.ok).toBe(true)
      const app = await appRow(tx, id)
      expect(app.package).toMatchObject({ formatVersion: 6, version: `9.9.9` })
      expect(app.name).toBe(`moved`)
      expect((await messageRow(tx, message)).card.surface!.version).toBe(`1.0`)
    })
  })

  it(`a row that keeps changing is reported, the last writer's value standing`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const id = await s.app(`busy-${id8()}`, { package: CAT.fieldVisits })
      let n = 0
      const report = await runA2ui10Migration(tx as Client, {
        apply: true,
        teamId: s.teamId,
        afterRead: async (table) => {
          if (table === `apps`) await tx.execute(sql`UPDATE apps SET package = ${JSON.stringify({ ...CAT.fieldVisits, version: `5.0.${(n += 1)}` })}::jsonb WHERE id = ${id}::uuid`)
        },
      })
      expect(report.ok).toBe(false)
      expect(report.columns[`apps.package`]!.conflicted).toEqual([id])
      expect((await appRow(tx, id)).package).toMatchObject({ formatVersion: 5, version: `5.0.${n}` })
    })
  })

  it(`a write beside the migrated columns is no conflict and survives`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const id = await s.app(`beside-${id8()}`, { package: CAT.fieldVisits })
      const report = await runA2ui10Migration(tx as Client, {
        apply: true,
        teamId: s.teamId,
        afterRead: async (table) => {
          if (table === `apps`) await tx.execute(sql`UPDATE apps SET automation_state = '{"nightly": {"checkedAt": "2026-11-07"}}'::jsonb WHERE id = ${id}::uuid`)
        },
      })
      expect(report.ok).toBe(true)
      const row = (await tx.execute<{ automation_state: unknown; package: { formatVersion: number } }>(sql`SELECT automation_state, package FROM apps WHERE id = ${id}::uuid`)).rows[0]!
      expect(row.package.formatVersion).toBe(6)
      expect(row.automation_state).toEqual({ nightly: { checkedAt: `2026-11-07` } })
    })
  })

  it(`a row Postgres refuses fails alone, the batch lands`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const good = await s.message(fixture.cards.plan)
      const bad = await s.message({ ...fixture.cards.plan, plan: `kept`, surface: { root: { id: `nul-node`, component: `Text`, props: { text: `\\u0000` } } } })
      await tx.execute(sql`UPDATE messages SET card = jsonb_set(card, '{surface,root,props,text}', to_jsonb('NUL'::text)) WHERE id = ${bad}::uuid`)
      const report = await runA2ui10Migration(
        new Proxy(tx as Client, {
          get(target, key, receiver) {
            if (key !== `transaction`) return Reflect.get(target, key, receiver)
            return (fn: (t: RealDbTx) => Promise<unknown>) =>
              target.transaction(async (inner) => {
                const refusing = new Proxy(inner, {
                  get(t, k, r) {
                    if (k !== `execute`) return Reflect.get(t, k, r)
                    return async (query: unknown) => {
                      const text = JSON.stringify(query)
                      if (text.includes(`NUL`) && text.includes(`UPDATE`)) throw new Error(`unsupported Unicode escape sequence`)
                      return t.execute(query as never)
                    }
                  },
                })
                return fn(refusing)
              })
          },
        }),
        { apply: true, batchSize: 500, teamId: s.teamId }
      )
      expect(report.columns[`messages.card`]!.failed).toEqual([{ id: bad, reason: `unsupported Unicode escape sequence` }])
      expect((await messageRow(tx, good)).card.surface!.version).toBe(`1.0`)
    })
  })

  it(`takes every package the platform ships, as stored rows`, async () => {
    const dir = (path: string, suffix: string) => readdirSync(join(ROOT, path)).filter((f) => f.endsWith(suffix)).map((f) => join(path, f))
    const shipped = [
      ...dir(`samples/apps`, `.app.json`),
      ...dir(`packages/exponential-ui/skills/exponential-apps/examples`, `.app.json`),
      ...dir(`packages/ui/exponential-ui/templates`, `.json`),
      `apps/runner/web/public/app/package.app.json`,
      `apps/runner/web/samples/album.app.json`,
      `apps/runner/ios/sample/app/package.app.json`,
      `apps/runner/android/app/src/main/assets/app/package.app.json`,
      `apps/desktop/crates/ui/src/app_screen.test-package.json`,
    ]
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const ids: string[] = []
      for (const path of shipped) {
        const file = JSON.parse(readFileSync(join(ROOT, path), `utf8`)) as { package?: unknown }
        const pkg = file.package ?? file
        ids.push(await s.app(`shipped-${id8()}`, { package: pkg, draft: pkg }))
      }
      const report = await runA2ui10Migration(tx as Client, { apply: true, teamId: s.teamId })
      expect(report.columns[`apps.package`]!.failed).toEqual([])
      for (const id of ids) {
        const app = await appRow(tx, id)
        expect(app.package!.formatVersion).toBe(6)
        expectPackageLoads(app.package)
        expectPackageLoads(app.draft_package)
      }
    })
  })
})

describe.skipIf(!ready.ok)(`stripUngrantableGrants (FEED-159) across the flag day`, () => {
  it(`keeps a grantable action in either spelling, drops the rest`, async () => {
    await inRollback(async (tx) => {
      const s = await seed(tx)
      const dotted = await s.app(`dotted-${id8()}`, { grants: [`exponential.issues.update`, `exponential.issues.delete`, `exponential.nope.nope`] })
      const flat = await s.app(`flat-${id8()}`, { grants: [`exponential_issues_update`, `exponential_issues_delete`] })
      await stripUngrantableGrants(tx as never)
      expect((await appRow(tx, dotted)).granted_tools).toEqual([`exponential.issues.update`])
      expect((await appRow(tx, flat)).granted_tools).toEqual([`exponential_issues_update`])
    })
  })
})
