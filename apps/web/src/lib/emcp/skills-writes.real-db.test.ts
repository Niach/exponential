// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { skills, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "./server"
import { FULL_ACCESS } from "./scope"

// #EXP-1663: `exponential_skills` against the REAL migrated Postgres, through
// the noun router into the same tRPC router Settings › Skills uses: an owner
// writes, a member reads but every write answers "only team owners can …",
// an outsider is refused, and fixtures/skills.json's limits hold.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'skills') AS ok
`)
if (!ready.ok) console.info(`[skills-writes.real-db] skipped: ${ready.reason}`)

const { registerNounTools } = await import(`./nouns`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<McpUser> {
  const id = `skill-mcp-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@skills.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email: `${id}@skills.test`, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(owner: McpUser, member?: McpUser): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Skills`, slug: `skills-${id8()}` }).returning({ id: teams.id })
  const teamId = row!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner.id, role: `owner` },
    ...(member ? [{ teamId, userId: member.id, role: `member` as const }] : []),
  ])
  return teamId
}

/** `exponential_skills` as this user sees it (a full-access key). */
function skillsTool(u: McpUser) {
  let handler: ((args: Record<string, unknown>, extra?: unknown) => Promise<ToolResult>) | undefined
  const server = {
    registerTool: (name: string, _def: unknown, h: typeof handler) => {
      if (name === `exponential_skills`) handler = h
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), FULL_ACCESS)
  return (args: Record<string, unknown>) => handler!(args, {})
}

const payload = (result: ToolResult) => {
  expect(result.isError, result.content[0]?.text).toBeFalsy()
  return JSON.parse(result.content[0]!.text!) as unknown
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

describe.skipIf(!ready.ok)(`MCP skills (real db)`, () => {
  it(`an owner lists, gets, creates, updates and deletes a skill`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const call = skillsTool(owner)

    const created = payload(
      await call({ op: `create`, teamId, name: `review-checklist`, description: `Review a PR`, body: `# Review\n` })
    ) as Record<string, unknown>
    expect(created).toMatchObject({ teamId, name: `review-checklist`, body: `# Review\n`, enabledByDefault: false })
    const id = created.id as string

    const listed = payload(await call({ op: `list`, teamId })) as Array<Record<string, unknown>>
    expect(listed.map((skill) => skill.name)).toEqual([`review-checklist`])
    expect(listed[0]).not.toHaveProperty(`body`)
    expect(payload(await call({ op: `list`, search: `review a` }))).toHaveLength(1)
    expect(payload(await call({ op: `get`, name: `review-checklist`, teamId }))).toMatchObject({ id, body: `# Review\n` })

    const updated = payload(await call({ op: `update`, id, enabledByDefault: true, body: `# v2` }))
    expect(updated).toMatchObject({ id, enabledByDefault: true, body: `# v2` })

    const clash = await call({ op: `create`, teamId, name: `review-checklist`, description: `d`, body: `x` })
    expect(clash.content[0]!.text).toContain(`already exists`)

    expect(payload(await call({ op: `delete`, id }))).toEqual({ ok: true, id })
    expect(await db.select().from(skills).where(eq(skills.id, id))).toHaveLength(0)
  })

  it(`a member reads, and every write says only team owners can; an outsider is refused`, async () => {
    const owner = await user()
    const mate = await user()
    const outsider = await user()
    const teamId = await team(owner, mate)
    const id = (payload(await skillsTool(owner)({ op: `create`, teamId, name: `keep`, description: `d`, body: `body` })) as {
      id: string
    }).id

    const member = skillsTool(mate)
    expect(payload(await member({ op: `get`, id }))).toMatchObject({ id, body: `body` })
    expect(payload(await member({ op: `list`, teamId }))).toHaveLength(1)
    const writes = [
      { op: `create`, teamId, name: `nope`, description: `d`, body: `x` },
      { op: `update`, id, body: `hijacked` },
      { op: `delete`, id },
    ]
    for (const args of writes) {
      const result = await member(args)
      expect(result.isError, args.op).toBe(true)
      expect(result.content[0]!.text, args.op).toMatch(/only team owners can/i)
    }
    const stranger = skillsTool(outsider)
    for (const args of [{ op: `get`, id }, ...writes]) {
      expect((await stranger(args)).isError, `outsider ${args.op}`).toBe(true)
    }
    const rows = await db.select({ name: skills.name, body: skills.body }).from(skills).where(eq(skills.teamId, teamId))
    expect(rows).toEqual([{ name: `keep`, body: `body` }])
  })

  it(`validates like fixtures/skills.json (name pattern, description 1024, body 64 KB)`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const call = skillsTool(owner)
    for (const args of [
      { name: `Has Caps`, description: `d`, body: `x` },
      { name: `n`.repeat(65), description: `d`, body: `x` },
      { name: `n`, description: `d`.repeat(1025), body: `x` },
      { name: `n`, description: `   `, body: `x` },
      { name: `n`, description: `d`, body: `b`.repeat(64 * 1024 + 1) },
      { name: `n`, description: `d`, body: `   ` },
    ]) {
      expect((await call({ op: `create`, teamId, ...args })).isError, JSON.stringify(args).slice(0, 60)).toBe(true)
    }
    const max = payload(
      await call({ op: `create`, teamId, name: `n`.repeat(64), description: `d`.repeat(1024), body: `b`.repeat(64 * 1024) })
    ) as { name: string }
    expect(max.name).toHaveLength(64)
    expect(await db.select().from(skills).where(eq(skills.teamId, teamId))).toHaveLength(1)
  })
})
