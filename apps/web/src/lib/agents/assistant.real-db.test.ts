// @vitest-environment node
// S-3 (agent-settings.json `assistant`): against the REAL migrated Postgres.
// Every member's ONE Assistant per team: created on first use (race-safe on
// the unique (team_id, owner_id) WHERE role = 'assistant' index) on their
// newest own desktop, null without a device; edited by its owner only (a
// team owner gets NOT_FOUND), one schedule preset only, never deleted; and
// migration 0208's data step (a member's own voice agent -> their Assistant,
// the voice mirror re-derived).
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
import { db } from "@/db/connection"
import { agents, boards, devices, issues, sessions, teamMembers, teams, userVoiceSettings } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => null,
    relayPostNudge: async () => {},
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})

const ready = await probeRealDb(`
  SELECT EXISTS (
    SELECT 1 FROM pg_indexes WHERE indexname = 'uniq_agents_assistant'
  ) AS ok
`)
if (!ready.ok) console.info(`[assistant.real-db] skipped: ${ready.reason}`)

const { ensureAssistant } = await import(`@/lib/agents/assistant`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)
const { conversationInTx } = await import(`@/lib/agents/conversation`)
const { assertAgentInTeam, onIssueAssignedInTx } = await import(`@/lib/agents/assignment`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `assistant-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@assistant.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const mate = await user()
  const [team] = await db.insert(teams).values({ name: `Assistants`, slug: `assistant-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: mate, role: `member` },
  ])
  const device = async (userId: string, kind: `desktop` | `phone` | `server`, minutesAgo: number) => {
    const deviceId = `dev-${kind}-${id8()}`
    await db.insert(devices).values({
      userId,
      deviceId,
      label: kind,
      kind,
      lastSeenAt: new Date(Date.now() - minutesAgo * 60_000),
    })
    return deviceId
  }
  return { owner, mate, teamId, device }
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

const person = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) })
const assistantsOf = (teamId: string, ownerId: string) =>
  db
    .select()
    .from(agents)
    .where(and(eq(agents.teamId, teamId), eq(agents.ownerId, ownerId), eq(agents.role, `assistant`)))

describe.skipIf(!ready.ok)(`ensureAssistant (S-3)`, () => {
  it(`creates ONE Assistant on the newest own desktop, even when two first uses race`, async () => {
    const w = await world()
    const desktop = await w.device(w.mate, `desktop`, 30)
    // A phone seen more recently never wins over a desktop.
    await w.device(w.mate, `phone`, 1)
    const [a, b] = await Promise.all([ensureAssistant(w.mate, w.teamId), ensureAssistant(w.mate, w.teamId)])
    expect(a?.id).toBeDefined()
    expect(b?.id).toBe(a?.id)
    const rows = await assistantsOf(w.teamId, w.mate)
    expect(rows).toHaveLength(1)
    expect(rows[0]).toMatchObject({
      name: ASSISTANT_COPY.name,
      icon: null,
      color: null,
      deviceId: desktop,
      role: `assistant`,
    })
    expect(rows[0]!.triggers).toEqual([expect.objectContaining({ kind: `schedule`, cron: `0 * * * *`, enabled: true })])
    // A later call answers the same row.
    expect((await ensureAssistant(w.mate, w.teamId))?.id).toBe(a?.id)
    // The voice mirror follows the Assistant.
    const [voice] = await db.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, w.mate))
    expect(voice).toMatchObject({ agentId: a!.id, voiceDeviceId: desktop })
  })

  it(`falls back to the newest device, and answers null without one`, async () => {
    const w = await world()
    expect(await ensureAssistant(w.mate, w.teamId)).toBeNull()
    expect(await assistantsOf(w.teamId, w.mate)).toHaveLength(0)
    const phone = await w.device(w.mate, `phone`, 1)
    expect((await ensureAssistant(w.mate, w.teamId))?.deviceId).toBe(phone)
  })

  it(`refuses a non-member`, async () => {
    const w = await world()
    const stranger = await user()
    await expect(ensureAssistant(stranger, w.teamId)).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`the tRPC call answers {agent: null} without a device`, async () => {
    const w = await world()
    expect(await agentsRouter.createCaller(person(w.mate) as never).ensureAssistant({ teamId: w.teamId })).toEqual({
      agent: null,
    })
  })
})

