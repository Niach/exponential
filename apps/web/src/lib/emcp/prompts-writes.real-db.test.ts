// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { prompts, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "./server"
import { FULL_ACCESS } from "./scope"

// Saved-prompt writes over MCP (`exponential_team_config` prompts_create /
// prompts_update / prompts_delete) against the REAL migrated Postgres,
// through the noun router (its strict op schema) into the same tRPC router
// Settings → Prompts uses: a team owner writes, a member or an outsider is
// refused, and an owner of ANOTHER team cannot touch this team's prompt by id.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'prompts') AS ok
`)
if (!ready.ok) console.info(`[prompts-writes.real-db] skipped: ${ready.reason}`)

const { registerNounTools } = await import(`./nouns`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<McpUser> {
  const id = `prompt-mcp-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@prompts.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email: `${id}@prompts.test`, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(owner: McpUser, member?: McpUser): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Prompts`, slug: `prompts-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner.id, role: `owner` },
    ...(member ? [{ teamId, userId: member.id, role: `member` as const }] : []),
  ])
  return teamId
}

/** `exponential_team_config` as this user sees it (a full-access key). */
function teamConfig(u: McpUser) {
  let handler: ((args: Record<string, unknown>, extra?: unknown) => Promise<ToolResult>) | undefined
  const server = {
    registerTool: (name: string, _def: unknown, h: typeof handler) => {
      if (name === `exponential_team_config`) handler = h
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), FULL_ACCESS)
  return (args: Record<string, unknown>) => handler!(args, {})
}

const payload = (result: ToolResult) => {
  expect(result.isError, result.content[0]?.text).toBeFalsy()
  return JSON.parse(result.content[0]!.text!) as Record<string, unknown>
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

describe.skipIf(!ready.ok)(`MCP saved-prompt writes (real db)`, () => {
  it(`an owner creates, updates and deletes a prompt`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const call = teamConfig(owner)

    const created = payload(
      await call({ op: `prompts_create`, teamId, name: `Release all`, body: `# Release\n`, icon: `rocket` })
    )
    expect(created).toMatchObject({ teamId, name: `Release all`, body: `# Release\n`, icon: `rocket` })
    const id = created.id as string

    const updated = payload(await call({ op: `prompts_update`, id, name: `Pre-cleanup release`, body: `# v2` }))
    expect(updated).toMatchObject({ id, name: `Pre-cleanup release`, body: `# v2`, icon: `rocket` })
    expect(payload(await call({ op: `prompts_update`, id, icon: null }))).toMatchObject({ id, icon: null })

    // A second prompt can't take the first one's name (the app's rule).
    const clash = await call({ op: `prompts_create`, teamId, name: `Pre-cleanup release`, body: `x` })
    expect(clash.isError).toBe(true)
    expect(clash.content[0]!.text).toContain(`already exists`)

    expect(payload(await call({ op: `prompts_delete`, id }))).toEqual({ ok: true, id })
    expect(await db.select().from(prompts).where(eq(prompts.id, id))).toHaveLength(0)
  })

  it(`refuses a member and a non-member; nothing is written`, async () => {
    const owner = await user()
    const mate = await user()
    const outsider = await user()
    const teamId = await team(owner, mate)
    const id = payload(await teamConfig(owner)({ op: `prompts_create`, teamId, name: `Keep`, body: `body` })).id as string

    for (const u of [mate, outsider]) {
      const call = teamConfig(u)
      for (const args of [
        { op: `prompts_create`, teamId, name: `Nope`, body: `x` },
        { op: `prompts_update`, id, body: `hijacked` },
        { op: `prompts_delete`, id },
      ]) {
        const result = await call(args)
        expect(result.isError, `${u.id} ${args.op}`).toBe(true)
      }
    }
    const rows = await db.select({ name: prompts.name, body: prompts.body }).from(prompts).where(eq(prompts.teamId, teamId))
    expect(rows).toEqual([{ name: `Keep`, body: `body` }])
  })

  it(`refuses an owner of another team acting on this team's prompt id`, async () => {
    const owner = await user()
    const otherOwner = await user()
    const teamId = await team(owner)
    await team(otherOwner)
    const id = payload(await teamConfig(owner)({ op: `prompts_create`, teamId, name: `Mine`, body: `body` })).id as string

    const call = teamConfig(otherOwner)
    expect((await call({ op: `prompts_update`, id, name: `Theirs` })).isError).toBe(true)
    expect((await call({ op: `prompts_delete`, id })).isError).toBe(true)
    expect((await call({ op: `prompts_create`, teamId, name: `Theirs`, body: `x` })).isError).toBe(true)
    const [row] = await db.select({ name: prompts.name }).from(prompts).where(eq(prompts.id, id))
    expect(row).toEqual({ name: `Mine` })
  })

  it(`validates lengths like the app (name 255, body 64KB, blank body)`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const call = teamConfig(owner)
    for (const args of [
      { name: `n`.repeat(256), body: `x` },
      { name: `n`, body: `b`.repeat(64 * 1024 + 1) },
      { name: `n`, body: `   ` },
      { name: `   `, body: `x` },
    ]) {
      expect((await call({ op: `prompts_create`, teamId, ...args })).isError, JSON.stringify(args).slice(0, 60)).toBe(true)
    }
    expect(payload(await call({ op: `prompts_create`, teamId, name: `n`.repeat(255), body: `b`.repeat(64 * 1024) })).name).toHaveLength(255)
    expect(await db.select().from(prompts).where(eq(prompts.teamId, teamId))).toHaveLength(1)
  })
})
