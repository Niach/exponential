// #EXP-1950 (fixtures/install-trust.json): the server half of install
// approval. The server STORES and RELAYS: which installs asked to be
// approved, the signed approvals between installs, and the list a person's
// Needs you draws from. It decides nothing about trust: every client checks
// each registration and approval signature itself (`reconcileTrust`) and
// shows only the rows it can verify. The server checks a signature it is
// given against the caller's own install (a bug catcher, not the protection).
import { verifyApproval, type ApprovalKind, type ServedApproval, type ServedInstall } from "@exp/domain-contract/install-trust"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm"
import { installApprovals, installs, teamMembers, teams, type Install } from "@/db/schema"
import type { Reader } from "@/lib/sessions/tree-keys"

export const APPROVAL_SIGNATURE_REFUSAL = `An approval's signature is not your install's over this install`
export const ROOT_CONFLICT = `This account already has a trusted install`

type Database = typeof import("@/db/connection").db

/** The caller and every member of a team the caller belongs to (its personal team included). */
export async function trustUserIds(database: Reader, userId: string): Promise<string[]> {
  const teamIds = (
    await database
      .select({ teamId: teams.id })
      .from(teams)
      .leftJoin(teamMembers, and(eq(teamMembers.teamId, teams.id), eq(teamMembers.userId, userId)))
      .where(sql`${teamMembers.userId} = ${userId} OR (${teams.ownerId} = ${userId} AND ${teams.kind} = 'personal')`)
  ).map((row) => row.teamId)
  if (teamIds.length === 0) return [userId]
  const rows = await database.select({ userId: teamMembers.userId }).from(teamMembers).where(inArray(teamMembers.teamId, teamIds))
  return [...new Set([userId, ...rows.map((row) => row.userId)])].sort()
}

function servedInstall(row: Install): ServedInstall {
  return {
    installId: row.id,
    userId: row.userId,
    endpointId: row.endpointId,
    x25519Public: row.x25519Public,
    registrationSignature: row.registrationSignature,
    registrationSignedAt: row.registrationSignedAt,
    approvalRequested: row.approvalRequestedAt !== null,
    label: row.label,
  }
}

/** `installs.trust`: every live install of the caller's account and of its
 *  team-mates, and every approval about them (`reconcileTrust`'s input). */
export async function trustAnswer(database: Reader, userId: string): Promise<{ installs: ServedInstall[]; approvals: ServedApproval[] }> {
  const userIds = await trustUserIds(database, userId)
  const rows = (
    await database
      .select()
      .from(installs)
      .where(and(inArray(installs.userId, userIds), isNull(installs.revokedAt)))
      .orderBy(asc(installs.createdAt), asc(installs.id))
  ).filter((row) => !row.revokedAt && userIds.includes(row.userId))
  if (rows.length === 0) return { installs: [], approvals: [] }
  const ids = rows.map((row) => row.id)
  const approvals = (
    await database
      .select()
      .from(installApprovals)
      .where(inArray(installApprovals.subjectInstallId, ids))
      .orderBy(asc(installApprovals.createdAt), asc(installApprovals.id))
  ).filter((row) => ids.includes(row.subjectInstallId))
  return {
    installs: rows.map(servedInstall),
    approvals: approvals.map((row) => ({
      kind: row.kind as ApprovalKind,
      subjectEndpointId: row.subjectEndpointId,
      approverEndpointId: row.approverEndpointId,
      signature: row.signature,
    })),
  }
}

/** A live install by id (any account), or null. */
async function liveInstall(database: Reader, installId: string): Promise<Install | null> {
  const [row] = await database.select().from(installs).where(eq(installs.id, installId)).limit(1)
  return row && row.id === installId && !row.revokedAt ? row : null
}

function checkSignature(kind: ApprovalKind, subject: Install, signature: string, approverEndpointId: string) {
  const ok = verifyApproval(kind, { userId: subject.userId, endpointId: subject.endpointId, x25519Public: subject.x25519Public }, signature, approverEndpointId)
  if (!ok) throw new TRPCError({ code: `BAD_REQUEST`, message: APPROVAL_SIGNATURE_REFUSAL })
}

/** Install ids of `userId` that are on a chain: a `root` or `approve` names them. */
async function chainedInstalls(database: Reader, installIds: string[]): Promise<Set<string>> {
  if (installIds.length === 0) return new Set()
  const rows = await database
    .select({ subjectInstallId: installApprovals.subjectInstallId, kind: installApprovals.kind })
    .from(installApprovals)
    .where(and(inArray(installApprovals.subjectInstallId, installIds), inArray(installApprovals.kind, [`root`, `approve`])))
  return new Set(rows.filter((row) => installIds.includes(row.subjectInstallId)).map((row) => row.subjectInstallId))
}

/** `installs.claimRoot`: the install starts its account's chain (the first
 *  install, or a reset once every other install is removed). CONFLICT while
 *  another live install of the account is on a chain. */
