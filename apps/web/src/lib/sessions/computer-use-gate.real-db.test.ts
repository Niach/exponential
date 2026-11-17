// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, sessions, teamMembers, teams } from "@/db/schema"
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
const { COMPUTER_USE_MCP_REFUSAL, COMPUTER_USE_SWITCH_CAP, dropComputerUseOnDevice } = await import(
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

  it(`an explicit true is refused from an opted-out caller, a root and a child alike; false always works`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    const on = await w.start()
    await expect(w.start({ computerUse: true }, { caller: off })).rejects.toThrow(COMPUTER_USE_MCP_REFUSAL)
    await expect(
      sendSession({
        userId: w.owner,
        input: { parentId: off, computerUse: true, text: `part` },
        viaMcp: true,
        callerSessionId: off,
      })
    ).rejects.toThrow(COMPUTER_USE_MCP_REFUSAL)
    expect(await stored(await w.start({ computerUse: true }, { caller: on }))).toBe(true)
    expect(await stored(await w.start({ computerUse: false }, { caller: on }))).toBe(false)
  })

  it(`configure over MCP lowers but never raises, on itself or another session`, async () => {
    const w = await world({ on: true })
    const off = await w.start({ computerUse: false })
    const on = await w.start()
    const configure = (sessionId: string, caller: string, computerUse: boolean) =>
      sendSession({ userId: w.owner, input: { sessionId, computerUse }, viaMcp: true, callerSessionId: caller })
    await expect(configure(off, off, true)).rejects.toThrow(COMPUTER_USE_MCP_REFUSAL)
    await expect(configure(off, on, true)).rejects.toThrow(COMPUTER_USE_MCP_REFUSAL)
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
