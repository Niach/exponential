// EXP-1457 / VAPP-37: the eNet install registry. Every app install holds an
// ed25519 identity (its iroh endpoint id = the lowercase hex public key) and
// an X25519 key peers seal to. `register` stores an install only with a
// fresh signature by that identity over the caller's user id, so an endpoint
// id belongs to exactly one account and a revoked one never comes back.
// Teammates discover each other through `list`, computed from the team's
// CURRENT membership: removing a member needs no cleanup, nothing crosses
// teams. Server-only table, no Electric shape.
import { teamMemberPairs } from "@/lib/auth/membership"
import { createPublicKey, verify } from "node:crypto"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { devices, installs, sessionKeys, sessions, teamMembers, teams } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/auth/access"
import { isUniqueViolation } from "@/lib/trpc/db-errors"
import { loadSessionForUser, sessionDeviceOwner } from "@/lib/sessions/access"
import { nudgeDevice } from "@/lib/trpc/devices"
import { CLIENT_VERSION_HEADER, parseClientVersionHeader, parseVersionTuple } from "@/lib/client-version"

/**
 * #EXP-2082, FOR THE RELEASE SHEPHERD: the first desktop / CLI version whose
 * device admits the starter of a session in a team it holds no registry list
 * for (steer-enet.json `access`, A TEAM WITHOUT A LIST; `steer`
 * `sender_in_team`). Set it to that release's version (`0.14.180`) once it is
 * tagged: from then `installs.forSession` names the endpoints of a
 * personal-team session (the Assistant's conversation) again for machines
 * whose every desktop / daemon install registered at or above it, so their
 * live feed and background catch-up run P2P. null = no build carries it:
 * every such session stays on the sealed relay tunnel.
 */
export const ENET_TEAMLESS_STARTER_MIN_VERSION: string | null = null

/** Whether `version` (an install's `app_version`) is at or above `floor`. */
export function versionAtLeast(version: string | null | undefined, floor: string | null): boolean {
  const have = version ? parseVersionTuple(version) : null
  const want = floor ? parseVersionTuple(floor) : null
  if (!have || !want) return false
  for (let i = 0; i < 3; i++) if (have[i] !== want[i]) return have[i]! > want[i]!
  return true
}

/** Test seam: the floor `forSession` applies (default: the constant). */
let teamlessStarterFloor: string | null = ENET_TEAMLESS_STARTER_MIN_VERSION
export function setTeamlessStarterFloorForTest(floor: string | null): void {
  teamlessStarterFloor = floor
}

/** The installs `forSession` names for a session in a PERSONAL team: the
 *  machine's, and only when EVERY one of them registered at or above the
 *  floor. One older install there = none (the sealed relay tunnel): it may
 *  be the one running the session, it would refuse the join, and a refused
 *  join opens no relay leg (steer-enet.json `enetRefused`). */
export function teamlessStarterEndpoints<T extends { appVersion: string | null }>(
  rows: readonly T[],
  floor: string | null
): T[] {
  return rows.length > 0 && rows.every((row) => versionAtLeast(row.appVersion, floor)) ? [...rows] : []
}

export const INSTALL_KINDS = [`desktop`, `daemon`, `ios`, `android`, `web`] as const
/** At most this many non-revoked installs per user. */
export const MAX_INSTALLS_PER_USER = 50
/** How old (or how far ahead) a registration's `signedAt` may be. */
export const INSTALL_SIGNATURE_SKEW_MS = 10 * 60 * 1000

const ENDPOINT_ID_RE = /^[0-9a-f]{64}$/
// 32 bytes as unpadded base64url = 43 chars; the last char carries only 4
// significant bits (same rule as push-tokens.ts).
const X25519_PUBLIC_RE = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/
// A 64-byte ed25519 signature as unpadded base64url = 86 chars (last char: 2 bits).
const SIGNATURE_RE = /^[A-Za-z0-9_-]{85}[AQgw]$/
// The DER SubjectPublicKeyInfo prefix of a raw 32-byte ed25519 key.
const ED25519_SPKI_PREFIX = Buffer.from(`302a300506032b6570032100`, `hex`)