export async function claimRoot(database: Database, userId: string, input: { installId: string; signature: string }): Promise<{ ok: true }> {
  return database.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`installs:${userId}`}))`)
    const me = await liveInstall(tx, input.installId)
    if (!me || me.userId !== userId) throw new TRPCError({ code: `FORBIDDEN`, message: `Not your install` })
    checkSignature(`root`, me, input.signature, me.endpointId)
    const others = (
      await tx
        .select({ id: installs.id, userId: installs.userId, revokedAt: installs.revokedAt })
        .from(installs)
        .where(and(eq(installs.userId, userId), isNull(installs.revokedAt)))
    )
      .filter((row) => row.userId === userId && !row.revokedAt && row.id !== me.id)
      .map((row) => row.id)
    if ((await chainedInstalls(tx, others)).size > 0) throw new TRPCError({ code: `CONFLICT`, message: ROOT_CONFLICT })
    await tx
      .insert(installApprovals)
      .values({ kind: `root`, subjectInstallId: me.id, subjectEndpointId: me.endpointId, approverInstallId: me.id, approverEndpointId: me.endpointId, signature: input.signature })
      .onConflictDoNothing()
    return { ok: true as const }
  })
}

/** `installs.approve`: the caller's install signs another in (`approve`:
 *  an install of the caller's account), or a team-mate's new root (`reset`),
 *  or turns that reset down (`decline`). Stored as given; never judged. */
export async function approveInstall(
  database: Database,
  userId: string,
  input: { installId: string; subjectInstallId: string; kind: `approve` | `reset` | `decline`; signature: string }
): Promise<{ ok: true }> {
  const approver = await liveInstall(database, input.installId)
  if (!approver || approver.userId !== userId) throw new TRPCError({ code: `FORBIDDEN`, message: `Not your install` })
  const subject = await liveInstall(database, input.subjectInstallId)
  const notFound = () => new TRPCError({ code: `NOT_FOUND`, message: `Install not found` })
  if (!subject || subject.id === approver.id) throw notFound()
  if (input.kind === `approve`) {
    if (subject.userId !== userId) throw notFound()
  } else {
    if (subject.userId === userId || !(await trustUserIds(database, userId)).includes(subject.userId)) throw notFound()
    const [root] = await database
      .select({ id: installApprovals.id })
      .from(installApprovals)
      .where(and(eq(installApprovals.subjectInstallId, subject.id), eq(installApprovals.kind, `root`)))
      .limit(1)
    if (!root) throw new TRPCError({ code: `BAD_REQUEST`, message: `Not a new root` })
  }
  checkSignature(input.kind, subject, input.signature, approver.endpointId)
  await database
    .insert(installApprovals)
    .values({
      kind: input.kind,
      subjectInstallId: subject.id,
      subjectEndpointId: subject.endpointId,
      approverInstallId: approver.id,
      approverEndpointId: approver.endpointId,
      signature: input.signature,
    })
    .onConflictDoNothing()
  return { ok: true }
}

/** A row Needs you may show (the client shows it only when `reconcileTrust` agrees). */
export interface WaitingInstall {
  kind: `newInstall` | `reset`
  installId: string
  userId: string
  endpointId: string
  label: string | null
  kindOfInstall: string
}

/** `installs.waiting`, the Needs-you source: the caller's own installs that
 *  asked and are on no chain, and team-mates' new roots (a user with an
 *  earlier root) that no install of the caller's account confirmed or
 *  declined. */
export async function waitingInstalls(database: Reader, userId: string): Promise<WaitingInstall[]> {
  const userIds = await trustUserIds(database, userId)
  const all = (await database.select().from(installs).where(inArray(installs.userId, userIds))).filter((row) => userIds.includes(row.userId))
  if (all.length === 0) return []
  const ids = all.map((row) => row.id)
  const approvals = (await database.select().from(installApprovals).where(inArray(installApprovals.subjectInstallId, ids))).filter((row) =>
    ids.includes(row.subjectInstallId)
  )
  const mine = new Set(all.filter((row) => row.userId === userId).map((row) => row.endpointId))
  const out: WaitingInstall[] = []
  for (const row of all) {
    if (row.revokedAt) continue
    const about = approvals.filter((approval) => approval.subjectInstallId === row.id)
    const chained = about.some((approval) => approval.kind === `root` || approval.kind === `approve`)
    if (row.userId === userId) {
      if (row.approvalRequestedAt && !chained) out.push(waitingRow(`newInstall`, row))
      continue
    }
    const root = about.find((approval) => approval.kind === `root`)
    if (!root) continue
    const earlier = approvals.some(
      (approval) =>
        approval.kind === `root` &&
        approval.subjectInstallId !== row.id &&
        all.find((other) => other.id === approval.subjectInstallId)?.userId === row.userId &&
        approval.createdAt.getTime() <= root.createdAt.getTime()
    )
    const answered = about.some((approval) => (approval.kind === `reset` || approval.kind === `decline`) && mine.has(approval.approverEndpointId))
    if (earlier && !answered) out.push(waitingRow(`reset`, row))
  }
  return out
}

function waitingRow(kind: WaitingInstall[`kind`], row: Install): WaitingInstall {
  return { kind, installId: row.id, userId: row.userId, endpointId: row.endpointId, label: row.label, kindOfInstall: row.kind }
}
