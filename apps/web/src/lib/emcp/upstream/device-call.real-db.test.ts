// @vitest-environment node
// #VAPP-239 (eApps M2, @exp/emcp upstream.json `device`): a device-routed
// upstream call against the REAL migrated Postgres. The relay is stubbed as
// the device: it claims the call over `claimDeviceCall` (as its owner, with
// its steer device id) and seals its answer to the reply key. Claims refuse
// another member, another device, a second claim and an expired call alike.
import { afterAll, describe, expect, it } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { devices, mcpDeviceCalls, mcpServers, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { pushSeal } from "@/lib/push-seal"
import { callOnDevice, claimDeviceCall, loadRoutedServer, pruneDeviceCalls, routeServer } from "@/lib/emcp/upstream/device-call"

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'mcp_device_calls') AS ok
`)
if (!ready.ok) console.info(`[device-call.real-db] skipped: ${ready.reason}`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `device-call-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@device.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function world() {
  const owner = await user()
  const other = await user()
  const [team] = await db.insert(teams).values({ name: `Device`, slug: `device-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  made.teams.push(teamId)
  await db.insert(teamMembers).values([
    { teamId, userId: owner, role: `owner` },
    { teamId, userId: other, role: `member` },
  ])
  const steerDeviceId = `steer-${id8()}`
  const [device] = await db
    .insert(devices)
    .values({ userId: owner, deviceId: steerDeviceId, label: `homelab`, kind: `server` })
    .returning({ id: devices.id })
  const [server] = await db
    .insert(mcpServers)
    .values({ teamId, name: `ha`, transport: `stdio`, command: `ha-mcp`, args: [`--lan`], auth: `none`, deviceId: device!.id })
    .returning({ id: mcpServers.id })
  return { owner, other, teamId, steerDeviceId, deviceId: device!.id, serverId: server!.id }
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id = ANY(${made.users})`)
})

describe.skipIf(!ready.ok)(`device-routed upstream calls (real db)`, () => {
  it(`routes the owner's call to their device, which claims it once and answers sealed`, async () => {
    const w = await world()
    const server = await loadRoutedServer(w.teamId, `ha`)
    expect(server?.device).toEqual({ id: w.deviceId, ownerId: w.owner, label: `homelab`, steerDeviceId: w.steerDeviceId })
    expect(routeServer(server!, w.owner)).toEqual({ route: `device`, deviceId: w.deviceId })
    expect(routeServer(server!, w.other)).toMatchObject({ error: `deviceNotOwned` })

    const seen: { refusals: unknown[]; claimed: unknown } = { refusals: [], claimed: null }
    const out = await callOnDevice(
      { userId: w.owner, server: server as never, tool: `get_state`, args: { entity: `sensor.kitchen` }, veto: { kind: `read`, sources: [`upstream:ha/*`] } },
      {
        relay: async (callId, device) => {
          expect(device.steerDeviceId).toBe(w.steerDeviceId)
          // Another member, another device: the same miss.
          seen.refusals.push(await claimDeviceCall({ callId, userId: w.other, steerDeviceId: w.steerDeviceId }))
          seen.refusals.push(await claimDeviceCall({ callId, userId: w.owner, steerDeviceId: `elsewhere` }))
          const claimed = await claimDeviceCall({ callId, userId: w.owner, steerDeviceId: w.steerDeviceId })
          seen.claimed = claimed
          // Once only.
          seen.refusals.push(await claimDeviceCall({ callId, userId: w.owner, steerDeviceId: w.steerDeviceId }))
          const answer = { ok: true, result: { content: [{ type: `text`, text: `21.5` }] } }
          const sealed = pushSeal(new Uint8Array(Buffer.from(claimed!.replyPublic, `base64url`)), new TextEncoder().encode(JSON.stringify(answer)))
          return { delivered: true, sealed: Buffer.from(sealed).toString(`base64url`) }
        },
      }
    )
    expect(out).toEqual({ result: { content: [{ type: `text`, text: `21.5` }] } })
    expect(seen.refusals).toEqual([null, null, null])
    expect(seen.claimed).toMatchObject({
      server: { name: `ha`, transport: `stdio`, command: `ha-mcp`, args: [`--lan`], env: [] },
      skipped: null,
      tool: `get_state`,
      args: { entity: `sensor.kitchen` },
      veto: { kind: `read`, sources: [`upstream:ha/*`] },
    })
    // The call row is gone once answered.
    expect(await db.select().from(mcpDeviceCalls).where(eq(mcpDeviceCalls.userId, w.owner))).toEqual([])
  })

  it(`an offline device reads as upstreamFailed with its label; a forged answer does not open`, async () => {
    const w = await world()
    const server = (await loadRoutedServer(w.teamId, `ha`))!
    const input = { userId: w.owner, server: server as never, tool: `t`, args: {}, veto: { kind: `call` as const, access: `read` } }
    expect(await callOnDevice(input, { relay: async () => ({ delivered: false, reason: `device_offline` }) })).toEqual({
      error: `upstreamFailed`,
      message: `ha runs on homelab, offline`,
      offline: `homelab`,
    })
    const forged = pushSeal(crypto.getRandomValues(new Uint8Array(32)), new TextEncoder().encode(`{"ok":true,"result":{}}`))
    expect(await callOnDevice(input, { relay: async () => ({ delivered: true, sealed: Buffer.from(forged).toString(`base64url`) }) })).toEqual({
      error: `upstreamFailed`,
      message: `ha: homelab did not answer`,
    })
  })

  it(`an expired call is never claimed and the watchdog prunes it`, async () => {
    const w = await world()
    const [row] = await db
      .insert(mcpDeviceCalls)
      .values({
        teamId: w.teamId,
        userId: w.owner,
        serverId: w.serverId,
        deviceId: w.deviceId,
        tool: `t`,
        args: {},
        veto: { kind: `call`, access: `read` },
        replyPublic: `x`.repeat(43),
        expiresAt: new Date(Date.now() - 1000),
      })
      .returning({ id: mcpDeviceCalls.id })
    expect(await claimDeviceCall({ callId: row!.id, userId: w.owner, steerDeviceId: w.steerDeviceId })).toBeNull()
    expect(await pruneDeviceCalls()).toBeGreaterThanOrEqual(1)
    expect(await db.select().from(mcpDeviceCalls).where(eq(mcpDeviceCalls.id, row!.id))).toEqual([])
  })
})
