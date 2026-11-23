// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { and, eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, messages, sessions, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// #EXP-1899: the device's Computer use switch is the gate, against the REAL
// migrated Postgres: what `sessions.send` and `sessions.configure` store per
// caller (a person, a session over MCP), and the clamp for a device build
// without the `computer-use-switch` cap (composer-menu.json `computerUse`).

vi.mock(`@/lib/pull-requests`, () => ({ syncPullRequestBodies: async () => {} }))
vi.mock(`@/lib/integrations/notifications`, () => ({
  notifySessionAsk: async () => {},
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns
    WHERE table_name = 'sessions' AND column_name = 'computer_use') AS ok
`)
if (!ready.ok) console.info(`[computer-use-gate.real-db] skipped: ${ready.reason}`)

const { sendSession } = await import(`@/lib/sessions/send`)
const { configureSession } = await import(`@/lib/sessions/configure`)
const { COMPUTER_USE_REFUSALS, COMPUTER_USE_SWITCH_CAP, computerUseStartLog, dropComputerUseOnDevice } = await import(
  `@/lib/sessions/computer-use`
)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

/** One member, one team, one device: `gate` = the build applies the gate
 *  itself (the cap), `on` = the server's copy of its switch. */
async function world(opts: { gate?: boolean; on?: boolean } = {}) {
  const owner = `cu-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${owner}, 'Tester', ${`${owner}@sessions.test`}, true, false, now(), now())
  `)
  made.users.push(owner)
  const [team] = await db.insert(teams).values({ name: `CU`, slug: `cu-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values({ teamId, userId: owner, role: `owner` })
  const device = async (gate: boolean, on: boolean) => {
    const deviceId = `dev-${id8()}`
    await db.insert(devices).values({
      userId: owner,
      deviceId,
      label: `box`,
      kind: `server`,
      caps: gate ? [COMPUTER_USE_SWITCH_CAP] : [],
      launchDefaults: { computerUse: on },
    })
    return deviceId
  }
  const deviceId = await device(opts.gate ?? true, opts.on ?? false)
  const start = async (input: Record<string, unknown> = {}, mcp?: { caller: string | null }) =>
    (
      await sendSession({
        userId: owner,
        input: { teamId, deviceId, text: `go`, ...input } as never,
        ...(mcp ? { viaMcp: true, callerSessionId: mcp.caller } : {}),
      })
    ).sessionId
  return { owner, teamId, deviceId, device, start }
}

const stored = async (id: string) =>
  (await db.select({ computerUse: sessions.computerUse }).from(sessions).where(eq(sessions.id, id)).limit(1))[0]!
    .computerUse

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe.skipIf(!ready.ok)(`computer use: a person's start`, () => {
  it(`without the key follows the device's switch, whatever the switch reads now`, async () => {
    const w = await world({ on: false })
    expect(await stored(await w.start())).toBe(true)
  })

  it(`an explicit false opts out and its child inherits that`, async () => {
    const w = await world({ on: true })
    const root = await w.start({ computerUse: false })
    expect(await stored(root)).toBe(false)
    const child = await sendSession({ userId: w.owner, input: { parentId: root, text: `part` } })
    expect(await stored(child.sessionId)).toBe(false)
  })
})

describe.skipIf(!ready.ok)(`computer use: a session over MCP cannot raise it`, () => {
  it(`a root or child it starts without the key takes the starter's value`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    const on = await w.start()
    expect(await stored(await w.start({}, { caller: off }))).toBe(false)
    expect(await stored(await w.start({}, { caller: on }))).toBe(true)
    const child = await sendSession({
      userId: w.owner,
      input: { parentId: off, text: `part` },
      viaMcp: true,
      callerSessionId: off,
    })
    expect(await stored(child.sessionId)).toBe(false)
  })

  it(`a start with no session to inherit from is off`, async () => {
    const w = await world({ on: true })
    expect(await stored(await w.start({}, { caller: null }))).toBe(false)
  })

  it(`an explicit true on a device whose switch is off is refused from an opted-out caller, a root and a child alike; false always works`, async () => {
    const w = await world({ on: false })
    const off = await w.start({ computerUse: false })
    const on = await w.start()
    await expect(w.start({ computerUse: true }, { caller: off })).rejects.toThrow(COMPUTER_USE_REFUSALS.switch)
    await expect(
      sendSession({
        userId: w.owner,
        input: { parentId: off, computerUse: true, text: `part` },
        viaMcp: true,
        callerSessionId: off,
      })
    ).rejects.toThrow(COMPUTER_USE_REFUSALS.switch)
    expect(await stored(await w.start({ computerUse: true }, { caller: on }))).toBe(true)
    expect(await stored(await w.start({ computerUse: false }, { caller: on }))).toBe(false)
  })

  it(`configure over MCP lowers but never raises, on itself or another session`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    const on = await w.start()
    const configure = (sessionId: string, caller: string, computerUse: boolean) =>
      sendSession({ userId: w.owner, input: { sessionId, computerUse }, viaMcp: true, callerSessionId: caller })
    await expect(configure(off, off, true)).rejects.toThrow(COMPUTER_USE_REFUSALS.running)
    await expect(configure(off, on, true)).rejects.toThrow(COMPUTER_USE_REFUSALS.running)
    expect(await stored(off)).toBe(false)
    await configure(on, on, false)
    expect(await stored(on)).toBe(false)
  })

  it(`a person turns it back on, by a message and by configure`, async () => {
    const w = await world({ on: true })
    const a = await w.start({ computerUse: false })
    await sendSession({ userId: w.owner, input: { sessionId: a, computerUse: true } })
    expect(await stored(a)).toBe(true)
    const b = await w.start({ computerUse: false })
    await configureSession(w.owner, { sessionId: b, computerUse: true })
    expect(await stored(b)).toBe(true)
  })
})

