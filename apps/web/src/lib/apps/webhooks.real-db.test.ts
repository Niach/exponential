// @vitest-environment node
// #VAPP-237 (eApps A3) against the REAL migrated Postgres: an owner mints a
// hook's secret (vault-sealed, answered once, rotatable); a signed delivery
// runs the published package's webhook automation with the body as
// `${event.body.*}`; a bad signature, a stale timestamp and a replay are
// refused and audited; a member cannot mint; the per-hook cap answers 429.
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { agents, appHooks, appRecords, apps, mcpCalls, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/integrations/notifications`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/integrations/notifications")>()
  return { ...real, notifyAutomationPaused: async () => {} }
})
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'app_hooks') AS ok
`)
if (!ready.ok) console.info(`[webhooks.real-db] skipped: ${ready.reason}`)

const { handleWebhookDelivery, hookRows, rotateHookSecret, pruneHookNonces } = await import(`@/lib/apps/webhooks`)
const { webhookSignature, WEBHOOK } = await import(`@exp/emcp/webhooks`)
const { migrateAppAutomations } = await import(`@/lib/apps/automations-migrate`)
const { triggerAutomationName } = await import(`@/lib/agents/app-triggers`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const saved: Record<string, string | undefined> = {}

beforeAll(() => {
  saved.VAULT_KEY = process.env.VAULT_KEY
  process.env.VAULT_KEY = `k1:${Buffer.alloc(32, 9).toString(`base64`)}`
})

async function user(): Promise<string> {
  const id = `hooks-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@hooks.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

const PACKAGE = {
  records: { payment: { schema: { type: `object`, properties: { customer: { type: `string` }, amount: { type: `number` } } } } },
  automations: [
    { name: `stripe_paid`, on: { webhook: { name: `stripe` } }, run: { records: { op: `upsert`, kind: `payment`, data: { customer: `\${event.body.data.customer}`, amount: `\${event.body.data.amount}` } } } },
  ],
}

async function world() {
  const owner = await user()
  const member = await user()
  const [row] = await db.insert(teams).values({ name: `Hooks`, slug: `hooks-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: member, role: `member` },
  ])
  const [app] = await db
    .insert(apps)
    .values({ teamId, slug: `billing`, name: `Billing`, package: PACKAGE as never, headVersion: 1, publishedVersion: 1, publishedBy: owner, createdBy: owner })
    .returning({ id: apps.id })
  // #EXP-1829: the boot pass moves the automation onto the app agent.
  await migrateAppAutomations()
  const [agent] = await db.select({ triggers: agents.triggers }).from(agents).innerJoin(apps, eq(apps.agentId, agents.id)).where(eq(apps.id, app!.id))
  const cid = `automation:${triggerAutomationName(agent!.triggers[0]!.id)}`
  return { owner, member, teamId, cid, app: { id: app!.id, teamId, package: PACKAGE } }
}

const NOW = new Date(`2026-11-03T12:00:00.000Z`)
const T = Math.floor(NOW.getTime() / 1000)

function deliver(appId: string, body: string, header: string | null, now: Date = NOW) {
  const headers: Record<string, string> = { "content-type": `application/json` }
  if (header) headers[WEBHOOK.header] = header
  return handleWebhookDelivery(new Request(`http://x.test/api/apps/hooks/${appId}/stripe`, { method: `POST`, headers, body }), appId, `stripe`, now)
}

const payments = (appId: string) => db.select({ data: appRecords.data, authorId: appRecords.authorId }).from(appRecords).where(and(eq(appRecords.appId, appId), eq(appRecords.kind, `payment`)))
const calls = (appId: string) => db.select().from(mcpCalls).where(eq(mcpCalls.appId, appId))

