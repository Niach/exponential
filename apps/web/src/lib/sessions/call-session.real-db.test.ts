// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, asc, eq, inArray, sql } from "drizzle-orm"
import { VOICE_CALL_SESSION } from "@exp/etalk/voice-call"
import { db } from "@/db/connection"
import { agents, devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { inSession, insertMessage } from "@/lib/sessions/messages"

// #EXP-1961 (etalk voice-call.json `callSession`, eagent bus.json, session-brief
// `callCases`) against the REAL migrated Postgres: a call's own session under
// the chat, its brief, the chat copied in before an utterance, no per-turn
// relay, the summary report after the hang-up, resume and replace.

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
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'sessions' AND column_name = 'voice_call') AS ok
`)
if (!ready.ok) console.info(`[call-session.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { startCallSession, endCallSession } = await import(`@/lib/sessions/call`)
const { busRelayTurn } = await import(`@/lib/sessions/bus`)
const { composeSessionBrief } = await import(`@/lib/sessions/brief`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function world() {
  const owner = `call-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Danny', ${`${owner}@call.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `Call`, slug: `call-${id8()}` }).returning({ id: teams.id })
  made.teams.push(team!.id)
  await db.insert(teamMembers).values({ teamId: team!.id, userId: owner, role: `owner` })
  const deviceId = `dev-${id8()}`
  await db.insert(devices).values({ userId: owner, deviceId, label: `box`, kind: `desktop`, sharedTeamIds: [] })
  const [agent] = await db
    .insert(agents)
    .values({ teamId: team!.id, name: `Ops`, icon: `rocket`, color: `#ff0000`, instructions: ``, deviceId, ownerId: owner, role: `agent`, voice: { language: `de` } })
    .returning({ id: agents.id })
  return { owner, agentId: agent!.id }
}

const rowsOf = (sessionId: string) =>
  db
    .select({ body: messages.body, card: messages.card, authorSessionId: messages.authorSessionId, consumedAt: messages.consumedAt })
    .from(messages)
    .where(inSession(sessionId))
    .orderBy(asc(messages.createdAt))

