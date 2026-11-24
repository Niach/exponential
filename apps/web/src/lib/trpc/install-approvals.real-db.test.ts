// @vitest-environment node
//
// #EXP-1950 (fixtures/install-trust.json): install approval through the
// installs router against the REAL migrated Postgres. The server stores and
// relays (`trust`, `requestApproval`, `claimRoot`, `approve`, `waiting`,
// `revoke`); every verdict here about who is TRUSTED comes from the client
// reference `reconcileTrust` run on what the server answered.
import { afterAll, describe, expect, it, vi } from "vitest"
import { eq, inArray, sql } from "drizzle-orm"
import { randomBytes } from "node:crypto"
import { db } from "@/db/connection"
import { installApprovals, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { emptyTrustStore, reconcileTrust, signApproval, signRegistration, installX25519Of, type TrustStore } from "@exp/domain-contract/install-trust"
import { endpointIdOf } from "@exp/domain-contract/wrap-signature"
import { APPROVAL_SIGNATURE_REFUSAL, ROOT_CONFLICT } from "@/lib/install-approvals"

process.env.BETTER_AUTH_SECRET ||= `install-approvals-test-secret-0123456789abcdef`

vi.mock(`@/lib/steer`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/steer")>()),
  relayPostNudge: async () => ({ delivered: false }),
  relayPostKill: async () => ({ delivered: false }),
  relayPostInterrupt: async () => ({ delivered: false }),
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'installs' AND column_name = 'approval_requested_at') AS ok
`)
if (!ready.ok) console.info(`[install-approvals.real-db] skipped: ${ready.reason}`)

const { installsRouter } = await import(`@/lib/trpc/installs`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const ctx = (userId: string) => ({ session: { user: { id: userId } }, db, request: new Request(`http://x.test`) }) as never
const api = (userId: string) => installsRouter.createCaller(ctx(userId))

async function user(): Promise<string> {
  const id = `trust-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@trust.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(...members: string[]): Promise<string> {
  const [row] = await db.insert(teams).values({ name: `Trust`, slug: `trust-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  for (const [at, userId] of members.entries()) await db.insert(teamMembers).values({ teamId: row!.id, userId, role: at === 0 ? `owner` : `member` })
  return row!.id
}

type Install = { userId: string; installId: string; identity: Uint8Array; endpointId: string; x25519Public: string; store: TrustStore }

/** A real registration (`installs.register`), so its signature is stored. */
async function install(userId: string, label = `Browser`): Promise<Install> {
  const identity = new Uint8Array(randomBytes(32))
  const signedAt = new Date().toISOString()
  const endpointId = endpointIdOf(identity)
  const x25519Public = installX25519Of(identity)
  const { installId } = await api(userId).register({ kind: `web`, label, endpointId, x25519Public, signedAt, signature: signRegistration(identity, userId, signedAt) })
  return { userId, installId, identity, endpointId, x25519Public, store: emptyTrustStore(endpointId, userId) }
}

/** One client round: what the server answers, judged by the client reference. */
async function round(who: Install) {
  const view = reconcileTrust(who.store, await api(who.userId).trust({ installId: who.installId }))
  who.store = view.store
  return view
}

const subjectOf = (who: Install) => ({ userId: who.userId, endpointId: who.endpointId, x25519Public: who.x25519Public })
const claim = (who: Install) => api(who.userId).claimRoot({ installId: who.installId, signature: signApproval(who.identity, `root`, subjectOf(who)) })
const approve = (by: Install, subject: Install, kind: `approve` | `reset` | `decline` = `approve`) =>
  api(by.userId).approve({ installId: by.installId, subjectInstallId: subject.installId, kind, signature: signApproval(by.identity, kind, subjectOf(subject)) })

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) await db.delete(teams).where(inArray(teams.id, made.teams))
  if (made.users.length) await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
})

