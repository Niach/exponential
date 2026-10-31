// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { comments, issues, teamMembers, teams, widgetConfigs, widgetSubmissions } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { createTestBoard } from "@/test/factories"
import type { McpUser } from "./server"
import { buildMcpAccess, FULL_ACCESS, type McpAccess } from "./scope"

// NEW-19 (FEED-116 + FEED-144) against the REAL migrated Postgres, through the
// noun router: a member reads a widget report's `widget` metadata; its
// reporter (a verified account email, not a member) reads the report and its
// reporter conversation by identifier AND by UUID and replies into it; anyone
// else still gets the member gate's error.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'widget_submissions') AS ok
`)
if (!ready.ok) console.info(`[reporter-access.real-db] skipped: ${ready.reason}`)

const { registerNounTools } = await import(`./nouns`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(verified = true): Promise<McpUser> {
  const id = `reporter-mcp-${id8()}`
  const email = `${id}@reporter.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Emma', ${email}, ${verified}, false, now(), now())
  `)
  made.users.push(id)
  return { id, email, name: `Emma`, image: null, emailVerified: verified, isAdmin: false } as unknown as McpUser
}

/** One noun tool as this user sees it (a full-access key by default). */
function tool(u: McpUser, name: string, access: McpAccess = FULL_ACCESS) {
  let handler: ((args: Record<string, unknown>, extra?: unknown) => Promise<ToolResult>) | undefined
  const server = {
    registerTool: (n: string, _def: unknown, h: typeof handler) => {
      if (n === name) handler = h
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), access)
  return (args: Record<string, unknown>) => handler!(args, {})
}

const payload = (result: ToolResult) => {
  expect(result.isError, result.content[0]?.text).toBeFalsy()
  return JSON.parse(result.content[0]!.text!) as Record<string, unknown>
}

/** A vendor team with one member and a widget report filed by `reporter`. */
async function report(reporter: McpUser) {
  const member = await user()
  const [team] = await db.insert(teams).values({ name: `Vendor`, slug: `vendor-${id8()}` }).returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values({ teamId: team!.id, userId: member.id, role: `owner` })
  const board = await createTestBoard({ teamId: team!.id, prefix: `FB` })
  const [config] = await db
    .insert(widgetConfigs)
    .values({ teamId: team!.id, boardId: board.id, name: `Feedback`, publicKey: `expw_${id8()}${id8()}`, allowedDomains: [`x.test`] })
    .returning({ id: widgetConfigs.id })
  const [issue] = await db
    .insert(issues)
    .values({ boardId: board.id, teamId: team!.id, title: `Save does nothing`, description: `Clicked save.`, status: `backlog`, priority: `none`, source: `widget` })
    .returning({ id: issues.id, identifier: issues.identifier })
  await db.insert(widgetSubmissions).values({
    widgetConfigId: config!.id,
    issueId: issue!.id,
    reporterEmail: reporter.email.toUpperCase(),
    reporterName: `Emma`,
    pageUrl: `https://dev.matflow.test/editor`,
    customData: { site: `app`, page: `/editor` },
    viewportWidth: 1440,
    viewportHeight: 900,
  })
  await db.insert(comments).values([
    { issueId: issue!.id, teamId: team!.id, boardId: board.id, authorId: member.id, body: `Internal: repro on staging`, audience: `team` },
    { issueId: issue!.id, teamId: team!.id, boardId: board.id, authorId: member.id, body: `Thanks, looking into it`, audience: `reporter` },
  ])
  return { member, teamId: team!.id, issueId: issue!.id, identifier: issue!.identifier as string }
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