const callOf = async (id: string) =>
  (await db.select({ voiceCall: sessions.voiceCall, status: sessions.status }).from(sessions).where(eq(sessions.id, id)).limit(1))[0]!

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-1961 call sessions`, () => {
  it(`starts under the chat, briefs it, copies the chat in, relays only the summary, resumes and replaces`, async () => {
    const w = await world()
    // No chat with the agent yet: the old path.
    expect(await startCallSession({ userId: w.owner, agentId: w.agentId, callId: `c1` })).toBeNull()
    const chat = await sendSession({ userId: w.owner, input: { agentId: w.agentId, text: `Can you check the build?` } })
    await db.transaction((tx) => insertMessage(tx, chat.sessionId, { authorKind: `agent`, body: `The build is green.`, consumedAt: new Date() }))

    const t0 = new Date()
    const call = (await startCallSession({ userId: w.owner, agentId: w.agentId, callId: `c1`, now: t0 }))!
    expect(call).toMatchObject({ chatId: chat.sessionId, resumed: false })
    const [child] = await db.select().from(sessions).where(eq(sessions.id, call.sessionId))
    expect(child).toMatchObject({ parentId: chat.sessionId, title: `Call` })
    expect(child!.voiceCall).toMatchObject({ callId: `c1`, endedAt: null, durationMs: 0, summaryPending: false })
    expect((await rowsOf(call.sessionId))[0]!.body).toContain(`in German`)
    const brief = await composeSessionBrief(call.sessionId)
    expect(brief.call).toBe(true)
    expect(brief.brief).toContain(`## Call`)
    expect(brief.brief).toContain(`- Danny: Can you check the build?\n- You: The build is green.`)
    expect(brief.brief).not.toContain(`## Parent`)

    // Typed in the chat during the call: copied in before the next utterance, never waking the call by itself.
    await sendSession({ userId: w.owner, input: { sessionId: chat.sessionId, text: `typed in the chat` } })
    expect((await rowsOf(call.sessionId)).map((row) => row.body)).not.toContain(`typed in the chat`)
    await sendSession({ userId: w.owner, input: { sessionId: call.sessionId, text: `what's new?` } })
    const inCall = await rowsOf(call.sessionId)
    const copied = inCall.findIndex((row) => row.body === `typed in the chat`)
    expect(copied).toBeGreaterThan(0)
    expect(inCall[copied]).toMatchObject({ authorSessionId: chat.sessionId, consumedAt: null })
    expect(inCall[copied + 1]!.body).toBe(`what's new?`)
    // Sent once: the next utterance copies nothing again.
    await sendSession({ userId: w.owner, input: { sessionId: call.sessionId, text: `and?` } })
    expect((await rowsOf(call.sessionId)).filter((row) => row.body === `typed in the chat`)).toHaveLength(1)

    // A spoken turn reaches nobody.
    const chatBefore = (await rowsOf(chat.sessionId)).length
    await db.transaction((tx) => busRelayTurn(tx, { id: call.sessionId, parentId: chat.sessionId, startedByAgentId: null }, `Sure.`))
    expect(await rowsOf(chat.sessionId)).toHaveLength(chatBefore)

    // The hang-up: the summary request, then the summary as a waking report in the chat.
    const t1 = new Date(t0.getTime() + 252_000)
    expect(await endCallSession({ userId: w.owner, sessionId: call.sessionId, now: t1 })).toBe(true)
    expect(await endCallSession({ userId: w.owner, sessionId: call.sessionId, now: t1 })).toBe(false)
    expect((await callOf(call.sessionId)).voiceCall).toMatchObject({ durationMs: 252_000, summaryPending: true, endedAt: t1.toISOString() })
    expect((await rowsOf(call.sessionId)).at(-1)!.body).toContain(`after 04:12`)
    await db.transaction((tx) =>
      busRelayTurn(tx, { id: call.sessionId, parentId: chat.sessionId, startedByAgentId: null }, `We checked the build.`)
    )
    const report = (await rowsOf(chat.sessionId)).at(-1)!
    expect(report).toMatchObject({ body: `We checked the build.`, authorSessionId: call.sessionId, consumedAt: null })
    expect(report.card).toMatchObject({
      kind: `report`,
      sessionId: call.sessionId,
      summary: `We checked the build.`,
      final: false,
      call: { startedAt: t0.toISOString(), endedAt: t1.toISOString(), durationMs: 252_000 },
    })
    expect((await callOf(call.sessionId)).voiceCall!.summaryPending).toBe(false)

    // Called back within the window: the same session, a resume line.
    // #EXP-2014: typed in the chat meanwhile: never beside the resume line
    // (its reply is the opening), copied in before the next utterance.
    await sendSession({ userId: w.owner, input: { sessionId: chat.sessionId, text: `typed while away` } })
    const t2 = new Date(t1.getTime() + 60_000)
    const again = (await startCallSession({ userId: w.owner, agentId: w.agentId, callId: `c2`, now: t2 }))!
    expect(again).toEqual({ sessionId: call.sessionId, chatId: chat.sessionId, resumed: true })
    const resumed = await rowsOf(call.sessionId)
    expect(resumed.at(-1)!.body).toBe(VOICE_CALL_SESSION.resumeText.replace(`{language}`, `German`))
    expect(resumed.map((row) => row.body)).not.toContain(`typed while away`)
    await sendSession({ userId: w.owner, input: { sessionId: call.sessionId, text: `back again` } })
    const after = (await rowsOf(call.sessionId)).map((row) => row.body)
    expect(after.slice(-2)).toEqual([`typed while away`, `back again`])
    // A reconnect of that running call says nothing new.
    const count = (await rowsOf(call.sessionId)).length
    await startCallSession({ userId: w.owner, agentId: w.agentId, callId: `c2`, now: t2 })
    expect(await rowsOf(call.sessionId)).toHaveLength(count)
    const t3 = new Date(t2.getTime() + 60_000)
    await endCallSession({ userId: w.owner, sessionId: call.sessionId, now: t3 })
    expect((await callOf(call.sessionId)).voiceCall!.durationMs).toBe(312_000)

    // Past the window: that session ends quietly, a new one starts.
    const reports = async () => (await rowsOf(chat.sessionId)).filter((row) => row.card?.kind === `report`).length
    const before = await reports()
    const t4 = new Date(t3.getTime() + VOICE_CALL_SESSION.resumeMs + 1)
    const fresh = (await startCallSession({ userId: w.owner, agentId: w.agentId, callId: `c3`, now: t4 }))!
    expect(fresh.sessionId).not.toBe(call.sessionId)
    expect((await callOf(call.sessionId)).status).toBe(`ended`)
    expect(await reports()).toBe(before)
    const children = await db.select({ id: sessions.id }).from(sessions).where(and(eq(sessions.parentId, chat.sessionId)))
    expect(children).toHaveLength(2)
  })
})
