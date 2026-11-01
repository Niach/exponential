// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { attachments, devices, messages, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import type { McpUser } from "./server"
import { FULL_ACCESS } from "./scope"

// EXP-1516: an MCP caller sends an image to a session. `exponential_attachments
// {op: "upload"}` without issueId = a LOOSE upload (issue, draft, message and
// board NULL) in the caller's session's team, which `sessions_send
// {attachmentIds}` then claims onto its message: both tools through the noun
// router against the REAL migrated Postgres (the object store is stubbed).

const uploaded = vi.hoisted(() => [] as string[])
vi.mock(`@/lib/storage`, () => ({
  uploadObject: async (input: { key: string }) => {
    uploaded.push(input.key)
  },
  deleteObject: async () => {},
}))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))
vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'root_id')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[attachments-loose.real-db] skipped: ${ready.reason}`)

const { registerNounTools } = await import(`./nouns`)
const { sendSession } = await import(`@/lib/sessions/send`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

// A 1×1 PNG: the paste path probes its pixels.
const PNG = `iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==`

async function user(): Promise<McpUser> {
  const id = `loose-mcp-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@loose.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email: `${id}@loose.test`, name: `Tester`, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(owner: McpUser): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Loose`, slug: `loose-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values({ teamId: row!.id, userId: owner.id, role: `owner` })
  return row!.id
}

/** One noun tool as this user sees it, optionally inside a session. */
function noun(u: McpUser, name: string, sessionId: string | null = null) {
  let handler: ((args: Record<string, unknown>, extra?: unknown) => Promise<ToolResult>) | undefined
  const server = {
    registerTool: (n: string, _def: unknown, h: typeof handler) => {
      if (n === name) handler = h
    },
  }
  registerNounTools(server as never, u, new Request(`https://x.test/api/mcp`), FULL_ACCESS, sessionId)
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

describe.skipIf(!ready.ok)(`loose MCP upload → sessions_send (real db)`, () => {
  it(`uploads without issueId into the session's team, and sessions_send claims it onto the message`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    // A second team: the session, not "the only team", picks where it lands.
    await team(owner)
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({ userId: owner.id, deviceId, label: `box`, kind: `server`, sharedTeamIds: [] })
    const root = await sendSession({ userId: owner.id, input: { teamId, deviceId, text: `start` } })

    const upload = payload(
      await noun(owner, `exponential_attachments`, root.sessionId)({
        op: `upload`,
        filename: `shot.png`,
        contentType: `image/png`,
        dataBase64: PNG,
      })
    )
    const id = upload.id as string
    expect(upload).toMatchObject({ url: `/api/attachments/${id}`, width: 1, height: 1 })
    const [row] = await db.select().from(attachments).where(eq(attachments.id, id))
    expect(row).toMatchObject({
      teamId,
      boardId: null,
      issueId: null,
      draftId: null,
      messageId: null,
      uploaderId: owner.id,
      storageKey: `session-attachments/pending/${teamId}/${id}-shot.png`,
    })
    expect(uploaded).toContain(row!.storageKey)

    const sent = payload(
      await noun(owner, `exponential_sessions_send`, root.sessionId)({
        sessionId: root.sessionId,
        text: `look`,
        attachmentIds: [id],
      })
    )
    const messageId = sent.messageId as string
    const [claimed] = await db.select({ messageId: attachments.messageId }).from(attachments).where(eq(attachments.id, id))
    expect(claimed!.messageId).toBe(messageId)
    const [message] = await db
      .select({ body: messages.body, attachmentIds: messages.attachmentIds })
      .from(messages)
      .where(eq(messages.id, messageId))
    expect(message!.attachmentIds).toEqual([id])
    expect(message!.body).toBe(`look\n\n![shot.png](/api/attachments/${id})`)
  })

  it(`without a session: the only team, else a clear refusal`, async () => {
    const owner = await user()
    const teamId = await team(owner)
    const upload = noun(owner, `exponential_attachments`)
    const args = { op: `upload`, filename: `shot.png`, contentType: `image/png`, dataBase64: PNG }
    const id = payload(await upload(args)).id as string
    const [row] = await db.select({ teamId: attachments.teamId }).from(attachments).where(eq(attachments.id, id))
    expect(row!.teamId).toBe(teamId)

    // Two teams and no session: ambiguous (membership is cached ~10 s per
    // user, hence a second user).
    const multi = await user()
    await team(multi)
    await team(multi)
    const refused = await noun(multi, `exponential_attachments`)(args)
    expect(refused.isError).toBe(true)
    expect(refused.content[0]!.text).toContain(`pass issueId, or upload from a session`)
  })
})