describe.skipIf(!ready.ok)(`agents.update / delete on an Assistant (S-3)`, () => {
  it(`is NOT_FOUND for anyone but its owner, even a team owner`, async () => {
    const w = await world()
    await w.device(w.mate, `desktop`, 1)
    const assistant = (await ensureAssistant(w.mate, w.teamId))!
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await expect(asOwner.update({ id: assistant.id, name: `Mine now` })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(asOwner.delete({ id: assistant.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(asOwner.get({ id: assistant.id })).rejects.toMatchObject({ code: `NOT_FOUND` })
    // Its owner (a plain member) edits it.
    const asMate = agentsRouter.createCaller(person(w.mate) as never)
    const { agent } = await asMate.update({ id: assistant.id, name: `Jarvis` })
    expect(agent.name).toBe(`Jarvis`)
  })

  it(`takes exactly one schedule preset, nothing else`, async () => {
    const w = await world()
    await w.device(w.mate, `desktop`, 1)
    const assistant = (await ensureAssistant(w.mate, w.teamId))!
    const asMate = agentsRouter.createCaller(person(w.mate) as never)
    for (const triggers of [
      [{ kind: `schedule` as const, cron: `*/10 * * * *`, enabled: true }],
      [{ kind: `event` as const, name: `issue.created`, enabled: true }],
      [
        { kind: `schedule` as const, cron: `*/15 * * * *`, enabled: true },
        { kind: `schedule` as const, cron: `0 * * * *`, enabled: true },
      ],
    ]) {
      await expect(asMate.update({ id: assistant.id, triggers } as never), JSON.stringify(triggers)).rejects.toMatchObject({
        code: `BAD_REQUEST`,
        message: ASSISTANT_COPY.scheduleRefused,
      })
    }
    const daily = await asMate.update({
      id: assistant.id,
      triggers: [{ kind: `schedule`, cron: `0 9 * * *`, tz: `Europe/Vienna`, enabled: true }],
    } as never)
    expect(daily.agent.triggers).toEqual([expect.objectContaining({ cron: `0 9 * * *`, tz: `Europe/Vienna` })])
    expect((await asMate.update({ id: assistant.id, triggers: [] })).agent.triggers).toEqual([])
  })

  it(`#EXP-1439: runs on the built-in prompt; an edit of its instructions is refused`, async () => {
    const w = await world()
    await w.device(w.mate, `desktop`, 1)
    const assistant = (await ensureAssistant(w.mate, w.teamId))!
    const asMate = agentsRouter.createCaller(person(w.mate) as never)
    await expect(asMate.update({ id: assistant.id, instructions: `Be loud.` })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: ASSISTANT_COPY.promptFixed,
    })
    // An old client resending the stored text unchanged still saves.
    const { agent } = await asMate.update({ id: assistant.id, name: `Jarvis`, instructions: assistant.instructions })
    expect(agent.name).toBe(`Jarvis`)
  })

  it(`is never deleted, not even by its owner`, async () => {
    const w = await world()
    await w.device(w.owner, `desktop`, 1)
    const assistant = (await ensureAssistant(w.owner, w.teamId))!
    await expect(
      agentsRouter.createCaller(person(w.owner) as never).delete({ id: assistant.id })
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: ASSISTANT_COPY.deleteRefused })
    expect(await assistantsOf(w.teamId, w.owner)).toHaveLength(1)
  })
})

describe.skipIf(!ready.ok)(`agents.update keeps a rotated conversation login (EXP-849)`, () => {
  it(`a sortOrder-only update leaves a conversation's device-rotated account alone; an explicit pick writes it`, async () => {
    const w = await world()
    const box = await w.device(w.owner, `server`, 1)
    const [bot] = await db
      .insert(agents)
      .values({ teamId: w.teamId, name: `Triage`, deviceId: box, ownerId: w.owner, account: `home` })
      .returning()
    const convo = (await db.transaction((tx) => conversationInTx(tx, bot!, w.owner))).session
    // The device rotated the conversation onto another login.
    await db.update(sessions).set({ account: `work` }).where(eq(sessions.id, convo.id))
    const asOwner = agentsRouter.createCaller(person(w.owner) as never)
    await asOwner.update({ id: bot!.id, sortOrder: 3 })
    await asOwner.update({ id: bot!.id, name: `Triage bot` })
    const account = async () => (await db.select().from(sessions).where(eq(sessions.id, convo.id)))[0]!.account
    expect(await account()).toBe(`work`)
    expect((await db.select().from(agents).where(eq(agents.id, bot!.id)))[0]).toMatchObject({ account: `home`, sortOrder: 3 })
    await asOwner.update({ id: bot!.id, account: `home` })
    expect(await account()).toBe(`home`)
  })
})

describe.skipIf(!ready.ok)(`another member's Assistant is never an assignee (S-3)`, () => {
  it(`assertAgentInTeam refuses it for anyone but its owner; onIssueAssignedInTx starts no turn on it`, async () => {
    const w = await world()
    const desktop = await w.device(w.mate, `desktop`, 1)
    // Shared with the team: only the role rule keeps the owner's turn off it.
    await db.update(devices).set({ sharedTeamIds: [w.teamId] }).where(eq(devices.deviceId, desktop))
    const assistant = (await ensureAssistant(w.mate, w.teamId))!
    await expect(assertAgentInTeam(db, assistant.id, w.teamId, w.owner)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(assertAgentInTeam(db, assistant.id, w.teamId, w.mate)).resolves.toBeUndefined()
    const [board] = await db
      .insert(boards)
      .values({ teamId: w.teamId, name: `Core`, slug: `core-${id8()}`, prefix: `AS` })
      .returning({ id: boards.id })
    const [issue] = await db
      .insert(issues)
      .values({ boardId: board!.id, teamId: w.teamId, title: `Theirs`, creatorId: w.owner })
      .returning({ id: issues.id })
    await inRollback(async (tx) => {
      await onIssueAssignedInTx(tx as never, {
        issueId: issue!.id,
        teamId: w.teamId,
        actorUserId: w.owner,
        assigneeId: null,
        assigneeAgentId: assistant.id,
      })
      // No conversation for the owner with the mate's Assistant.
      const turns = await tx
        .select({ id: sessions.id })
        .from(sessions)
        .where(and(eq(sessions.agentId, assistant.id), eq(sessions.userId, w.owner)))
      expect(turns).toHaveLength(0)
    })
  })
})

// Migration 0208's DATA statements (everything after the DDL), replayed in
// a rolled-back transaction over rows shaped like before it.
const MIGRATION = readFileSync(join(__dirname, `../../db/out/0208_s3_assistant.sql`), `utf8`)
  .split(`--> statement-breakpoint`)
  .map((statement) => statement.trim())
  .filter((statement) => /^(--[^\n]*\n)*UPDATE/.test(statement))

// A conversation session of `userId` with the agent.
const conversation = async (
  tx: RealDbTx,
  teamId: string,
  agentId: string,
  userId: string,
  deviceId: string
) => {
  const id = crypto.randomUUID()
  await tx.insert(sessions).values({ id, rootId: id, teamId, agentId, userId, deviceId, kind: `agent` } as never)
}

describe.skipIf(!ready.ok)(`migration 0208 data step (S-3)`, () => {
  it(`has the data statements`, () => {
    expect(MIGRATION.length).toBe(4)
  })

  it(`leaves a TEAM agent alone: another member's conversation or an event trigger keeps it role 'agent'`, async () => {
    const w = await world()
    const ownerDesktop = await w.device(w.owner, `desktop`, 1)
    await inRollback(async (tx) => {
      const triggers = [
        { id: crypto.randomUUID(), kind: `schedule` as const, cron: `30 8 * * 1`, tz: `Europe/Vienna`, enabled: true },
        { id: crypto.randomUUID(), kind: `schedule` as const, cron: `*/15 * * * *`, enabled: false },
      ]
      const [shared] = await tx
        .insert(agents)
        .values({ teamId: w.teamId, ownerId: w.owner, deviceId: ownerDesktop, name: `Triage`, icon: `bot`, color: `#ff0000`, triggers })
        .returning()
      // The mate talks to the owner's agent: it is a team agent.
      await conversation(tx, w.teamId, shared!.id, w.mate, ownerDesktop)
      const third = await user()
      await tx.insert(teamMembers).values({ teamId: w.teamId, userId: third, role: `member` })
      const eventTriggers = [{ id: crypto.randomUUID(), kind: `event` as const, name: `issue.created`, enabled: true }]
      const [evented] = await tx
        .insert(agents)
        .values({ teamId: w.teamId, ownerId: third, deviceId: ownerDesktop, name: `Watcher`, icon: `eye`, triggers: eventTriggers })
        .returning()
      await tx.insert(userVoiceSettings).values([
        { userId: w.owner, agentId: shared!.id, voiceDeviceId: ownerDesktop },
        { userId: third, agentId: evented!.id, voiceDeviceId: ownerDesktop },
      ])

      for (const statement of MIGRATION) await tx.execute(sql.raw(statement))

      const row = async (id: string) => (await tx.select().from(agents).where(eq(agents.id, id)))[0]!
      expect(await row(shared!.id)).toMatchObject({ role: `agent`, icon: `bot`, color: `#ff0000`, triggers })
      expect(await row(evented!.id)).toMatchObject({ role: `agent`, icon: `eye`, triggers: eventTriggers })
      const voice = await tx.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, w.owner))
      expect(voice[0]).toMatchObject({ agentId: null, voiceDeviceId: null })
    })
  })

  it(`maps an enabled preset schedule to that preset and keeps its tz`, async () => {
    const w = await world()
    const desktop = await w.device(w.mate, `desktop`, 1)
    await inRollback(async (tx) => {
      const [own] = await tx
        .insert(agents)
        .values({
          teamId: w.teamId,
          ownerId: w.mate,
          deviceId: desktop,
          name: `Voice`,
          triggers: [
            { id: crypto.randomUUID(), kind: `schedule`, cron: `*/5 * * * *`, enabled: true },
            { id: crypto.randomUUID(), kind: `schedule`, cron: `0 9 * * *`, tz: `Europe/Vienna`, enabled: true },
          ],
        })
        .returning()
      // Its owner's own conversation does not make it a team agent.
      await conversation(tx, w.teamId, own!.id, w.mate, desktop)
      await tx.insert(userVoiceSettings).values({ userId: w.mate, agentId: own!.id, voiceDeviceId: desktop })

      for (const statement of MIGRATION) await tx.execute(sql.raw(statement))

      const [after] = await tx.select().from(agents).where(eq(agents.id, own!.id))
      expect(after).toMatchObject({ role: `assistant` })
      expect(after!.triggers).toEqual([
        { id: expect.any(String), kind: `schedule`, cron: `0 9 * * *`, tz: `Europe/Vienna`, enabled: true },
      ])
    })
  })

  it(`makes a member's own voice agent their Assistant and re-derives the mirror`, async () => {
    const w = await world()
    const desktop = await w.device(w.mate, `desktop`, 1)
    const phone = await w.device(w.owner, `phone`, 1)
    await inRollback(async (tx) => {
      const insert = async (values: Partial<typeof agents.$inferInsert> & { ownerId: string; deviceId: string }) =>
        (
          await tx
            .insert(agents)
            .values({ teamId: w.teamId, name: `Voice`, icon: `bot`, color: `#ff0000`, ...values })
            .returning()
        )[0]!
      // The mate's own voice agent with a paused schedule and an enabled
      // custom cron: becomes their Assistant on the hourly preset.
      const mine = await insert({
        ownerId: w.mate,
        deviceId: desktop,
        triggers: [
          { id: crypto.randomUUID(), kind: `schedule`, cron: `0 9 * * *`, enabled: false },
          { id: crypto.randomUUID(), kind: `schedule`, cron: `*/5 * * * *`, enabled: true },
        ],
      })
      // The owner's voice agent is SOMEONE ELSE's agent: no Assistant, no voice.
      const foreign = await insert({ ownerId: w.mate, deviceId: desktop, name: `Shared` })
      // The owner's own agent on a phone: an Assistant, but no voice device.
      const ownersOwn = await insert({
        ownerId: w.owner,
        deviceId: phone,
        triggers: [{ id: crypto.randomUUID(), kind: `schedule`, cron: `0 * * * *`, enabled: false }],
      })
      const third = await user()
      await tx.insert(teamMembers).values({ teamId: w.teamId, userId: third, role: `member` })
      await tx.insert(userVoiceSettings).values([
        { userId: w.mate, agentId: mine.id, voiceDeviceId: desktop, wakeWord: true },
        { userId: third, agentId: foreign.id, voiceDeviceId: desktop },
        { userId: w.owner, agentId: ownersOwn.id, voiceDeviceId: phone },
      ])

      for (const statement of MIGRATION) await tx.execute(sql.raw(statement))

      const row = async (id: string) => (await tx.select().from(agents).where(eq(agents.id, id)))[0]!
      const voice = async (userId: string) =>
        (await tx.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, userId)))[0]!

      expect(await row(mine.id)).toMatchObject({ role: `assistant`, icon: null, color: null })
      expect((await row(mine.id)).triggers).toEqual([
        expect.objectContaining({ kind: `schedule`, cron: `0 * * * *`, enabled: true }),
      ])
      expect(await voice(w.mate)).toMatchObject({ agentId: mine.id, voiceDeviceId: desktop, wakeWord: true })

      expect(await row(foreign.id)).toMatchObject({ role: `agent`, icon: `bot` })
      expect(await voice(third)).toMatchObject({
        agentId: null,
        voiceDeviceId: null,
      })

      expect(await row(ownersOwn.id)).toMatchObject({ role: `assistant` })
      expect((await row(ownersOwn.id)).triggers).toEqual([])
      expect(await voice(w.owner)).toMatchObject({ agentId: ownersOwn.id, voiceDeviceId: null })
    })
  })
})