/** The UTF-8 string the install identity signs. */
/** EXP-1458: the machine id (`devices.device_id`, the string sessions name)
 *  of each install's linked devices row, so a writer can wrap a tree key
 *  for the install of the device that will run a session. */
async function machineIds(
  database: Pick<typeof import("@/db/connection").db, `select`>,
  rows: ReadonlyArray<{ deviceId: string | null }>
): Promise<Map<string, string>> {
  const ids = [...new Set(rows.flatMap((row) => (row.deviceId ? [row.deviceId] : [])))]
  if (ids.length === 0) return new Map()
  const found = await database
    .select({ id: devices.id, deviceId: devices.deviceId })
    .from(devices)
    .where(inArray(devices.id, ids))
  return new Map(found.map((row) => [row.id, row.deviceId]))
}

export function installMessage(
  userId: string,
  endpointId: string,
  x25519Public: string,
  signedAt: string
): string {
  return `exp-install-v1\n${userId}\n${endpointId}\n${x25519Public}\n${signedAt}`
}

/** Null when `signature` is the endpoint key's ed25519 signature over
 *  `message`, else why not. Never throws. */
function signatureRefusal(endpointId: string, message: string, signature: string): string | null {
  try {
    const key = createPublicKey({
      key: Buffer.concat([ED25519_SPKI_PREFIX, Buffer.from(endpointId, `hex`)]),
      format: `der`,
      type: `spki`,
    })
    return verify(null, Buffer.from(message, `utf8`), key, Buffer.from(signature, `base64url`))
      ? null
      : `Invalid install signature`
  } catch {
    return `Invalid install signature`
  }
}

/** #EXP-1950: an install's newest registration signature over
 *  `installMessage(userId, endpointId, x25519Public, signedAt)`; null
 *  until it registers again. */
function registrationOf(row: { registrationSignature: string | null; registrationSignedAt: string | null }) {
  return { registrationSignature: row.registrationSignature, registrationSignedAt: row.registrationSignedAt }
}

/** EXP-1472: the machines whose installs hold a current tree key a new
 *  install of `userId` reads (its own trees, and the shared trees of its
 *  teams): each gets a `check_in` nudge, which runs its key holder's pass. */
async function nudgeKeyHolders(database: typeof import("@/db/connection").db, userId: string): Promise<void> {
  const rows = await database
    .selectDistinct({ userId: devices.userId, deviceId: devices.deviceId })
    .from(sessionKeys)
    .innerJoin(sessions, and(eq(sessions.id, sessionKeys.rootId), eq(sessions.sealEpoch, sessionKeys.epoch)))
    .innerJoin(installs, eq(installs.id, sessionKeys.installId))
    .innerJoin(devices, eq(devices.id, installs.deviceId))
    .where(
      and(
        isNull(installs.revokedAt),
        sql`(${sessions.userId} = ${userId} OR (${sessions.shared} AND ${sessions.teamId} IN (SELECT ${teamMembers.teamId} FROM ${teamMembers} WHERE ${teamMembers.userId} = ${userId})))`
      )
    )
    .limit(50)
  for (const row of rows) nudgeDevice(row.userId, row.deviceId)
}

