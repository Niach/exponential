// @vitest-environment node
// #NEW-144: `/api/mcp` against the REAL migrated Postgres, through the route
// and the gate, with the SDK's own client in each era: 2025-06-18 and
// 2025-11-25 (`initialize`) and 2026-07-28 (`server/discover`, the
// per-request envelope). Every era lists the same 22 tools and runs one.
import { hashOAuthToken } from "@/lib/auth/oauth-token-hash"
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client"
import { db } from "@/db/connection"
import { mcpGrants, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'mcp_grants') AS ok`)
if (!ready.ok) console.info(`[protocol.real-db] skipped: ${ready.reason}`)

const { Route } = await import(`@/routes/api/mcp`)
const post = (Route.options.server as { handlers: { POST: (ctx: { request: Request }) => Promise<Response> } }).handlers.POST
const { SERVER_EXTENSIONS } = await import(`./extensions`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[], clients: [] as string[] }

async function world(): Promise<{ token: string; teamId: string }> {
  const userId = `protocol-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@protocol.test`}, true, false, now(), now())
  `)
  made.users.push(userId)
  const [team] = await db.insert(teams).values({ name: `Protocol`, slug: `protocol-${id8()}` }).returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values({ teamId: team!.id, userId, role: `owner` })
  const clientId = `client-${id8()}`
  await db.execute(sql`
    INSERT INTO oauth2_clients (id, name, client_id, redirect_uris, token_endpoint_auth_method, disabled, created_at, updated_at)
    VALUES (${clientId}, 'Claude Code', ${clientId}, ARRAY['http://localhost/cb'], 'none', false, now(), now())
  `)
  made.clients.push(clientId)
  const token = `tok-${crypto.randomUUID()}`
  await db.execute(sql`
    INSERT INTO oauth2_access_tokens (id, token, client_id, user_id, scopes, expires_at, created_at)
    VALUES (${`at-${id8()}`}, ${hashOAuthToken(token)}, ${clientId}, ${userId}, ARRAY['openid'], now() + interval '1 hour', now())
  `)
  await db.insert(mcpGrants).values({ userId, clientId, kind: `builtin`, teamIds: [team!.id] })
  return { token, teamId: team!.id }
}

/** The SDK client over the route, in process. */
async function connect(token: string, era: `2025-06-18` | `2025-11-25` | `2026-07-28`) {
  const methods: string[] = []
  const transport = new StreamableHTTPClientTransport(new URL(`http://localhost/api/mcp`), {
    requestInit: { headers: { authorization: `Bearer ${token}` } },
    fetch: async (url, init) => {
      const request = new Request(url, init)
      const body = (await request.clone().json().catch(() => null)) as { method?: string } | null
      if (body?.method) methods.push(body.method)
      return post({ request })
    },
  })
  const client = new Client(
    { name: `protocol-test`, version: `1` },
    era === `2026-07-28` ? { versionNegotiation: { mode: { pin: era } } } : { supportedProtocolVersions: [era] }
  )
  await client.connect(transport)
  return { client, methods }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.clients.length) await db.execute(sql`DELETE FROM oauth2_clients WHERE client_id IN ${sql.raw(`('${made.clients.join(`','`)}')`)}`)
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`/api/mcp in every era (real db, #NEW-144)`, () => {
  const listed: Record<string, string[]> = {}
  for (const era of [`2025-06-18`, `2025-11-25`, `2026-07-28`] as const) {
    it(`${era}: connects, lists the tools and runs one`, async () => {
      const { token, teamId } = await world()
      const { client, methods } = await connect(token, era)
      expect(methods[0]).toBe(era === `2026-07-28` ? `server/discover` : `initialize`)
      expect(client.getServerCapabilities()?.extensions).toEqual(SERVER_EXTENSIONS)
      const { tools } = await client.listTools()
      listed[era] = tools.map((tool) => tool.name).sort()
      expect(listed[era]).toContain(`exponential_teams`)
      const result = (await client.callTool({ name: `exponential_teams`, arguments: { op: `list` } })) as { isError?: boolean; content: { text: string }[] }
      expect(result.isError).toBeFalsy()
      expect(result.content[0]!.text).toContain(teamId)
      await client.close()
    })
  }
  it(`every era lists the same tools`, () => {
    expect(listed[`2025-06-18`]).toEqual(listed[`2026-07-28`])
    expect(listed[`2025-11-25`]).toEqual(listed[`2026-07-28`])
  })
})
