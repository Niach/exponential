// @vitest-environment node
// #FEED-242 against the REAL migrated Postgres: an app's default access
// (`apps.default_access`, @exp/emcp access.json `memberPolicy`). At `none`
// a member without a row loses the app everywhere (apps.list, the apps
// shape's hidden ids, records NOT_FOUND, the broker's tools/list) while the
// creator, an owner and a member with an explicit row keep it; the MCP
// `exponential_apps_write {op: "access"}` is owners only and names members by
// user id, member id or email.
import { hashOAuthToken } from "@/lib/auth/oauth-token-hash"
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appAccess, apps, mcpGrants, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "./server"
import { FULL_ACCESS } from "./scope"

vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'apps' AND column_name = 'default_access') AS ok
`)
if (!ready.ok) console.info(`[app-default-access.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp_/apps`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST
const { registerNounTools } = await import(`./nouns`)
const { caller } = await import(`./tools`)
const { appShapeRules, invalidateAppShapeRules } = await import(`@/lib/apps/records-shape`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], clients: [] as string[] }

async function user(): Promise<McpUser> {
  const id = `default-access-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@access.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email: `${id}@access.test`, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

const asUser = (u: McpUser) => caller(u as never, new Request(`http://localhost/api/trpc`))

/** `exponential_apps_write` as this user sees it (a full-access key). */
function appsWrite(u: McpUser) {
  let handler: ((args: Record<string, unknown>, extra?: unknown) => Promise<ToolResult>) | undefined
  const server = {
    registerTool: (name: string, _def: unknown, h: typeof handler) => {
      if (name === `exponential_apps_write`) handler = h
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), FULL_ACCESS)
  return (args: Record<string, unknown>) => handler!(args, {})
}

const EXPOSES = [
  {
    name: `list_entries`,
    title: `List entries`,
    description: `The entries.`,
    input: { type: `object`, properties: {} },
    access: `read`,
    run: { records: { op: `list`, kind: `entry` } },
  },
]

/** owner + creator (a member who made the app) + two plain members. */
async function world() {
  const owner = await user()
  const creator = await user()
  const mate = await user()
  const other = await user()
  const [row] = await db.insert(teams).values({ name: `Access`, slug: `access-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner.id, role: `owner` },
    { teamId, userId: creator.id, role: `member` },
    { teamId, userId: mate.id, role: `member` },
    { teamId, userId: other.id, role: `member` },
  ])
  const [app] = await db
    .insert(apps)
    .values({ teamId, slug: `ledger`, name: `Ledger`, package: { records: { entry: { schema: { type: `object` } } }, exposes: EXPOSES } as never, headVersion: 1, createdBy: creator.id })
    .returning({ id: apps.id })
  return { teamId, appId: app!.id, owner, creator, mate, other }
}

async function brokerToken(userId: string, teamId: string): Promise<string> {
  const clientId = `client-${id8()}`
  const token = `tok-${crypto.randomUUID()}`
  await db.execute(sql`
    INSERT INTO oauth2_clients (id, name, client_id, redirect_uris, token_endpoint_auth_method, disabled, created_at, updated_at)
    VALUES (${clientId}, 'Test client', ${clientId}, ARRAY['http://localhost/cb'], 'none', false, now(), now())
  `)
  await db.execute(sql`
    INSERT INTO oauth2_access_tokens (id, token, client_id, user_id, scopes, expires_at, created_at)
    VALUES (${`at-${id8()}`}, ${hashOAuthToken(token)}, ${clientId}, ${userId}, ARRAY['openid'], now() + interval '1 hour', now())
  `)
  made.clients.push(clientId)
  await db.insert(mcpGrants).values({ userId, clientId, kind: `apps`, teamIds: [teamId], allApps: true, access: `full` })
  return token
}

async function brokerTools(token: string): Promise<string[]> {
  const response = await post({
    request: new Request(`http://localhost/api/mcp/apps`, {
      method: `POST`,
      headers: { authorization: `Bearer ${token}`, "content-type": `application/json`, accept: `application/json, text/event-stream` },
      body: JSON.stringify({ jsonrpc: `2.0`, id: 1, method: `tools/list`, params: {} }),
    }),
  })
  const body = (await response.json()) as { result?: { tools?: { name: string }[] } }
  return (body.result?.tools ?? []).map((t) => t.name)
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.clients.length) {
    await db.execute(sql`DELETE FROM mcp_calls WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
    await db.execute(sql`DELETE FROM oauth2_clients WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
  }
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`app default access (real db, #FEED-242)`, () => {
  it(`default none hides the app from members without a row; creator, owners and an explicit row keep it`, async () => {
    const w = await world()
    const owner = asUser(w.owner)
    await expect(asUser(w.mate).appAccess.setDefault({ appId: w.appId, level: `none` })).rejects.toThrow(/owner/)
    await owner.appAccess.setDefault({ appId: w.appId, level: `none` })
    await owner.appAccess.set({ appId: w.appId, userId: w.other.id, level: `read` })
    expect(await owner.appAccess.levels({ appId: w.appId })).toEqual({ default: `none`, levels: { [w.mate.id]: `none`, [w.other.id]: `read` } })

    // apps.list
    const listed = async (u: McpUser) => (await asUser(u).apps.list({ teamId: w.teamId })).apps.map((a) => a.id)
    expect(await listed(w.mate)).toEqual([])
    for (const u of [w.owner, w.creator, w.other]) expect(await listed(u)).toEqual([w.appId])

    // The shapes' hidden ids.
    invalidateAppShapeRules()
    expect((await appShapeRules(w.mate.id, [w.teamId])).hiddenAppIds).toEqual([w.appId])
    for (const u of [w.owner, w.creator, w.other]) expect((await appShapeRules(u.id, [w.teamId])).hiddenAppIds).toEqual([])

    // Records + get: NOT_FOUND for the hidden member.
    await expect(asUser(w.mate).apps.records.list({ teamId: w.teamId, slug: `ledger` })).rejects.toThrow(/not found/i)
    await expect(asUser(w.mate).apps.get({ teamId: w.teamId, slug: `ledger` })).rejects.toThrow(/not found/i)
    expect((await asUser(w.mate).appAccess.mine({ appId: w.appId })).level).toBe(`none`)
    expect((await asUser(w.other).appAccess.mine({ appId: w.appId })).level).toBe(`read`)
    for (const u of [w.owner, w.creator]) {
      expect((await asUser(u).appAccess.mine({ appId: w.appId })).level).toBe(`write`)
      await asUser(u).apps.records.list({ teamId: w.teamId, slug: `ledger` })
    }

    // The broker.
    expect(await brokerTools(await brokerToken(w.mate.id, w.teamId))).toEqual([])
    expect(await brokerTools(await brokerToken(w.other.id, w.teamId))).toEqual([`ledger__list_entries`])
    expect(await brokerTools(await brokerToken(w.creator.id, w.teamId))).toEqual([`ledger__list_entries`])

    // Back to write: the member sees it again.
    await owner.appAccess.setDefault({ appId: w.appId, level: `write` })
    expect(await listed(w.mate)).toEqual([w.appId])
    expect(await brokerTools(await brokerToken(w.mate.id, w.teamId))).toEqual([`ledger__list_entries`])
  })

  it(`MCP access op: owners only, members by user id, member id or email`, async () => {
    const w = await world()
    const refused = await appsWrite(w.mate)({ op: `access`, slug: `ledger`, teamId: w.teamId, default: `none` })
    expect(refused.isError).toBe(true)
    expect((await db.select({ d: apps.defaultAccess }).from(apps).where(eq(apps.id, w.appId)))[0]!.d).toBe(`write`)

    const [creatorMember] = await db.select({ id: teamMembers.id }).from(teamMembers).where(eq(teamMembers.userId, w.creator.id))
    const result = await appsWrite(w.owner)({
      op: `access`,
      slug: `ledger`,
      teamId: w.teamId,
      default: `none`,
      members: { [w.mate.id]: `read`, [creatorMember!.id]: `read`, [w.other.email.toUpperCase()]: `write` },
    })
    expect(result.isError, result.content[0]?.text).toBeFalsy()
    const body = JSON.parse(result.content[0]!.text!) as { access: { default: string; members: Record<string, string> } }
    expect(body.access).toEqual({ default: `none`, members: { [w.mate.id]: `read`, [w.creator.id]: `read` } })
    // `other` at write while the default is none = an explicit row.
    const rows = await db.select({ userId: appAccess.userId, level: appAccess.level }).from(appAccess).where(eq(appAccess.appId, w.appId))
    expect(Object.fromEntries(rows.map((r) => [r.userId, r.level]))).toEqual({ [w.mate.id]: `read`, [w.creator.id]: `read`, [w.other.id]: `write` })

    const unknown = await appsWrite(w.owner)({ op: `access`, slug: `ledger`, teamId: w.teamId, members: { "nobody@nowhere.test": `none` } })
    expect(unknown.isError).toBe(true)
  })
})
