// @vitest-environment node
// #EXP-1935, against the REAL migrated Postgres: what voice.test.ts's stub
// (a `where()` that ignores its argument) cannot see. The call procedures'
// authorization (`memberAgent`'s team filter, the own-devices filters, the
// minted claims), the voice Assistant after a member left its team, and
// `agents.voice` under two writers. Rows are dated from ONE fixed instant.
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { verifySteerTicket } from "@exp/steer-ticket"
import { db } from "@/db/connection"
import { agents, devices, teamMembers, teams, userVoiceSettings } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

const h = vi.hoisted(() => ({
  userId: ``,
  /** Runs between `agents.update`'s read of the agent and its write. */
  beforeAgentWrite: null as null | (() => Promise<void>),
}))

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  getSteerRelayConfig: () => ({ url: `https://relay.example`, secret: `relay-secret` }),
  relayPostNudge: async () => ({ delivered: true }),
}))
vi.mock(`@/lib/auth/resolve-bearer`, () => ({ resolveSession: async () => ({ user: { id: h.userId } }) }))
vi.mock(`@/lib/storage/session-attachment-upload`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/storage/session-attachment-upload")>()),
  prepareSessionImageBytes: async (file: { filename: string; body: Uint8Array }) => {
    const attachmentId = crypto.randomUUID()
    return {
      attachmentId,
      filename: file.filename,
      contentType: `audio/wav`,
      sizeBytes: file.body.byteLength,
      storageKey: `test/${attachmentId}`,
      url: `/api/attachments/${attachmentId}`,
      width: null,
      height: null,
    }
  },
  rollbackSessionImage: async () => {},
}))
vi.mock(`@/lib/trpc`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/trpc")>()
  return {
    ...real,
    generateTxId: async (tx: Parameters<typeof real.generateTxId>[0]) => {
      await h.beforeAgentWrite?.()
      return real.generateTxId(tx)
    },
  }
})

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'uniq_agents_assistant') AS ok
`)
if (!ready.ok) console.info(`[voice.real-db] skipped: ${ready.reason}`)

const { voiceRouter } = await import(`@/lib/trpc/voice`)
const { agentsRouter } = await import(`@/lib/trpc/agents`)
const { teamMembersRouter } = await import(`@/lib/trpc/team-members`)
const { handleAgentVoiceClipUpload, handleAgentVoiceClipDelete } = await import(`@/lib/storage/agent-voice-clip-upload`)

const NOW = new Date(`2026-11-17T12:00:00.000Z`)
const minutesBefore = (minutes: number) => new Date(NOW.getTime() - minutes * 60_000)
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const voice = (userId: string) => voiceRouter.createCaller(ctx(userId))

async function user(): Promise<string> {
  const id = `voice-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@voice.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(members: Record<string, `owner` | `member`>): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Voice`, slug: `voice-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(Object.entries(members).map(([userId, role]) => ({ teamId: row!.id, userId, role })))
  return row!.id
}

/** A device that speaks, last seen at the fixed instant (online). */
async function device(userId: string, kind: `desktop` | `server`, sharedTeamIds: string[] = []): Promise<string> {
  const deviceId = `dev-${kind}-${id8()}`
  await db.insert(devices).values({
    userId,
    deviceId,
    label: deviceId,
    kind,
    sharedTeamIds,
    lastSeenAt: NOW,
    doctor: { checkedAt: NOW.toISOString(), items: [{ key: `speech`, group: `voice`, state: `ok` }] } as never,
  })
  return deviceId
}

async function agent(
  teamId: string,
  ownerId: string,
  deviceId: string,
  role: `assistant` | `agent` = `agent`,
  createdAt: Date = NOW
): Promise<string> {
  const [row] = await db
    .insert(agents)
    .values({ teamId, name: role, icon: null, color: null, instructions: ``, deviceId, ownerId, role, createdAt })
    .returning({ id: agents.id })
  return row!.id
}

const claimsOf = (ticket: string) => {
  const verdict = verifySteerTicket(ticket, `relay-secret`)
  expect(verdict.ok).toBe(true)
  return verdict.ok ? verdict.claims : null
}
const voiceOf = async (agentId: string) =>
  (await db.select({ voice: agents.voice }).from(agents).where(eq(agents.id, agentId)))[0]!.voice

beforeAll(() => {
  vi.useFakeTimers({ toFake: [`Date`] })
  vi.setSystemTime(NOW)
})

