// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { events, repositories, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "./server"
import { FULL_ACCESS, type McpAccess } from "./scope"

// #FEED-303: `exponential_repositories_write {op: "configure_ci"}` against
// the REAL migrated Postgres, through the noun router into the same tRPC
// mutation the repository's CI settings screen uses (`ci.setRepoConfig`):
// the member who shared the repository or a team owner switches Exponential's
// own CI, any other member and another team's owner are refused, and every
// change leaves a `repository.ci_configured` event in the team's stream.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'repositories' AND column_name = 'ci_config') AS ok
`)
if (!ready.ok) console.info(`[repositories-configure-ci.real-db] skipped: ${ready.reason}`)

const { registerNounTools } = await import(`./nouns`)
const { REPOSITORY_CI_CONFIGURED_EVENT } = await import(`./tools`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<McpUser> {
  const id = `ci-mcp-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@ci.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email: `${id}@ci.test`, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(owner: McpUser, ...members: McpUser[]): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `CI`, slug: `ci-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner.id, role: `owner` },
    ...members.map((m) => ({ teamId, userId: m.id, role: `member` as const })),
  ])
  return teamId
}

async function repo(teamId: string, sharedBy?: McpUser): Promise<string> {
  const [row] = await db
    .insert(repositories)
    .values({ teamId, fullName: `org/ci-${id8()}`, sharedByUserId: sharedBy?.id ?? null })
    .returning({ id: repositories.id })
  return row!.id
}

/** `exponential_repositories_write` as this user sees it. */
function writeTool(u: McpUser, access: McpAccess = FULL_ACCESS) {
  let handler: ((args: Record<string, unknown>, extra?: unknown) => Promise<ToolResult>) | undefined
  const server = {
    registerTool: (name: string, _def: unknown, h: typeof handler) => {
      if (name === `exponential_repositories_write`) handler = h
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), access)
  return (args: Record<string, unknown>) => handler!(args, {})
}

const payload = (result: ToolResult) => {
  expect(result.isError, result.content[0]?.text).toBeFalsy()
  return JSON.parse(result.content[0]!.text!) as Record<string, unknown>
}

const configOf = async (repositoryId: string) =>
  (await db.select({ ciConfig: repositories.ciConfig }).from(repositories).where(eq(repositories.id, repositoryId)))[0]!
    .ciConfig

const logged = (teamId: string) =>
  db
    .select({ payload: events.payload, userId: events.userId, emittedBy: events.emittedBy })
    .from(events)
    .where(and(eq(events.teamId, teamId), eq(events.name, REPOSITORY_CI_CONFIGURED_EVENT)))
    .orderBy(events.createdAt)

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe.skipIf(!ready.ok)(`MCP repositories configure_ci (real db)`, () => {
  it(`a team owner switches CI on and off, the other settings stay, each call is logged`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const repositoryId = await repo(teamId)
    await db.update(repositories).set({ ciConfig: { requiredChecks: [`gate`] } }).where(eq(repositories.id, repositoryId))
    const call = writeTool(owner)

    const on = payload(await call({ op: `configure_ci`, repositoryId, enabled: true, actionsRunners: true }))
    expect(on).toMatchObject({ repositoryId, config: { enabled: true, actionsRunners: true, requiredChecks: [`gate`] } })
    expect(await configOf(repositoryId)).toEqual({ enabled: true, actionsRunners: true, requiredChecks: [`gate`] })

    const off = payload(await call({ op: `configure_ci`, repositoryId, enabled: false }))
    expect(off.config).toEqual({ enabled: false, actionsRunners: true, requiredChecks: [`gate`] })
    expect(await configOf(repositoryId)).toEqual({ enabled: false, actionsRunners: true, requiredChecks: [`gate`] })

    const rows = await logged(teamId)
    expect(rows).toHaveLength(2)
    expect(rows[0]).toMatchObject({
      emittedBy: `user`,
      userId: owner.id,
      payload: { repositoryId, fullName: on.fullName, enabled: true, actionsRunners: true, userId: owner.id },
    })
    expect(rows[1]!.payload).toMatchObject({ repositoryId, enabled: false, userId: owner.id })
    expect(rows[1]!.payload).not.toHaveProperty(`actionsRunners`)
  })

  it(`the member who shared the repository may switch it`, async () => {
    const owner = await user()
    const sharer = await user()
    const teamId = await team(owner, sharer)
    const repositoryId = await repo(teamId, sharer)

    payload(await writeTool(sharer)({ op: `configure_ci`, repositoryId, enabled: true }))
    expect(await configOf(repositoryId)).toEqual({ enabled: true })
    expect(await logged(teamId)).toHaveLength(1)
  })

  it(`refuses a member who may not manage the repository; nothing is written or logged`, async () => {
    const owner = await user()
    const sharer = await user()
    const mate = await user()
    const teamId = await team(owner, sharer, mate)
    const repositoryId = await repo(teamId, sharer)

    const result = await writeTool(mate)({ op: `configure_ci`, repositoryId, enabled: true })
    expect(result.isError).toBe(true)
    expect(result.content[0]!.text).toContain(`Only the member who shared this repository or a team owner`)
    expect(await configOf(repositoryId)).toEqual({})
    expect(await logged(teamId)).toHaveLength(0)
  })

  it(`refuses a repository of another team, an owner there included`, async () => {
    const owner = await user()
    const otherOwner = await user()
    const teamId = await team(owner)
    await team(otherOwner)
    const repositoryId = await repo(teamId)

    const result = await writeTool(otherOwner)({ op: `configure_ci`, repositoryId, enabled: true })
    expect(result.isError).toBe(true)
    expect(await configOf(repositoryId)).toEqual({})
    expect(await logged(teamId)).toHaveLength(0)
  })

  it(`refuses a credential without the full team grant`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const repositoryId = await repo(teamId)
    // A board-level grant: the team is visible, never fully granted.
    const scoped: McpAccess = { full: false, fullTeamIds: new Set(), grantedBoardIds: new Set(), visibleTeamIds: new Set([teamId]) }

    const result = await writeTool(owner, scoped)({ op: `configure_ci`, repositoryId, enabled: true })
    expect(result.isError).toBe(true)
    expect(await configOf(repositoryId)).toEqual({})
    expect(await logged(teamId)).toHaveLength(0)
  })

  it(`validates the arguments (enabled is required, no unknown key)`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const repositoryId = await repo(teamId)
    const call = writeTool(owner)

    expect((await call({ op: `configure_ci`, repositoryId })).isError).toBe(true)
    expect((await call({ op: `configure_ci`, repositoryId, enabled: true, requiredChecks: [] })).isError).toBe(true)
    expect(await configOf(repositoryId)).toEqual({})
  })
})