describe.skipIf(!ready.ok)(`MCP widget metadata + reporter access (real db)`, () => {
  it(`a member sees the widget metadata on get and list`, async () => {
    const reporter = await user()
    const r = await report(reporter)
    const got = payload(await tool(r.member, `exponential_issues`)({ op: `get`, id: r.identifier }))
    expect(got.widget).toMatchObject({
      reporter: { name: `Emma`, email: reporter.email.toUpperCase() },
      customData: { site: `app`, page: `/editor` },
      pageUrl: `https://dev.matflow.test/editor`,
    })
    const listed = await tool(r.member, `exponential_issues`)({ op: `list`, teamId: r.teamId })
    const rows = JSON.parse(listed.content[0]!.text!) as Array<{ id: string; widget?: unknown }>
    expect(rows.find((row) => row.id === r.issueId)?.widget).toEqual({
      reporter: { name: `Emma`, email: reporter.email.toUpperCase() },
      customData: { site: `app`, page: `/editor` },
    })
  })

  it(`the reporter reads the report by identifier and UUID, sees only the reporter conversation, and replies`, async () => {
    const reporter = await user()
    const r = await report(reporter)
    for (const id of [r.identifier, r.issueId]) {
      const view = payload(await tool(reporter, `exponential_issues`)({ op: `get`, id }))
      expect(view).toMatchObject({ identifier: r.identifier, title: `Save does nothing`, access: `reporter` })
      expect(JSON.stringify(view)).not.toContain(`Internal`)
      expect(view.recentComments).toMatchObject([{ author: `Support`, body: `Thanks, looking into it`, fromReporter: false }])
    }

    const created = payload(await tool(reporter, `exponential_comments`)({ op: `create`, issueId: r.identifier, body: `Still broken on Safari` }))
    expect(created).toMatchObject({ source: `reporter`, audience: `reporter` })
    const [row] = await db
      .select({ authorId: comments.authorId, source: comments.source })
      .from(comments)
      .where(and(eq(comments.issueId, r.issueId), eq(comments.source, `reporter`)))
    expect(row).toEqual({ authorId: null, source: `reporter` })

    const list = await tool(reporter, `exponential_comments`)({ op: `list`, issueId: r.identifier })
    expect(list.isError).toBeFalsy()
    expect(list.content[0]!.text).toContain(`Still broken on Safari`)
    expect(list.content[0]!.text).not.toContain(`Internal`)
  })

  it(`anyone else, and an unverified account with the reporter's email, gets the member error`, async () => {
    const reporter = await user()
    const r = await report(reporter)
    const outsider = await user()
    const unverified = { ...(await user(false)), email: reporter.email } as McpUser
    for (const u of [outsider, unverified]) {
      for (const id of [r.identifier, r.issueId]) {
        const got = await tool(u, `exponential_issues`)({ op: `get`, id })
        expect(got.isError).toBe(true)
      }
      const reply = await tool(u, `exponential_comments`)({ op: `create`, issueId: r.issueId, body: `spam` })
      expect(reply.isError).toBe(true)
    }
    // The reporter cannot write anything beyond a reply.
    const update = await tool(reporter, `exponential_issues`)({ op: `update`, id: r.issueId, title: `hijacked` })
    expect(update.isError).toBe(true)
  })

  // NEW-85: the reporter fallback sits outside every team/board selection, so
  // a confined credential (here: a board-confined OAuth grant and a scoped key
  // on the reporter's OWN team) never opens or answers a report elsewhere.
  it(`a confined grant or scoped key never reaches the reporter fallback`, async () => {
    const reporter = await user()
    const r = await report(reporter)
    const [own] = await db.insert(teams).values({ name: `Own`, slug: `own-${id8()}` }).returning({ id: teams.id })
    made.teams.push(own!.id)
    await db.insert(teamMembers).values({ teamId: own!.id, userId: reporter.id, role: `owner` })
    const ownBoard = await createTestBoard({ teamId: own!.id, prefix: `OWN` })
    const boardGrant = buildMcpAccess({ allTeams: false, teamIds: [], boardIds: [ownBoard.id] }, new Map([[ownBoard.id, own!.id]]))
    const scopedKey = buildMcpAccess({ allTeams: false, teamIds: [own!.id], boardIds: [] }, new Map(), `apiKey`)
    for (const access of [boardGrant, scopedKey]) {
      for (const id of [r.identifier, r.issueId]) {
        const got = await tool(reporter, `exponential_issues`, access)({ op: `get`, id })
        expect(got.isError).toBe(true)
        expect(got.content[0]!.text).not.toContain(`Save does nothing`)
        const list = await tool(reporter, `exponential_comments`, access)({ op: `list`, issueId: id })
        expect(list.isError).toBe(true)
        expect(list.content[0]!.text).not.toContain(`Thanks, looking into it`)
      }
      const reply = await tool(reporter, `exponential_comments`, access)({ op: `create`, issueId: r.issueId, body: `scoped reply` })
      expect(reply.isError).toBe(true)
    }
    const replies = await db
      .select({ id: comments.id })
      .from(comments)
      .where(and(eq(comments.issueId, r.issueId), eq(comments.source, `reporter`)))
    expect(replies).toEqual([])
  })
})
