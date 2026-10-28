// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import { agents, conversations, devices, messages, teamMembers, teams, userVoiceSettings } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// NEW-57 (meeting.json RESIDENCY) against the REAL migrated Postgres:
// `sessions.send {meeting: true}` stamps `messages.expires_at` only on the
// caller's own conversation with THEIR voice agent, and the watchdog's prune
// deletes expired rows in a bounded batch while the digest trigger keeps
// `conversations.digest` in step. The prune scans the whole database, so its
// case runs at a `now` far in the past and backdates only its own rows.

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
}))
vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => null,
}))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'messages' AND column_name = 'expires_at')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'refresh_conversation_digests')
  ) AS ok
`)
if (!ready.ok) console.info(`[meeting.real-db] skipped: ${ready.reason}`)

const { sendSession, MEETING_SEND_REFUSED } = await import(`@/lib/sessions/send`)
const { pruneExpiredMessages } = await import(`@/lib/sessions/watchdog`)

const DAY = 24 * 60 * 60 * 1000
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `meet-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@meeting.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db
    .insert(teams)
    .values({ name: `Meeting`, slug: `meeting-${id8()}` })
    .returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, sharedTeamIds: [teamId] })
  const agent = async (name: string) =>
    (
      await db
        .insert(agents)
        .values({ teamId, name, icon: `rocket`, color: `#ff0000`, instructions: ``, deviceId, ownerId: owner })
        .returning({ id: agents.id })
    )[0]!.id
  const ops = await agent(`Ops`)
  const other = await agent(`Other`)
  return { owner, mate, teamId, deviceId, ops, other }
}

const expiresOf = async (id: string) =>
  (await db.select({ expiresAt: messages.expiresAt }).from(messages).where(eq(messages.id, id)).limit(1))[0]?.expiresAt

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe.skipIf(!ready.ok)(`#NEW-57 meeting sends`, () => {
  it(`stamps expires_at only on the caller's own conversation with their voice agent`, async () => {
    const w = await world()
    const refused = { code: `BAD_REQUEST`, message: MEETING_SEND_REFUSED }
    // No voice agent picked: refused.
    await expect(
      sendSession({ userId: w.owner, input: { agentId: w.ops, text: `batch` }, meeting: true })
    ).rejects.toMatchObject(refused)

    await db.insert(userVoiceSettings).values({ userId: w.owner, agentId: w.ops, voiceDeviceId: w.deviceId })
    const sent = await sendSession({ userId: w.owner, input: { agentId: w.ops, text: `batch 1` }, meeting: true })
    const at = await expiresOf(sent.messageId!)
    expect(at).toBeInstanceOf(Date)
    const days = (at!.getTime() - Date.now()) / DAY
    expect(days).toBeGreaterThan(contract.meeting.retentionDays - 0.01)
    expect(days).toBeLessThan(contract.meeting.retentionDays + 0.01)

    // The conversation's sessionId form works too; a plain send never expires.
    const bySession = await sendSession({
      userId: w.owner,
      input: { sessionId: sent.sessionId, text: `batch 2` },
      meeting: true,
    })
    expect(bySession.sessionId).toBe(sent.sessionId)
    expect(await expiresOf(bySession.messageId!)).toBeInstanceOf(Date)
    const plain = await sendSession({ userId: w.owner, input: { agentId: w.ops, text: `hello` } })
    expect(await expiresOf(plain.messageId!)).toBeNull()

    // Another agent, another member's conversation, an extra field, MCP: refused.
    await expect(
      sendSession({ userId: w.owner, input: { agentId: w.other, text: `batch` }, meeting: true })
    ).rejects.toMatchObject(refused)
    const otherConversation = await sendSession({ userId: w.owner, input: { agentId: w.other, text: `hi` } })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: otherConversation.sessionId, text: `batch` }, meeting: true })
    ).rejects.toMatchObject(refused)
    await db.insert(userVoiceSettings).values({ userId: w.mate, agentId: w.ops })
    await expect(
      sendSession({ userId: w.mate, input: { sessionId: sent.sessionId, text: `batch` }, meeting: true })
    ).rejects.toMatchObject(refused)
    await expect(
      sendSession({ userId: w.owner, input: { agentId: w.ops, text: `batch`, issueIds: [crypto.randomUUID()] }, meeting: true })
    ).rejects.toMatchObject(refused)
    await expect(
      sendSession({ userId: w.owner, input: { agentId: w.ops, text: `batch` }, meeting: true, viaMcp: true })
    ).rejects.toMatchObject(refused)
  })

  it(`the prune deletes expired rows in a bounded batch and the digest follows`, async () => {
    const w = await world()
    await db.insert(userVoiceSettings).values({ userId: w.owner, agentId: w.ops, voiceDeviceId: w.deviceId })
    const kept = await sendSession({ userId: w.owner, input: { agentId: w.ops, text: `kept forever` } })
    const batch = await sendSession({ userId: w.owner, input: { agentId: w.ops, text: `meeting batch` }, meeting: true })
    const [conversation] = await db
      .select({ id: conversations.id, digest: conversations.digest })
      .from(conversations)
      .where(eq(conversations.sessionId, batch.sessionId))
    expect(JSON.stringify(conversation!.digest)).toContain(`meeting batch`)

    // Backdate ONLY this row's expiry into 1999; prune at 2000 (nobody
    // else's row is that old). The live row stays.
    const NOW = new Date(`2000-01-01T00:00:00.000Z`)
    await db.update(messages).set({ expiresAt: new Date(`1999-12-31T00:00:00.000Z`) }).where(eq(messages.id, batch.messageId!))
    expect(await pruneExpiredMessages(NOW)).toBe(1)
    expect(await db.select({ id: messages.id }).from(messages).where(eq(messages.id, batch.messageId!))).toEqual([])
    expect(await db.select({ id: messages.id }).from(messages).where(eq(messages.id, kept.messageId!))).toHaveLength(1)
    const [after] = await db
      .select({ digest: conversations.digest })
      .from(conversations)
      .where(eq(conversations.id, conversation!.id))
    expect(JSON.stringify(after!.digest)).not.toContain(`meeting batch`)
    expect(JSON.stringify(after!.digest)).toContain(`kept forever`)
    // Nothing left to prune at that clock.
    expect(await pruneExpiredMessages(NOW)).toBe(0)
  })
})
