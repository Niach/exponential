/**
 * Member spaces (#EXP-2161) on a SEEDED demo stack, for the captures of every
 * platform: a teammate's `unraid` server opted in for the demo team (Runs on's
 * team section), the demo user's own `Home server` shared, with Mira's
 * Assistant on it (Sharing › Members), and the demo user's Assistant as the
 * state asks:
 *
 *   bun scripts/seed-member-space.ts section   on their own machine (default)
 *   bun scripts/seed-member-space.ts waiting   on unraid, its install waits
 *   bun scripts/seed-member-space.ts ready     on unraid, let in
 *   bun scripts/seed-member-space.ts ended     its space ended: no device
 *   bun scripts/seed-member-space.ts off       as section, Home server not opted in
 *   bun scripts/seed-member-space.ts cannot    as off, and it cannot host members
 *
 * Run after `seed:screenshots`, on an isolated stack only: it writes.
 */
import { and, eq, inArray, isNotNull } from "drizzle-orm"
import { signApproval, signRegistration } from "@exp/domain-contract/install-trust"
import { MEMBER_SPACE } from "@exp/domain-contract/member-space"
import trust from "@exp/domain-contract/fixtures/install-trust.json" with { type: "json" }
import { db } from "@/db/connection"
import { agents, devices, installApprovals, installs, teams, users } from "@/db/schema"
import { DEMO_DEVICE_ID, DEMO_EMAIL, demoAgentReport } from "./screenshot-demo"
import { assertLocalDatabase, assertNotSharedDevDatabase } from "./lib/local-db-guard"

const CONSEQUENCE = `seed-member-space writes demo devices, spaces and installs.`
assertLocalDatabase(CONSEQUENCE)
assertNotSharedDevDatabase(CONSEQUENCE)

const state = process.argv[2] ?? `section`
const TEAM_HOST = `screenshot-demo-unraid`
const OWN_HOST = `screenshot-demo-home-server`
const SPACE_ID = `5bace000-0000-4000-8000-000000000001`
const MIRA_SPACE_ID = `5bace000-0000-4000-8000-000000000002`
const SPACE_DEVICE = `${MEMBER_SPACE.deviceIdPrefix}${SPACE_ID}`
const key = trust.space.keys.S

const userOf = async (email: string) => {
  const [row] = await db.select({ id: users.id }).from(users).where(eq(users.email, email)).limit(1)
  if (!row) throw new Error(`no ${email}: run seed:screenshots first`)
  return row.id
}
const demo = await userOf(DEMO_EMAIL)
const jonas = await userOf(`jonas@acme.dev`)
const mira = await userOf(`mira@acme.dev`)
const [team] = await db.select({ id: teams.id }).from(teams).where(eq(teams.slug, `acme`)).limit(1)
if (!team) throw new Error(`no acme team: run seed:screenshots first`)

// Online for a day: no daemon beats here.
const lastSeenAt = new Date(Date.now() + 86_400_000)
const optedIn = state !== `off` && state !== `cannot`
const doctor = {
  checkedAt: new Date().toISOString(),
  items: [
    { key: `git`, group: `required`, state: `ok`, detail: `2.49.0` },
    { key: `claude`, group: `agents`, state: `ok`, detail: `alex@northwind.dev` },
    state === `cannot`
      ? { key: MEMBER_SPACE.doctorKey, group: `storage`, state: `missing`, detail: `No user namespaces` }
      : { key: MEMBER_SPACE.doctorKey, group: `storage`, state: `ok` },
  ],
}
const { agentAccounts, agentUsage } = demoAgentReport(new Date())
const server = { kind: `server`, platform: `linux`, version: `0.14.22`, agents: [`claude`, `codex`], agentAccounts, agentUsage, lastSeenAt }

await db.delete(devices).where(and(isNotNull(devices.spaceHostId), inArray(devices.userId, [demo, mira])))
await db.delete(devices).where(inArray(devices.deviceId, [TEAM_HOST, OWN_HOST]))
await db.delete(installs).where(eq(installs.endpointId, key.endpointId))

const [teamHost] = await db
  .insert(devices)
  .values({ ...server, userId: jonas, deviceId: TEAM_HOST, label: `unraid`, sharedTeamIds: [team.id], assistantTeamIds: [team.id], doctor })
  .returning({ id: devices.id })
const [ownHost] = await db
  .insert(devices)
  .values({ ...server, userId: demo, deviceId: OWN_HOST, label: `Home server`, sharedTeamIds: [team.id], assistantTeamIds: optedIn ? [team.id] : [], doctor })
  .returning({ id: devices.id })
if (optedIn) {
  await db.insert(devices).values({
    id: MIRA_SPACE_ID,
    userId: mira,
    deviceId: `${MEMBER_SPACE.deviceIdPrefix}${MIRA_SPACE_ID}`,
    label: `Home server`,
    kind: `server`,
    platform: `linux`,
    spaceHostId: ownHost!.id,
    spaceTeamId: team.id,
    lastSeenAt,
  })
}

const bound = state === `waiting` || state === `ready`
if (bound) {
  await db.insert(devices).values({ ...server, id: SPACE_ID, userId: demo, deviceId: SPACE_DEVICE, label: `unraid`, spaceHostId: teamHost!.id, spaceTeamId: team.id })
  // The fixture's space install, registered by the demo user.
  const bytes = (text: string) => new Uint8Array((text.match(/../g) ?? []).map((b) => parseInt(b, 16)))
  const [install] = await db
    .insert(installs)
    .values({
      userId: demo,
      kind: MEMBER_SPACE.installKind,
      deviceId: SPACE_ID,
      label: `unraid`,
      platform: `linux`,
      endpointId: key.endpointId,
      x25519Public: key.x25519Public,
      registrationSignature: signRegistration(bytes(key.identity), demo, key.signedAt, `space`),
      registrationSignedAt: key.signedAt,
      approvalRequestedAt: new Date(),
    })
    .returning({ id: installs.id })
  if (state === `ready`) {
    const approver = trust.keys.A
    await db.insert(installApprovals).values({
      kind: MEMBER_SPACE.approvalKind,
      subjectInstallId: install!.id,
      subjectEndpointId: key.endpointId,
      approverInstallId: null,
      approverEndpointId: approver.endpointId,
      signature: signApproval(bytes(approver.identity), `space`, { userId: demo, endpointId: key.endpointId, x25519Public: key.x25519Public })!,
    })
  }
}
await db
  .update(agents)
  .set({ deviceId: bound ? SPACE_DEVICE : state === `ended` ? `` : DEMO_DEVICE_ID, account: null, updatedAt: new Date() })
  .where(and(eq(agents.ownerId, demo), eq(agents.role, `assistant`)))
// The demo machine reads online beside them (the relay stub's job otherwise).
await db.update(devices).set({ lastSeenAt }).where(and(eq(devices.userId, demo), eq(devices.deviceId, DEMO_DEVICE_ID)))
console.log(`member space: ${state}`)
process.exit(0)
