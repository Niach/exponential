// @vitest-environment node
// #VAPP-184: the revocation flows against the REAL migrated Postgres: a
// revoked client answers 401 on its next call; an owner revokes a member's
// client, a member cannot; a member leaving the team loses their apps grant,
// policy rows and orphaned tokens (the builtin grant stays); unlinking an
// upstream revokes at the provider (RFC 7009) and deletes the credential.
import { afterAll, afterEach, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { appAccess, apps, mcpCredentials, mcpGrants, mcpServers, teamMembers, teams } from "@/db/schema"
import { oauthAccessTokens } from "@/db/auth-schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'app_access') AS ok
`)
if (!ready.ok) console.info(`[revoke.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp_/apps`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST
const { caller } = await import(`./tools`)
const { unlinkUpstream } = await import(`./revoke`)
const { storeOauthTokens } = await import(`./upstream/credentials`)
const { fakeProvider, MCP_URL, AS_ISSUER } = await import(`./upstream/test-provider`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], clients: [] as string[] }

async function user(): Promise<string> {
  const id = `revoke-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@revoke.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function client(userId: string, clientId = `client-${id8()}`): Promise<{ clientId: string; token: string }> {
  const token = `tok-${crypto.randomUUID()}`
  if (!made.clients.includes(clientId)) {
    await db.execute(sql`
      INSERT INTO oauth_applications (id, name, client_id, redirect_urls, type, disabled, created_at, updated_at)
      VALUES (${clientId}, 'Claude Code', ${clientId}, 'http://localhost/cb', 'public', false, now(), now())
    `)
    made.clients.push(clientId)
  }
  await db.execute(sql`
    INSERT INTO oauth_access_tokens (id, access_token, refresh_token, access_token_expires_at, refresh_token_expires_at, client_id, user_id, scopes, created_at, updated_at)
    VALUES (${`at-${id8()}`}, ${token}, ${`rt-${crypto.randomUUID()}`}, now() + interval '1 hour', now() + interval '1 day', ${clientId}, ${userId}, 'openid', now(), now())
  `)
  return { clientId, token }
}

async function world() {
  const owner = await user()
  const mate = await user()
  const [row] = await db.insert(teams).values({ name: `Revoke`, slug: `revoke-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  const [, mateRow] = await db
    .insert(teamMembers)
    .values([
      { teamId, userId: owner, role: `owner` },
      { teamId, userId: mate, role: `member` },
    ])
    .returning({ id: teamMembers.id })
  const [app] = await db
    .insert(apps)
    .values({
      teamId,
      slug: `notes`,
      name: `Notes`,
      headVersion: 1,
      package: {
        records: { note: { schema: { type: `object` } } },
        exposes: [{ name: `list`, title: `List`, description: `Notes.`, input: { type: `object` }, access: `read`, run: { records: { op: `list`, kind: `note` } } }],
      } as never,
    })
    .returning({ id: apps.id })
  return { owner, mate, mateMemberId: mateRow!.id, teamId, appId: app!.id }
}

const appsGrant = (userId: string, clientId: string, teamId: string) =>
  db.insert(mcpGrants).values({ userId, clientId, kind: `apps`, teamIds: [teamId], allApps: true, access: `full` })

async function list(token: string) {
  return post({
    request: new Request(`http://localhost/api/mcp/apps`, {
      method: `POST`,
      headers: { authorization: `Bearer ${token}`, "content-type": `application/json`, accept: `application/json, text/event-stream` },
      body: JSON.stringify({ jsonrpc: `2.0`, id: 1, method: `tools/list`, params: {} }),
    }),
  })
}

async function asUser(id: string) {
  const [row] = await db.execute(sql`SELECT id, name, email, image, email_verified AS "emailVerified", is_admin AS "isAdmin" FROM users WHERE id = ${id}`).then((r) => r.rows)
  return caller(row as never, new Request(`http://localhost/api/trpc`))
}