afterAll(async () => {
  vi.useRealTimers()
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe.skipIf(!ready.ok)(`#EXP-1935 the call procedures' authorization`, () => {
  it(`startCall rings the agent's own desktop and mints the member's claims`, async () => {
    const me = await user()
    const teamId = await team({ [me]: `owner` })
    const desktop = await device(me, `desktop`)
    const agentId = await agent(teamId, me, desktop)
    const out = await voice(me).startCall({ agentId, platform: `phone` })
    expect(out).toMatchObject({ deviceId: desktop, fallback: false })
    expect(claimsOf(out.ticket)).toMatchObject({
      role: `caller`,
      sub: me,
      callId: out.callId,
      agentId,
      deviceId: desktop,
      deviceOwnerId: me,
    })
  })

  it(`startCall refuses an agent of a team the member is not in`, async () => {
    const me = await user()
    const stranger = await user()
    await team({ [me]: `owner` })
    await device(me, `desktop`)
    const foreign = await agent(await team({ [stranger]: `owner` }), stranger, await device(stranger, `desktop`))
    await expect(voice(me).startCall({ agentId: foreign, platform: `phone` })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`startCall refuses a team-mate's Assistant`, async () => {
    const me = await user()
    const mate = await user()
    const teamId = await team({ [me]: `owner`, [mate]: `member` })
    await device(me, `desktop`)
    const theirs = await agent(teamId, mate, await device(mate, `desktop`), `assistant`)
    await expect(voice(me).startCall({ agentId: theirs, platform: `phone` })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`startCall never rings a team-mate's shared server: the member's own desktop, or refused`, async () => {
    const me = await user()
    const mate = await user()
    const teamId = await team({ [me]: `member`, [mate]: `owner` })
    const shared = await device(mate, `server`, [teamId])
    const agentId = await agent(teamId, mate, shared)
    await expect(voice(me).startCall({ agentId, platform: `phone` })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
    const desktop = await device(me, `desktop`)
    const out = await voice(me).startCall({ agentId, platform: `phone` })
    expect(out).toMatchObject({ deviceId: desktop, fallback: true })
    expect(claimsOf(out.ticket)).toMatchObject({ deviceId: desktop, deviceOwnerId: me })
  })

  it(`callHostTicket names only one of the member's own devices`, async () => {
    const me = await user()
    const mate = await user()
    await team({ [me]: `owner`, [mate]: `member` })
    const mine = await device(me, `desktop`)
    const theirs = await device(mate, `desktop`)
    const callId = crypto.randomUUID()
    await expect(voice(me).callHostTicket({ callId, deviceId: theirs })).rejects.toMatchObject({ code: `NOT_FOUND` })
    const out = await voice(me).callHostTicket({ callId, deviceId: mine })
    const claims = claimsOf(out.ticket)
    expect(claims).toMatchObject({ role: `call_host`, sub: me, callId, deviceId: mine })
    expect(claims?.deviceOwnerId).toBeUndefined()
    // Without a device id (an old IDE) and without a voice device: none.
    await expect(voice(me).callHostTicket({ callId })).rejects.toMatchObject({ code: `PRECONDITION_FAILED` })
  })
})

describe.skipIf(!ready.ok)(`#EXP-1935 the voice Assistant follows team membership`, () => {
  /** `actor` removes `userId` from the team (themselves = they leave). */
  async function remove(actor: string, teamId: string, userId: string) {
    const [membership] = await db
      .select({ id: teamMembers.id })
      .from(teamMembers)
      .where(and(eq(teamMembers.teamId, teamId), eq(teamMembers.userId, userId)))
    await teamMembersRouter.createCaller(ctx(actor)).remove({ memberId: membership!.id })
  }

  it(`after leaving its team, voice moves to the Assistant of a team the member is still in`, async () => {
    const me = await user()
    const boss = await user()
    const first = await team({ [me]: `member`, [boss]: `owner` })
    const second = await team({ [me]: `member`, [boss]: `owner` })
    const desktop = await device(me, `desktop`)
    const laptop = await device(me, `desktop`)
    const a1 = await agent(first, me, desktop, `assistant`, minutesBefore(60))
    const a2 = await agent(second, me, laptop, `assistant`, minutesBefore(30))
    await db.insert(userVoiceSettings).values({ userId: me, agentId: a1, voiceDeviceId: desktop, meetingStartedAt: NOW })
    expect(await voice(me).get()).toMatchObject({ agentId: a1, voiceDeviceId: desktop })

    await remove(me, first, me)

    expect(await voice(me).get()).toMatchObject({ agentId: a2, voiceDeviceId: laptop, meetingStartedAt: null })
    // The mirror (the heartbeat, old clients) moved with it; the meeting on
    // the old device stopped.
    const [mirror] = await db.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, me))
    expect(mirror).toMatchObject({ agentId: a2, voiceDeviceId: laptop, meetingStartedAt: null })
    // The Assistant they left is no longer theirs to pick.
    await expect(voice(me).update({ agentId: a1 })).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`leaving the only team with an Assistant clears voice`, async () => {
    const me = await user()
    const boss = await user()
    const only = await team({ [me]: `member`, [boss]: `owner` })
    const desktop = await device(me, `desktop`)
    const a1 = await agent(only, me, desktop, `assistant`)
    await db.insert(userVoiceSettings).values({ userId: me, agentId: a1, voiceDeviceId: desktop, wakeWord: true })
    await remove(boss, only, me)
    expect(await voice(me).get()).toMatchObject({ agentId: null, voiceDeviceId: null, wakeWord: false })
    const [mirror] = await db.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, me))
    expect(mirror).toMatchObject({ agentId: null, voiceDeviceId: null, wakeWord: false })
  })
})

describe.skipIf(!ready.ok)(`#EXP-1935 agents.voice under two writers`, () => {
  const wav = () => {
    const body = new Uint8Array(64)
    body.set(new TextEncoder().encode(`RIFF`), 0)
    body.set(new TextEncoder().encode(`WAVE`), 8)
    const form = new FormData()
    form.set(`file`, new File([body], `me.wav`, { type: `audio/wav` }))
    return form
  }
  const gate = () => {
    let open!: () => void
    let reached!: () => void
    const opened = new Promise<void>((resolve) => (open = resolve))
    const waiting = new Promise<void>((resolve) => (reached = resolve))
    return { open, waiting, pass: () => (reached(), opened) }
  }
  async function assistant(voiceValue: Record<string, string>) {
    const me = await user()
    const teamId = await team({ [me]: `owner` })
    const agentId = await agent(teamId, me, await device(me, `desktop`), `assistant`)
    await db.update(agents).set({ voice: voiceValue }).where(eq(agents.id, agentId))
    h.userId = me
    return { me, agentId }
  }
  const save = (me: string, agentId: string, patch: Record<string, string | null>) =>
    agentsRouter.createCaller(ctx(me)).update({ id: agentId, voice: patch })

  it(`a transcript saved while a clip uploads survives the upload`, async () => {
    const { me, agentId } = await assistant({ language: `de` })
    const body = gate()
    // The handler read the agent; its multipart body is still arriving.
    const request = { headers: new Headers(), formData: () => body.pass().then(wav) } as unknown as Request
    const upload = handleAgentVoiceClipUpload({ params: { agentId }, request })
    await body.waiting
    await save(me, agentId, { transcript: `hello there` })
    body.open()
    const { clipId } = (await (await upload).json()) as { clipId: string }
    expect(await voiceOf(agentId)).toEqual({ language: `de`, transcript: `hello there`, clipId, clipName: `me.wav` })
  })

  it(`a clip uploaded, and one removed, while a transcript saves both survive the save`, async () => {
    const { me, agentId } = await assistant({ language: `de` })
    const request = () => ({ headers: new Headers(), formData: async () => wav() }) as unknown as Request
    // The save read the agent; the upload commits before the save writes.
    let write = gate()
    h.beforeAgentWrite = write.pass
    let saving = save(me, agentId, { transcript: `one`, language: null })
    await write.waiting
    h.beforeAgentWrite = null
    const { clipId } = (await (await handleAgentVoiceClipUpload({ params: { agentId }, request: request() })).json()) as {
      clipId: string
    }
    write.open()
    await saving
    expect(await voiceOf(agentId)).toEqual({ transcript: `one`, clipId, clipName: `me.wav` })

    // And the other way: the clip is removed while a save holds the old row.
    write = gate()
    h.beforeAgentWrite = write.pass
    saving = save(me, agentId, { transcript: `two` })
    await write.waiting
    h.beforeAgentWrite = null
    await handleAgentVoiceClipDelete({ params: { agentId }, request: request() })
    write.open()
    await saving
    expect(await voiceOf(agentId)).toEqual({ transcript: `two` })
  })
})