describe.skipIf(!ready.ok)(`#EXP-1950 install approval`, () => {
  it(`the first install claims the root; a second asks, shows in waiting, is approved, and both trust each other`, async () => {
    const me = await user()
    await team(me)
    const A = await install(me, `MacBook`)
    expect((await round(A)).status).toBe(`claimRoot`)
    await claim(A)
    expect(await round(A)).toMatchObject({ status: `trusted`, rows: [] })

    const B = await install(me, `Firefox`)
    expect((await round(B)).status).toBe(`waiting`)
    await api(me).requestApproval({ installId: B.installId })
    await api(me).requestApproval({ installId: B.installId }) // once is enough; again is a no-op
    expect(await api(me).waiting()).toEqual([expect.objectContaining({ kind: `newInstall`, installId: B.installId, endpointId: B.endpointId, label: `Firefox` })])
    const before = await round(A)
    expect(before.rows).toEqual([expect.objectContaining({ kind: `newInstall`, installId: B.installId })])

    // A second root while A is on the chain: CONFLICT.
    await expect(claim(B)).rejects.toMatchObject({ code: `CONFLICT`, message: ROOT_CONFLICT })

    await approve(A, B)
    await approve(A, B) // stored once
    expect(await db.select().from(installApprovals).where(eq(installApprovals.subjectInstallId, B.installId))).toHaveLength(1)
    expect(await api(me).waiting()).toEqual([])
    expect(await round(A)).toMatchObject({ status: `trusted`, rows: [] })
    expect(A.store.pins[B.endpointId]).toMatchObject({ via: `approved`, by: A.endpointId })
    expect(await round(B)).toMatchObject({ status: `trusted` })
    expect(B.store.pins[A.endpointId]).toMatchObject({ via: `chain` })
  })

  it(`a bad signature, an approver that is not the caller's, another account's subject: refused`, async () => {
    const me = await user()
    const stranger = await user()
    await team(me)
    const A = await install(me)
    await claim(A)
    const B = await install(me)
    const S = await install(stranger)
    // Signed by B over itself, posted as A's approval.
    await expect(
      api(me).approve({ installId: A.installId, subjectInstallId: B.installId, kind: `approve`, signature: signApproval(B.identity, `approve`, subjectOf(B)) })
    ).rejects.toMatchObject({ code: `BAD_REQUEST`, message: APPROVAL_SIGNATURE_REFUSAL })
    // A's signature of another kind.
    await expect(
      api(me).approve({ installId: A.installId, subjectInstallId: B.installId, kind: `approve`, signature: signApproval(A.identity, `reset`, subjectOf(B)) })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(approve(S, B)).rejects.toMatchObject({ code: `NOT_FOUND` })
    await expect(api(stranger).approve({ installId: A.installId, subjectInstallId: B.installId, kind: `approve`, signature: `A`.repeat(86) })).rejects.toMatchObject({
      code: `FORBIDDEN`,
    })
    await expect(api(me).claimRoot({ installId: B.installId, signature: signApproval(A.identity, `root`, subjectOf(B)) })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(await db.select().from(installApprovals).where(eq(installApprovals.subjectInstallId, B.installId))).toEqual([])
    // trust and waiting never answer another account's installs.
    expect((await api(stranger).trust({ installId: S.installId })).installs.map((row) => row.installId)).toEqual([S.installId])
  })

  it(`the server stores what it is given and judges nothing: an approval by an install on no chain pins nobody`, async () => {
    const me = await user()
    await team(me)
    const A = await install(me)
    await claim(A)
    const X = await install(me) // never approved
    const Y = await install(me)
    await api(me).requestApproval({ installId: Y.installId })
    await approve(X, Y)
    expect(await db.select().from(installApprovals).where(eq(installApprovals.subjectInstallId, Y.installId))).toHaveLength(1)
    const view = await round(A)
    expect(A.store.pins[Y.endpointId]).toBeUndefined()
    expect(view.rows.map((row) => row.installId)).toEqual([Y.installId])
    // The server's waiting list says Y is chained; the client does not believe it.
    expect(await api(me).waiting()).toEqual([])
  })

  it(`remove = revoke: the install is no longer served and its pin is dropped`, async () => {
    const me = await user()
    await team(me)
    const A = await install(me)
    await claim(A)
    const B = await install(me)
    await api(me).requestApproval({ installId: B.installId })
    await approve(A, B)
    await round(A)
    expect(A.store.pins[B.endpointId]).toBeDefined()
    await api(me).revoke({ installId: B.installId })
    await round(A)
    expect(A.store.pins[B.endpointId]).toBeUndefined()
    expect((await api(me).trust({ installId: A.installId })).installs.map((row) => row.installId)).toEqual([A.installId])
    await expect(api(me).trust({ installId: B.installId })).rejects.toMatchObject({ code: `FORBIDDEN` })
  })

  it(`a team-mate: first contact pins their root; every install lost, a new root is a Reset row; approve and decline`, async () => {
    const me = await user()
    const mate = await user()
    const outsider = await user()
    await team(me, mate)
    const A = await install(me)
    await claim(A)
    const M1 = await install(mate)
    await claim(M1)
    const O = await install(outsider)
    await claim(O)
    await round(A)
    expect(A.store.pins[M1.endpointId]).toMatchObject({ via: `first` })
    expect(A.store.pins[O.endpointId]).toBeUndefined() // not a team-mate: never served

    // The mate loses every install: M1 removed, M2 starts a new chain.
    await api(mate).revoke({ installId: M1.installId })
    const M2 = await install(mate)
    await claim(M2)
    expect(await api(me).waiting()).toEqual([expect.objectContaining({ kind: `reset`, installId: M2.installId, userId: mate })])
    expect((await round(A)).rows).toEqual([expect.objectContaining({ kind: `reset`, installId: M2.installId })])
    expect(A.store.pins[M2.endpointId]).toBeUndefined()
    // A reset of a non-team-mate, an own install as a reset: refused.
    await expect(approve(A, O, `reset`)).rejects.toMatchObject({ code: `NOT_FOUND` })
    await approve(A, M2, `reset`)
    expect(await api(me).waiting()).toEqual([])
    expect(await round(A)).toMatchObject({ rows: [] })
    expect(A.store.pins[M2.endpointId]).toMatchObject({ via: `reset`, by: A.endpointId })

    // Decline: the mate resets again; this account's Remove turns it down.
    await api(mate).revoke({ installId: M2.installId })
    const M3 = await install(mate)
    await claim(M3)
    await approve(A, M3, `decline`)
    expect(await api(me).waiting()).toEqual([])
    expect(await round(A)).toMatchObject({ rows: [] })
    expect(A.store.pins[M3.endpointId]).toBeUndefined()
    expect(A.store.declined).toContain(M3.endpointId)
  })

  it(`a reset needs a root; claimRoot after every other install is removed is a reset of the own account`, async () => {
    const me = await user()
    const mate = await user()
    await team(me, mate)
    const A = await install(me)
    await claim(A)
    const M = await install(mate)
    await expect(approve(A, M, `reset`)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    const B = await install(me)
    await expect(claim(B)).rejects.toMatchObject({ code: `CONFLICT` })
    await api(me).revoke({ installId: A.installId })
    await claim(B)
    expect(await round(B)).toMatchObject({ status: `trusted` })
  })
})
