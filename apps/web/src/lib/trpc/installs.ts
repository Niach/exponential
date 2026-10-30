// EXP-1457 / VAPP-37: the eNet install registry. Every app install holds an
// ed25519 identity (its iroh endpoint id = the lowercase hex public key) and
// an X25519 key peers seal to. `register` stores an install only with a
// fresh signature by that identity over the caller's user id, so an endpoint
// id belongs to exactly one account and a revoked one never comes back.
// Teammates discover each other through `list`, computed from the team's
// CURRENT membership: removing a member needs no cleanup, nothing crosses
// teams. Server-only table, no Electric shape.
import { createPublicKey, verify } from "node:crypto"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, asc, eq, inArray, isNull, sql } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { devices, installs, teamMembers } from "@/db/schema"
import { resolveTeamAccess } from "@/lib/auth/access"
import { isUniqueViolation } from "@/lib/trpc/db-errors"

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

      const fields = {
        kind: input.kind,
        label: input.label || null,
        platform: input.platform || null,
        deviceId: deviceRowId,
        x25519Public: input.x25519Public,
      }

      try {
        return await ctx.db.transaction(async (tx) => {
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
            return { installId: existing.id }
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
          return { installId: row!.id }
        })
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
      const members = await ctx.db
        .select({ userId: teamMembers.userId })
        .from(teamMembers)
        .where(eq(teamMembers.teamId, input.teamId))
      const userIds = [...new Set(members.map((m) => m.userId))]
      if (userIds.length === 0) return []
      const rows = await ctx.db
        .select()
        .from(installs)
        .where(and(inArray(installs.userId, userIds), isNull(installs.revokedAt)))
        .orderBy(asc(installs.createdAt))
      return rows
        .filter((row) => !row.revokedAt)
        .map((row) => ({
          installId: row.id,
          userId: row.userId,
          kind: row.kind,
          label: row.label,
          platform: row.platform,
          endpointId: row.endpointId,
          x25519Public: row.x25519Public,
          lastSeenAt: row.lastSeenAt,
        }))
    }),

  mine: authedProcedure.query(async ({ ctx }) => {
    const rows = await ctx.db
      .select()
      .from(installs)
      .where(eq(installs.userId, ctx.session.user.id))
      .orderBy(asc(installs.createdAt))
    return rows.map((row) => ({
      installId: row.id,
      kind: row.kind,
      label: row.label,
      platform: row.platform,
      endpointId: row.endpointId,
      x25519Public: row.x25519Public,
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