afterAll(async () => {
  if (saved.VAULT_KEY === undefined) delete process.env.VAULT_KEY
  else process.env.VAULT_KEY = saved.VAULT_KEY
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM mcp_calls WHERE team_id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`app webhooks (real db)`, () => {
  it(`an owner mints a sealed secret; a signed delivery runs the automation once; refusals are audited`, async () => {
    const w = await world()
    expect(await hookRows(w.app)).toEqual([{ name: `stripe`, url: expect.stringContaining(`/api/apps/hooks/${w.app.id}/stripe`), hasSecret: false, rotatedAt: null }])
    // No secret yet: the hook does not exist.
    expect((await deliver(w.app.id, `{}`, null)).status).toBe(404)
    await expect(rotateHookSecret(w.member, w.app, `stripe`)).rejects.toThrow(/owner/)
    await expect(rotateHookSecret(w.owner, w.app, `github`)).rejects.toThrow(/no webhook/)
    const { secret } = await rotateHookSecret(w.owner, w.app, `stripe`)
    expect(secret.startsWith(WEBHOOK.secretPrefix)).toBe(true)
    const [stored] = await db.select().from(appHooks).where(eq(appHooks.appId, w.app.id))
    expect(stored?.secretCiphertext.startsWith(`ev1.k1.`)).toBe(true)
    expect(stored?.secretCiphertext).not.toContain(secret)
    expect((await hookRows(w.app))[0]?.hasSecret).toBe(true)

    const body = JSON.stringify({ data: { customer: `cus_1`, amount: 42 } })
    const signed = webhookSignature(secret, T, body)
    const ok = await deliver(w.app.id, body, signed)
    expect(ok.status).toBe(202)
    expect(await ok.json()).toEqual({ ok: true, runs: [{ name: w.cid.slice(`automation:`.length), status: `ok` }] })
    const [payment] = await payments(w.app.id)
    expect(payment).toMatchObject({ data: { customer: `cus_1`, amount: 42 }, authorId: w.owner })

    expect((await deliver(w.app.id, body, signed)).status).toBe(409)
    expect((await deliver(w.app.id, body, webhookSignature(`whsec_wrong`, T, body))).status).toBe(401)
    expect((await deliver(w.app.id, body, signed, new Date(NOW.getTime() + 301_000))).status).toBe(401)
    expect((await deliver(w.app.id, body, null)).status).toBe(401)
    expect((await deliver(w.app.id, `x`.repeat(WEBHOOK.bodyMax + 1), signed)).status).toBe(413)
    expect(await payments(w.app.id)).toHaveLength(1)
    const errors = (await calls(w.app.id)).map((c) => `${c.status}:${c.error ?? ``}`).sort()
    expect(errors).toEqual([`denied:badSignature`, `denied:missingSignature`, `denied:replayed`, `denied:stale`, `denied:tooLarge`, `ok:`].sort())

    // Rotation: the old secret stops at once.
    const rotated = await rotateHookSecret(w.owner, w.app, `stripe`)
    expect(rotated.secret).not.toBe(secret)
    const fresh = JSON.stringify({ data: { customer: `cus_2`, amount: 1 } })
    expect((await deliver(w.app.id, fresh, webhookSignature(secret, T, fresh))).status).toBe(401)
    expect((await deliver(w.app.id, fresh, webhookSignature(rotated.secret, T, fresh))).status).toBe(202)
    expect(await pruneHookNonces(new Date(NOW.getTime() + 3 * WEBHOOK.toleranceSeconds * 1000))).toBeGreaterThanOrEqual(1)
  })

  it(`over the per-hook cap answers 429 and audits nothing more`, async () => {
    const w = await world()
    await rotateHookSecret(w.owner, w.app, `stripe`)
    const rows = Array.from({ length: WEBHOOK.callsPerMinute }, () => ({
      teamId: w.teamId,
      userId: w.owner,
      clientId: w.cid,
      appId: w.app.id,
      tool: `records.upsert:payment`,
      status: `ok`,
      latencyMs: 1,
      argsHash: `x`,
      createdAt: new Date(),
    }))
    await db.insert(mcpCalls).values(rows as never)
    const res = await deliver(w.app.id, `{}`, null, new Date())
    expect(res.status).toBe(429)
    expect(await calls(w.app.id)).toHaveLength(WEBHOOK.callsPerMinute)
  })
})
