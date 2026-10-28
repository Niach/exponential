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
import { agents, devices, teamMembers, teams, userVoiceSettings } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"

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

// Migration 0208's DATA statements (everything after the DDL), replayed in
// a rolled-back transaction over rows shaped like before it.
const MIGRATION = readFileSync(join(__dirname, `../../db/out/0208_s3_assistant.sql`), `utf8`)
  .split(`--> statement-breakpoint`)
  .map((statement) => statement.trim())
  .filter((statement) => /^(--[^\n]*\n)*UPDATE/.test(statement))

describe.skipIf(!ready.ok)(`migration 0208 data step (S-3)`, () => {
  it(`has the data statements`, () => {
    expect(MIGRATION.length).toBe(4)
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
      // The mate's own voice agent with a paused + an event trigger and an
      // enabled schedule: becomes their Assistant on the hourly preset.
      const mine = await insert({
        ownerId: w.mate,
        deviceId: desktop,
        triggers: [
          { id: crypto.randomUUID(), kind: `schedule`, cron: `*/5 * * * *`, enabled: true },
          { id: crypto.randomUUID(), kind: `event`, name: `issue.created`, enabled: true },
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
        { userId: w.mate, agentId: mine.id, voiceDeviceId: desktop, speakReplies: true, wakeWord: true },
        { userId: third, agentId: foreign.id, voiceDeviceId: desktop, speakReplies: true, pushToTalk: true },
        { userId: w.owner, agentId: ownersOwn.id, voiceDeviceId: phone, speakReplies: true },
      ])

      for (const statement of MIGRATION) await tx.execute(sql.raw(statement))

      const row = async (id: string) => (await tx.select().from(agents).where(eq(agents.id, id)))[0]!
      const voice = async (userId: string) =>
        (await tx.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, userId)))[0]!

      expect(await row(mine.id)).toMatchObject({ role: `assistant`, icon: null, color: null })
      expect((await row(mine.id)).triggers).toEqual([
        expect.objectContaining({ kind: `schedule`, cron: `0 * * * *`, enabled: true }),
      ])
      expect(await voice(w.mate)).toMatchObject({ agentId: mine.id, voiceDeviceId: desktop, speakReplies: true, wakeWord: true })

      expect(await row(foreign.id)).toMatchObject({ role: `agent`, icon: `bot` })
      expect(await voice(third)).toMatchObject({
        agentId: null,
        voiceDeviceId: null,
        speakReplies: false,
        pushToTalk: false,
      })

      expect(await row(ownersOwn.id)).toMatchObject({ role: `assistant` })
      expect((await row(ownersOwn.id)).triggers).toEqual([])
      expect(await voice(w.owner)).toMatchObject({ agentId: ownersOwn.id, voiceDeviceId: null, speakReplies: false })
    })
  })
})