export const installsRouter = router({
  register: authedProcedure
    .input(
      z.object({
        kind: z.enum(INSTALL_KINDS),
        label: z.string().trim().max(64).optional(),
        platform: z.string().trim().max(32).optional(),
        // The `devices.device_id` string desktop/daemon register with.
        deviceId: z.string().min(1).max(128).optional(),
        endpointId: z.string().regex(ENDPOINT_ID_RE, `lowercase hex of 32 bytes`),
        x25519Public: z.string().regex(X25519_PUBLIC_RE, `base64url of 32 bytes`),
        signedAt: z.string().min(1).max(64),
        signature: z.string().regex(SIGNATURE_RE, `base64url of 64 bytes`),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })

      const at = Date.parse(input.signedAt)
      if (!Number.isFinite(at)) throw bad(`signedAt is not an ISO timestamp`)
      if (Math.abs(at - Date.now()) > INSTALL_SIGNATURE_SKEW_MS) {
        throw bad(`signedAt is more than 10 minutes off the server clock`)
      }
      const refusal = signatureRefusal(
        input.endpointId,
        installMessage(userId, input.endpointId, input.x25519Public, input.signedAt),
        input.signature
      )
      if (refusal) throw bad(refusal)

      let deviceRowId: string | null = null
      if (input.deviceId !== undefined) {
        if (input.kind !== `desktop` && input.kind !== `daemon`) {
          throw bad(`deviceId is only for desktop and daemon installs`)
        }
        const [device] = await ctx.db
          .select({ id: devices.id })
          .from(devices)
          .where(and(eq(devices.userId, userId), eq(devices.deviceId, input.deviceId)))
          .limit(1)
        if (!device) throw bad(`Unknown device`)
        deviceRowId = device.id
      }

      // #EXP-2082: the registering build's own version (desktop + CLI send
      // `x-client-version` on every request; anything else = unknown).
      const client = parseClientVersionHeader(ctx.request.headers.get(CLIENT_VERSION_HEADER))
      const fields = {
        kind: input.kind,
        label: input.label || null,
        platform: input.platform || null,
        appVersion: client && (input.kind === `desktop` || input.kind === `daemon`) ? client.version.slice(0, 32) : null,
        deviceId: deviceRowId,
        x25519Public: input.x25519Public,
        // #EXP-1950: kept, so a client re-checks the binding (`installMessage`).
        registrationSignature: input.signature,
        registrationSignedAt: input.signedAt,
      }

      try {
        const registered = await ctx.db.transaction(async (tx) => {
          // Serializes one user's registrations, so the cap holds under races.
          await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`installs:${userId}`}))`)
          const [existing] = await tx
            .select({ id: installs.id, userId: installs.userId, revokedAt: installs.revokedAt })
            .from(installs)
            .where(eq(installs.endpointId, input.endpointId))
            .limit(1)
          if (existing) {
            if (existing.userId !== userId) {
              throw new TRPCError({ code: `CONFLICT`, message: `Endpoint belongs to another account` })
            }
            if (existing.revokedAt) {
              throw new TRPCError({ code: `FORBIDDEN`, message: `This install was revoked` })
            }
            await tx
              .update(installs)
              .set({ ...fields, lastSeenAt: new Date() })
              .where(eq(installs.id, existing.id))
            return { installId: existing.id, fresh: false }
          }

          const live = (
            await tx
              .select({ id: installs.id, revokedAt: installs.revokedAt })
              .from(installs)
              .where(and(eq(installs.userId, userId), isNull(installs.revokedAt)))
          ).filter((row) => !row.revokedAt)
          if (live.length >= MAX_INSTALLS_PER_USER) {
            throw bad(`At most ${MAX_INSTALLS_PER_USER} installs per account: revoke one first`)
          }
          const [row] = await tx
            .insert(installs)
            .values({ ...fields, userId, endpointId: input.endpointId })
            .returning({ id: installs.id })
          return { installId: row!.id, fresh: true }
        })
        // EXP-1472: a new install reads its trees once a holder wraps their
        // keys for it; nudge the holders now instead of their next pass.
        if (registered.fresh) void nudgeKeyHolders(ctx.db, userId).catch(() => {})
        return { installId: registered.installId }
      } catch (err) {
        // Another account registered the endpoint concurrently.
        if (isUniqueViolation(err)) {
          throw new TRPCError({ code: `CONFLICT`, message: `Endpoint belongs to another account` })
        }
        throw err
      }
    }),

  list: authedProcedure
    .input(z.object({ teamId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      await resolveTeamAccess(ctx.session.user.id, input.teamId)
      // #EXP-1984: a personal team's owner is its member (their Assistant's chat).
      const members = await teamMemberPairs(ctx.db, [input.teamId])
      const userIds = [...new Set(members.map((m) => m.userId))]
      if (userIds.length === 0) return []
      const rows = await ctx.db
        .select()
        .from(installs)
        .where(and(inArray(installs.userId, userIds), isNull(installs.revokedAt)))
        .orderBy(asc(installs.createdAt))
      const machines = await machineIds(ctx.db, rows)
      return rows
        .filter((row) => !row.revokedAt)
        .map((row) => ({
          installId: row.id,
          userId: row.userId,
          deviceId: (row.deviceId && machines.get(row.deviceId)) ?? null,
          kind: row.kind,
          label: row.label,
          platform: row.platform,
          endpointId: row.endpointId,
          x25519Public: row.x25519Public,
          lastSeenAt: row.lastSeenAt,
          ...registrationOf(row),
        }))
    }),

  // VAPP-33 (fixtures/steer-enet.json DISCOVERY): the installs a viewer
  // dials to steer a session over eNet = the live desktop/daemon installs
  // of the account RUNNING it on the session's machine, newest first. Only
  // for a caller who can see the session (NOT_FOUND otherwise).
  forSession: authedProcedure
    .input(z.object({ sessionId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId, ctx.db)
      // #EXP-1989 follow-up: a session in a PERSONAL team (the Assistant's
      // conversation after the move). Every desktop and daemon released
      // before #EXP-2082 answers a viewer's eNet join from the registry list
      // of the SESSION's team (`steer` `user_in_team`), which it reads per
      // SYNCED team: the personal team is never one, so it refuses every
      // viewer (`unauthorized`), and a refused join opens no relay leg
      // (steer-enet.json `enetRefused`): no feed at all. With no endpoint the
      // viewer has no eNet feed and takes the SEALED RELAY TUNNEL at once,
      // where the device applies the access rule to the ticket's user (the
      // starter). #EXP-2082: a machine whose installs all carry the starter
      // rule is named again (`teamlessStarterEndpoints`).
      const [team] = await ctx.db.select({ kind: teams.kind }).from(teams).where(eq(teams.id, session.teamId)).limit(1)
      const personal = team?.kind === `personal`
      if (personal && !teamlessStarterFloor) return []
      const owner = await sessionDeviceOwner(session, ctx.db)
      const machines = await ctx.db
        .select({ id: devices.id })
        .from(devices)
        .where(and(eq(devices.userId, owner), eq(devices.deviceId, session.deviceId)))
      if (machines.length === 0) return []
      const rows = await ctx.db
        .select()
        .from(installs)
        .where(
          and(
            eq(installs.userId, owner),
            inArray(
              installs.deviceId,
              machines.map((machine) => machine.id)
            ),
            inArray(installs.kind, [`desktop`, `daemon`]),
            isNull(installs.revokedAt)
          )
        )
      const live = rows.filter((row) => !row.revokedAt)
      return (personal ? teamlessStarterEndpoints(live, teamlessStarterFloor) : live)
        .sort((a, b) => (b.lastSeenAt?.getTime() ?? 0) - (a.lastSeenAt?.getTime() ?? 0))
        .map((row) => ({ endpointId: row.endpointId, kind: row.kind }))
    }),

  mine: authedProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select()
      .from(installs)
      .where(eq(installs.userId, ctx.session.user.id))
      .orderBy(asc(installs.createdAt))
    const machines = await machineIds(ctx.db, rows)
    return rows.map((row) => ({
      installId: row.id,
      deviceId: (row.deviceId && machines.get(row.deviceId)) ?? null,
      kind: row.kind,
      label: row.label,
      platform: row.platform,
      endpointId: row.endpointId,
      x25519Public: row.x25519Public,
      ...registrationOf(row),
      createdAt: row.createdAt,
      lastSeenAt: row.lastSeenAt,
      revokedAt: row.revokedAt,
    }))
  }),

  revoke: authedProcedure
    .input(z.object({ installId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const [row] = await ctx.db
        .select({ id: installs.id, revokedAt: installs.revokedAt })
        .from(installs)
        .where(and(eq(installs.id, input.installId), eq(installs.userId, userId)))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Install not found` })
      if (!row.revokedAt) {
        await ctx.db
          .update(installs)
          .set({ revokedAt: new Date() })
          .where(and(eq(installs.id, row.id), eq(installs.userId, userId)))
      }
      return { ok: true }
    }),
})
