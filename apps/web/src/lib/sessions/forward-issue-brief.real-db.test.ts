// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { composeIssueBrief } from "@exp/domain-contract/session-brief"
import { db } from "@/db/connection"
import { agents, boards, devices, issues, messages, sessionIssues, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1584 (fixtures/session-brief.json `issueBriefCases`): Start coding
// with a team agent picked sends the issue into the member's conversation
// with it: the message carries the issue's section (`issue_brief`) for the
// device to hand the agent, plaintext and sealed alike; a NEW conversation's
// brief carries it too. #EXP-1686: no row ever lands on the conversation.
// Against the REAL migrated Postgres.

process.env.BETTER_AUTH_SECRET ||= `forward-brief-test-secret-0123456789abcdef`

vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  fireAndForgetPrNotify: () => {},
  notifySessionBlocked: async () => {},
  notifySessionAsk: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(
  `SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'messages' AND column_name = 'issue_brief') AS ok`
)
if (!ready.ok) console.info(`[forward-issue-brief.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { composeSessionBrief } = await import(`@/lib/sessions/brief`)
const { AGENT_CONVERSATION_LINK_MESSAGE, linkIssues } = await import(`@/lib/sessions/issues`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

async function setup() {
  const owner = `fwd-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Danny', ${`${owner}@fwd.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Fwd`, slug: `fwd-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `server` })
  const [agent] = await db
    .insert(agents)
    .values({ teamId, name: `Big Brother`, deviceId, ownerId: owner })
    .returning()
  const [board] = await db
    .insert(boards)
    .values({ teamId, name: `Exp`, slug: `exp-${id8()}`, prefix: `EXP` })
    .returning()
  const [issue] = await db
    .insert(issues)
    .values({
      boardId: board!.id,
      teamId,
      title: `Agent picks drop the issue`,
      description: `The agent only got the prompt.`,
      creatorId: owner,
    })
    .returning()
  return { owner, teamId, agentId: agent!.id, issue: issue! }
}

describe.skipIf(!ready.ok)(`#EXP-1584 forwarded issues reach the agent`, () => {
  it(`an existing agent conversation's message carries the issue brief`, async () => {
    const { owner, agentId, issue } = await setup()
    const first = await sendSession({ userId: owner, input: { agentId, text: `Hi.` } })
    await db.update(sessions).set({ status: `idle` }).where(eq(sessions.id, first.sessionId))

    const sent = await sendSession({ userId: owner, input: { agentId, text: `Please look.`, issueIds: [issue.id] } })
    expect(sent.sessionId).toBe(first.sessionId)
    const [row] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(row!.body).toBe(`Please look.`)
    expect(row!.issueBrief).toBe(
      composeIssueBrief([{ identifier: issue.identifier, title: `Agent picks drop the issue`, description: `The agent only got the prompt.` }])
    )
  })

  it(`a bare forward wakes the agent with the refs`, async () => {
    const { owner, agentId, issue } = await setup()
    const first = await sendSession({ userId: owner, input: { agentId, text: `Hi.` } })
    await db.update(sessions).set({ status: `idle` }).where(eq(sessions.id, first.sessionId))

    const sent = await sendSession({ userId: owner, input: { agentId, issueIds: [issue.id] } })
    const [row] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(row!.body).toBe(`#${issue.identifier}`)
    expect(row!.issueBrief).toContain(`### ${issue.identifier}\n\nThe agent only got the prompt.`)
  })

  it(`a new conversation's brief lists the issue with its description, the message carries it too`, async () => {
    const { owner, agentId, issue } = await setup()
    const sent = await sendSession({ userId: owner, input: { agentId, text: `Please look.`, issueIds: [issue.id] } })
    const [row] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(row!.issueBrief).toContain(`- ${issue.identifier} Agent picks drop the issue`)
    const { brief } = await composeSessionBrief(sent.sessionId)
    expect(brief).toBe(
      `## Request\n\nPlease look.\n\n## Issues\n\n- ${issue.identifier} Agent picks drop the issue\n\n### ${issue.identifier}\n\nThe agent only got the prompt.`
    )
  })

  // #EXP-1686: Bro's chat was sealed and Start coding sent no text: the rows
  // landed, no message did, the agent never heard.
  it(`a bare forward into a SEALED conversation wakes the agent and links nothing`, async () => {
    const { owner, agentId, issue } = await setup()
    const first = await sendSession({ userId: owner, input: { agentId, text: `Hi.` } })
    await db
      .update(sessions)
      .set({ status: `idle`, sealEpoch: 1, sealPublicKey: `k`.repeat(43) })
      .where(eq(sessions.id, first.sessionId))

    const sent = await sendSession({ userId: owner, input: { agentId, issueIds: [issue.id] } })
    expect(sent.sessionId).toBe(first.sessionId)
    const [row] = await db.select().from(messages).where(eq(messages.id, sent.messageId!))
    expect(row!.body).toBe(`#${issue.identifier}`)
    expect(row!.consumedAt).toBeNull()
    expect(row!.issueBrief).toContain(`### ${issue.identifier}\n\nThe agent only got the prompt.`)
    expect(await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, first.sessionId))).toEqual([])
  })

  it(`a conversation holds no rows on any path: naming it, an explicit link`, async () => {
    const { owner, agentId, issue } = await setup()
    const first = await sendSession({ userId: owner, input: { agentId, issueIds: [issue.id] } })
    const named = await sendSession({ userId: owner, input: { sessionId: first.sessionId, issueIds: [issue.id] } })
    expect(named.messageId).not.toBeNull()
    expect(await db.select().from(sessionIssues).where(eq(sessionIssues.sessionId, first.sessionId))).toEqual([])
    await expect(
      db.transaction((tx) =>
        linkIssues(tx, { sessionId: first.sessionId, issueIds: [issue.id], source: `user`, addedBy: owner, addedBySessionId: null })
      )
    ).rejects.toThrow(AGENT_CONVERSATION_LINK_MESSAGE)
  })
})