afterEach(() => vi.unstubAllGlobals())
afterAll(async () => {
  if (!ready.ok) return
  if (made.clients.length) await db.execute(sql`DELETE FROM oauth_applications WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`MCP revocation (real db, VAPP-184)`, () => {
  it(`a member revokes their own client: grants and tokens go, the next call is 401`, async () => {
    const w = await world()
    const { clientId, token } = await client(w.mate)
    await appsGrant(w.mate, clientId, w.teamId)
    await db.insert(mcpGrants).values({ userId: w.mate, clientId, kind: `builtin`, teamIds: [w.teamId] })
    expect((await list(token)).status).toBe(200)

    const mate = await asUser(w.mate)
    const mine = await mate.mcpClients.list({ teamId: w.teamId, scope: `mine` })
    expect(mine.clients).toEqual([expect.objectContaining({ clientId, userId: w.mate, kinds: [`apps`, `builtin`], appsAccess: `full`, name: `Claude Code` })])
    await expect(mate.mcpClients.list({ teamId: w.teamId, scope: `everyone` })).rejects.toThrow(/owner/)

    await mate.mcpClients.revoke({ clientId, teamId: w.teamId })
    expect(await db.select().from(mcpGrants).where(eq(mcpGrants.clientId, clientId))).toHaveLength(0)
    expect(await db.select().from(oauthAccessTokens).where(eq(oauthAccessTokens.clientId, clientId))).toHaveLength(0)
    expect((await list(token)).status).toBe(401)
  })

  it(`an owner sees and revokes a member's client; a member cannot revoke someone else's`, async () => {
    const w = await world()
    const { clientId, token } = await client(w.owner)
    await appsGrant(w.owner, clientId, w.teamId)
    const theirs = await client(w.mate)
    await appsGrant(w.mate, theirs.clientId, w.teamId)

    const mate = await asUser(w.mate)
    await expect(mate.mcpClients.revoke({ clientId, teamId: w.teamId, userId: w.owner })).rejects.toThrow(/owner/)
    expect((await list(token)).status).toBe(200)

    const owner = await asUser(w.owner)
    const everyone = await owner.mcpClients.list({ teamId: w.teamId, scope: `everyone` })
    expect(everyone.clients.map((c) => c.userId).sort()).toEqual([w.mate, w.owner].sort())
    await owner.mcpClients.revoke({ clientId: theirs.clientId, teamId: w.teamId, userId: w.mate })
    expect((await list(theirs.token)).status).toBe(401)
  })

  it(`a member leaving the team loses the apps grant, policy rows and orphaned tokens; the builtin grant stays`, async () => {
    const w = await world()
    const appsOnly = await client(w.mate)
    await appsGrant(w.mate, appsOnly.clientId, w.teamId)
    const both = await client(w.mate)
    await appsGrant(w.mate, both.clientId, w.teamId)
    await db.insert(mcpGrants).values({ userId: w.mate, clientId: both.clientId, kind: `builtin`, allTeams: true })
    await db.insert(appAccess).values({ appId: w.appId, teamId: w.teamId, userId: w.mate, level: `read` })

    await (await asUser(w.owner)).teamMembers.remove({ memberId: w.mateMemberId })

    expect(await db.select().from(mcpGrants).where(and(eq(mcpGrants.userId, w.mate), eq(mcpGrants.kind, `apps`)))).toHaveLength(0)
    expect(await db.select().from(appAccess).where(eq(appAccess.userId, w.mate))).toHaveLength(0)
    expect(await db.select().from(oauthAccessTokens).where(eq(oauthAccessTokens.clientId, appsOnly.clientId))).toHaveLength(0)
    // The other client keeps its builtin grant and token (it still reaches the member's other teams).
    expect(await db.select().from(mcpGrants).where(eq(mcpGrants.clientId, both.clientId))).toHaveLength(1)
    expect(await db.select().from(oauthAccessTokens).where(eq(oauthAccessTokens.clientId, both.clientId))).toHaveLength(1)
  })

  it(`unlinking an upstream revokes the refresh token at the provider and deletes the credential`, async () => {
    const w = await world()
    const provider = fakeProvider()
    vi.stubGlobal(`fetch`, provider.fetch)
    const [server] = await db
      .insert(mcpServers)
      .values({ teamId: w.teamId, name: `m5`, transport: `http`, url: MCP_URL, auth: `oauth` })
      .returning()
    await storeOauthTokens(db, {
      serverId: server!.id,
      userId: w.mate,
      teamId: w.teamId,
      issuer: AS_ISSUER,
      clientId: `cimd-client`,
      tokenEndpoint: `${AS_ISSUER}/token`,
      tokens: { accessToken: `at-1`, refreshToken: `rt-1`, tokenType: `Bearer`, expiresAt: new Date(Date.now() + 3_600_000) } as never,
    })

    expect(await unlinkUpstream(db, server!, w.mate)).toEqual({ revoked: true })
    expect(provider.revocationForms.map((f) => [f.get(`token`), f.get(`token_type_hint`), f.get(`client_id`)])).toEqual([[`rt-1`, `refresh_token`, `cimd-client`]])
    expect(await db.select().from(mcpCredentials).where(eq(mcpCredentials.serverId, server!.id))).toHaveLength(0)

    // A provider without a revocation endpoint: the row still goes.
    vi.stubGlobal(`fetch`, fakeProvider({ metadata: { revocation_endpoint: undefined } }).fetch)
    await storeOauthTokens(db, {
      serverId: server!.id,
      userId: w.mate,
      teamId: w.teamId,
      issuer: AS_ISSUER,
      clientId: `cimd-client`,
      tokenEndpoint: `${AS_ISSUER}/token`,
      tokens: { accessToken: `at-2`, refreshToken: null, tokenType: `Bearer`, expiresAt: null } as never,
    })
    expect(await unlinkUpstream(db, server!, w.mate)).toEqual({ revoked: false })
    expect(await db.select().from(mcpCredentials).where(eq(mcpCredentials.serverId, server!.id))).toHaveLength(0)
  })
})