// #FEED-302 (composer-menu.json `computerUse.agentStart`): the device owner's
// own sessions start another with computer use; the switch stays the gate.
describe.skipIf(!ready.ok)(`computer use: a session starts another with it over MCP`, () => {
  /** A second member of `teamId`; `shares` = their own device, shared with it. */
  const teammate = async (teamId: string, shares?: { on: boolean }) => {
    const id = `cu-test-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${id}, 'Mate', ${`${id}@sessions.test`}, true, false, now(), now())
    `)
    made.users.push(id)
    await db.insert(teamMembers).values({ teamId, userId: id, role: `member` })
    let deviceId: string | null = null
    if (shares) {
      deviceId = `dev-${id8()}`
      await db.insert(devices).values({
        userId: id,
        deviceId,
        label: `theirs`,
        kind: `server`,
        caps: [COMPUTER_USE_SWITCH_CAP],
        launchDefaults: { computerUse: shares.on },
        sharedTeamIds: [teamId],
      })
    }
    return { id, deviceId }
  }
  const share = (w: { owner: string; deviceId: string; teamId: string }) =>
    db
      .update(devices)
      .set({ sharedTeamIds: [w.teamId] })
      .where(and(eq(devices.deviceId, w.deviceId), eq(devices.userId, w.owner)))
  const logLines = async (sessionId: string) =>
    (
      await db
        .select({ body: messages.body, consumedAt: messages.consumedAt })
        .from(messages)
        .where(and(eq(messages.sessionId, sessionId), eq(messages.authorKind, `system`)))
    ).filter((row) => row.body?.startsWith(`Computer use`))

  it(`the owner's own session starts a root on their device, switch on, and the start is logged`, async () => {
    const w = await world({ on: true })
    // The Assistant's conversation and every MCP-started session are off.
    const off = await w.start({ computerUse: false })
    const info = vi.spyOn(console, `info`).mockImplementation(() => {})
    const started = await w.start({ computerUse: true }, { caller: off })
    expect(await stored(started)).toBe(true)
    expect(await stored(off)).toBe(false)
    const lines = await logLines(started)
    expect(lines).toHaveLength(1)
    expect(lines[0]!.body).toBe(computerUseStartLog(off))
    // A server line, never the new agent's input.
    expect(lines[0]!.consumedAt).not.toBeNull()
    expect(info.mock.calls.some(([line]) => String(line).startsWith(`[computer-use]`) && String(line).includes(started))).toBe(true)
    info.mockRestore()
  })

  it(`a child of an opted-out parent the same way, logged`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    const child = await sendSession({
      userId: w.owner,
      input: { parentId: off, computerUse: true, text: `part` },
      viaMcp: true,
      callerSessionId: off,
    })
    expect(await stored(child.sessionId)).toBe(true)
    expect(await logLines(child.sessionId)).toHaveLength(1)
  })

  it(`a start that raises nothing writes no log line`, async () => {
    const w = await world({ on: true })
    const on = await w.start()
    const off = await w.start({ computerUse: false })
    expect(await logLines(await w.start({ computerUse: true }, { caller: on }))).toHaveLength(0)
    expect(await logLines(await w.start({}, { caller: off }))).toHaveLength(0)
    expect(await logLines(await w.start({ computerUse: true }))).toHaveLength(0)
  })

  it(`the switch is the gate: off = refused, nothing started`, async () => {
    const w = await world({ on: false })
    const off = await w.start({ computerUse: false })
    const before = await db.select({ id: sessions.id }).from(sessions).where(eq(sessions.teamId, w.teamId))
    await expect(w.start({ computerUse: true }, { caller: off })).rejects.toThrow(COMPUTER_USE_REFUSALS.switch)
    const after = await db.select({ id: sessions.id }).from(sessions).where(eq(sessions.teamId, w.teamId))
    expect(after).toHaveLength(before.length)
  })

  it(`a device build without the gate cap: the server's copy of the switch decides the same`, async () => {
    const on = await world({ gate: false, on: true })
    expect(await stored(await on.start({ computerUse: true }, { caller: await on.start({ computerUse: false }) }))).toBe(true)
    const offBox = await world({ gate: false, on: false })
    await expect(
      offBox.start({ computerUse: true }, { caller: await offBox.start({ computerUse: false }) })
    ).rejects.toThrow(COMPUTER_USE_REFUSALS.switch)
  })

  it(`a connection outside a session never starts one with it`, async () => {
    const w = await world({ on: true })
    await expect(w.start({ computerUse: true }, { caller: null })).rejects.toThrow(COMPUTER_USE_REFUSALS.caller)
  })

  it(`another member's device is refused, shared and switched on or not, and its switch is never told`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    for (const on of [true, false]) {
      const mate = await teammate(w.teamId, { on })
      await expect(w.start({ computerUse: true, deviceId: mate.deviceId }, { caller: off })).rejects.toThrow(
        COMPUTER_USE_REFUSALS.owner
      )
      await expect(
        sendSession({
          userId: w.owner,
          input: { parentId: off, deviceId: mate.deviceId, computerUse: true, text: `part` } as never,
          viaMcp: true,
          callerSessionId: off,
        })
      ).rejects.toThrow(COMPUTER_USE_REFUSALS.owner)
    }
  })

  it(`another member's session on the owner's shared device is refused, with its own key and with the host's`, async () => {
    const w = await world({ on: true })
    await share(w)
    const mate = await teammate(w.teamId)
    const theirs = (
      await sendSession({
        userId: mate.id,
        input: { teamId: w.teamId, deviceId: w.deviceId, computerUse: false, text: `go` } as never,
      })
    ).sessionId
    for (const key of [mate.id, w.owner]) {
      await expect(
        sendSession({
          userId: key,
          input: { teamId: w.teamId, deviceId: w.deviceId, computerUse: true, text: `go` } as never,
          viaMcp: true,
          callerSessionId: theirs,
        })
      ).rejects.toThrow(COMPUTER_USE_REFUSALS.owner)
      await expect(
        sendSession({
          userId: key,
          input: { parentId: theirs, computerUse: true, text: `part` } as never,
          viaMcp: true,
          callerSessionId: theirs,
        })
      ).rejects.toThrow(COMPUTER_USE_REFUSALS.owner)
    }
  })

  it(`the owner's session inside a tree another member owns is refused`, async () => {
    const w = await world({ on: true })
    await share(w)
    const mate = await teammate(w.teamId)
    // The teammate's shared tree on the owner's box; the owner starts a part in it.
    const theirRoot = (
      await sendSession({
        userId: mate.id,
        input: { teamId: w.teamId, deviceId: w.deviceId, computerUse: false, text: `go` } as never,
      })
    ).sessionId
    const ownPart = (
      await sendSession({ userId: w.owner, input: { parentId: theirRoot, computerUse: false, text: `part` } })
    ).sessionId
    await expect(w.start({ computerUse: true }, { caller: ownPart })).rejects.toThrow(COMPUTER_USE_REFUSALS.owner)
  })

  it(`a running session is still never raised over MCP, also by the owner's own session`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    await expect(
      sendSession({ userId: w.owner, input: { sessionId: off, computerUse: true }, viaMcp: true, callerSessionId: off })
    ).rejects.toThrow(COMPUTER_USE_REFUSALS.running)
    expect(await stored(off)).toBe(false)
  })
})

