// @vitest-environment node
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { devices, installs, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"

// #PERF-6: `sessionKeys.pending` against the REAL migrated Postgres, inside a
// rolled-back transaction. A desktop holding the wraps of many trees (the
// steady state: nothing pending) must answer in a fixed number of queries,
// not ~6 sequential ones per held tree (2 s averages in production).

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_session_keys_install') AS ok`)
if (!ready.ok) console.info(`[session-keys.real-db] skipped: ${ready.reason}`)

const { sessionKeysRouter } = await import(`@/lib/trpc/session-keys`)

const ROOTS = Number(process.env.PERF6_ROOTS ?? 300)
const id8 = () => crypto.randomUUID().slice(0, 8)
const key = (n: number) => Buffer.alloc(32, n % 256).toString(`base64url`)
const WRAP = Buffer.alloc(81, 7).toString(`base64url`)

async function world(tx: RealDbTx) {
  const users: string[] = []
  for (let n = 0; n < 4; n++) {
    const userId = `perf6-${id8()}`
    await tx.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${userId}, 'Tester', ${`${userId}@perf6.test`}, true, false, now(), now())
    `)
    users.push(userId)
  }
  const [team] = await tx.insert(teams).values({ name: `Perf`, slug: `perf-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  await tx.insert(teamMembers).values(users.map((userId, n) => ({ teamId, userId, role: n === 0 ? (`owner` as const) : (`member` as const) })))
  const [device] = await tx
    .insert(devices)
    .values({ userId: users[0]!, deviceId: `perf6-box-${id8()}`, label: `Box`, kind: `desktop` })
    .returning({ id: devices.id, deviceId: devices.deviceId })
  const mk = (userId: string, kind: string, deviceId: string | null = null) => ({
    userId,
    kind,
    deviceId,
    endpointId: `perf6-${id8()}-${id8()}`,
    x25519Public: key(users.indexOf(userId) * 7 + kind.length),
  })
  const rows = await tx
    .insert(installs)
    .values([mk(users[0]!, `desktop`, device!.id), ...users.flatMap((userId) => [mk(userId, `ios`), mk(userId, `web`)])])
    .returning({ id: installs.id, userId: installs.userId })
  const holder = rows[0]!.id
  const roots: Array<{ id: string; readers: string[] }> = []
  for (let n = 0; n < ROOTS; n++) {
    const id = crypto.randomUUID()
    const shared = n % 2 === 0
    await tx.insert(sessions).values({
      id,
      rootId: id,
      teamId,
      userId: users[0]!,
      deviceId: device!.deviceId,
      shared,
      sealPublicKey: key(n),
      sealEpoch: 1,
    } as never)
    const children = Array.from({ length: 3 }, () => crypto.randomUUID())
    for (const child of children) {
      await tx.insert(sessions).values({
        id: child,
        rootId: id,
        parentId: id,
        depth: 1,
        teamId,
        userId: users[0]!,
        deviceId: device!.deviceId,
        shared,
      } as never)
    }
    const readers = rows.filter((row) => shared || row.userId === users[0]).map((row) => row.id)
    roots.push({ id, readers })
  }
  // Every reader wrapped except the LAST tree's newest member installs.
  const wraps = roots.flatMap((root, n) =>
    root.readers
      .filter((installId) => n !== roots.length - 1 || installId === holder || rows.find((r) => r.id === installId)!.userId === users[0])
      .map((installId) => ({ rootId: root.id, epoch: 1, installId, wrapped: WRAP, createdByInstallId: holder }))
  )
  for (let at = 0; at < wraps.length; at += 1000) await tx.insert(sessionKeys).values(wraps.slice(at, at + 1000))
  const missing = roots.at(-1)!.readers.filter((installId) => !wraps.some((w) => w.rootId === roots.at(-1)!.id && w.installId === installId))
  return { holder, owner: users[0]!, missing, lastRoot: roots.at(-1)!.id }
}

describe.skipIf(!ready.ok)(`sessionKeys.pending on a busy holder (#PERF-6)`, () => {
  it(`lists only the missing pairs, in a fixed number of queries`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      let queries = 0
      const counting = new Proxy(tx, {
        get(target, prop, receiver) {
          if (prop === `select`) queries++
          const value = Reflect.get(target, prop, receiver)
          return typeof value === `function` ? value.bind(target) : value
        },
      })
      const caller = sessionKeysRouter.createCaller({
        session: { user: { id: w.owner }, session: { id: `s` } },
        db: counting,
        request: new Request(`http://x.test`),
      } as never)
      const input = { installId: w.holder }
      await caller.pending(input)
      const times: number[] = []
      for (let n = 0; n < 5; n++) {
        queries = 0
        const started = performance.now()
        const pairs = await caller.pending(input)
        times.push(performance.now() - started)
        expect(pairs.map((pair) => pair.targetInstallId).sort()).toEqual([...w.missing].sort())
        expect(pairs.every((pair) => pair.rootId === w.lastRoot && pair.epoch === 1)).toBe(true)
      }
      times.sort((a, b) => a - b)
      process.stderr.write(`[PERF-6] pending over ${ROOTS} held trees: median ${times[2]!.toFixed(1)} ms, ${queries} queries\n`)
      expect(queries).toBeLessThanOrEqual(5 + 6 * Math.ceil(ROOTS / 100))
    })
  }, 120_000)
})