// A build without the cap lets the row win over its switch: the server
// stores the effective value for it.
describe.skipIf(!ready.ok)(`computer use: a device build without the gate`, () => {
  it(`stores the pick AND the server's copy of the switch`, async () => {
    const offBox = await world({ gate: false, on: false })
    expect(await stored(await offBox.start())).toBe(false)
    expect(await stored(await offBox.start({ computerUse: true }))).toBe(false)
    const onBox = await world({ gate: false, on: true })
    expect(await stored(await onBox.start())).toBe(true)
    expect(await stored(await onBox.start({ computerUse: false }))).toBe(false)
  })

  it(`a person's pick on a live session is clamped the same way`, async () => {
    const w = await world({ gate: false, on: false })
    const a = await w.start()
    await sendSession({ userId: w.owner, input: { sessionId: a, computerUse: true } })
    expect(await stored(a)).toBe(false)
    await configureSession(w.owner, { sessionId: a, computerUse: true })
    expect(await stored(a)).toBe(false)
  })

  it(`a child on such a device does not inherit past its switch`, async () => {
    const w = await world({ gate: true, on: true })
    const root = await w.start()
    const oldBox = await w.device(false, false)
    const child = await sendSession({ userId: w.owner, input: { parentId: root, deviceId: oldBox, text: `part` } })
    expect(await stored(child.sessionId)).toBe(false)
  })

  it(`switching off clears its live sessions and leaves the ended ones`, async () => {
    const w = await world({ gate: false, on: true })
    const live = await w.start()
    const ended = await w.start()
    await db.update(sessions).set({ status: `ended` }).where(eq(sessions.id, ended))
    const other = await world({ gate: false, on: true })
    const elsewhere = await other.start()
    await dropComputerUseOnDevice(db, w.deviceId)
    expect(await stored(live)).toBe(false)
    expect(await stored(ended)).toBe(true)
    expect(await stored(elsewhere)).toBe(true)
  })
})
