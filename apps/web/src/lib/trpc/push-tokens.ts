import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { fcmTokens, liveActivityFollows, sessions } from "@/db/schema"
import { kickLiveActivities } from "@/lib/integrations/live-activities"

// 32 bytes as unpadded base64url = 43 chars; the last char carries only 4
// significant bits, so it is one of [AEIMQUYcgkosw048].
const PUSH_PUBLIC_KEY_RE = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/

/** The relay base URL a DEVICE registers its install with (EXP-1435):
 * PUSH_RELAY_PUBLIC_URL when the web app reaches the relay on an internal
 * address, else PUSH_RELAY_URL; null = no relay (self-hosted without push),
 * and the apps register nothing. */
export function pushRelayPublicUrl(): string | null {
  const url = process.env.PUSH_RELAY_PUBLIC_URL || process.env.PUSH_RELAY_URL
  return url ? url.replace(/\/+$/, ``) : null
}

export const pushTokensRouter = router({
  relay: authedProcedure.query(() => ({ url: pushRelayPublicUrl() })),

  register: authedProcedure
    .input(
      z.object({
        token: z.string().min(1),
        // Only the native apps register push tokens — web push is
        // intentionally unimplemented (no service-worker push handler, no
        // PushManager subscription anywhere), so there is no `web` arm.
        platform: z.enum([`android`, `ios`]),
        // EXP-1428: the push relay's accountless mode. The device registered
        // itself with the relay, minted a capability for this server and
        // hands it over with its X25519 push public key; pushes to this row
        // are sealed to that key (lib/push-seal.ts) and POSTed to /push.
        // Both are required since NEW-14 retired the bare-token /send shim.
        capability: z.string().startsWith(`cap_`).max(512),
        pushPublicKey: z.string().regex(PUSH_PUBLIC_KEY_RE, `base64url of 32 bytes`),
        // #EXP-1639: the build draws Live Activities / Live Updates (their
        // content is sealed to the push key).
        liveActivities: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      // One device token may be held by several signed-in accounts at once
      // (multi-account phones), so the conflict target is the (token, user)
      // pair: a re-register only refreshes this user's own row and never
      // steals the registration from another account on the same device.
      // Cleanup is three-layered: sign-out issues its own unregister (but that
      // call is best-effort — it can miss when offline or on old client
      // builds that never call it), rows whose relay install or capability
      // is gone are swept by the send path, and rows that stop being re-registered age out via the
      // staleness sweep (lib/fcm-token-sweep.ts) so a departed account's
      // pushes can never leak to the device indefinitely.
      await ctx.db
        .insert(fcmTokens)
        .values({
          userId,
          token: input.token,
          platform: input.platform,
          relayCapability: input.capability,
          pushPublicKey: input.pushPublicKey,
          liveActivities: Boolean(input.liveActivities),
        })
        .onConflictDoUpdate({
          target: [fcmTokens.token, fcmTokens.userId],
          // The registration describes what the running build can open, so a
          // re-register replaces the capability, key and Live Activities flag.
          set: {
            platform: input.platform,
            relayCapability: input.capability,
            pushPublicKey: input.pushPublicKey,
            liveActivities: Boolean(input.liveActivities),
            updatedAt: new Date(),
          },
        })
      if (input.liveActivities) kickLiveActivities([userId])
      return { ok: true }
    }),

  /** #EXP-1639: the phone started (or ActivityKit restarted) the Live
   *  Activity of a session and registered its push token with the relay as
   *  `activityId`; the next change of that session is pushed to it. Only the
   *  member's own registration and own session. */
  activity: authedProcedure
    .input(
      z.object({
        token: z.string().min(1),
        sessionId: z.string().uuid(),
        activityId: z.string().startsWith(`act_`).max(64),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const [row] = await ctx.db
        .select({ id: fcmTokens.id })
        .from(fcmTokens)
        .where(and(eq(fcmTokens.token, input.token), eq(fcmTokens.userId, userId)))
        .limit(1)
      const [session] = await ctx.db
        .select({ id: sessions.id })
        .from(sessions)
        .where(and(eq(sessions.id, input.sessionId), eq(sessions.userId, userId)))
        .limit(1)
      if (!row || !session) return { ok: false }
      const now = new Date()
      await ctx.db
        .insert(liveActivityFollows)
        .values({ sessionId: session.id, fcmTokenId: row.id, relayActivityId: input.activityId, content: ``, sentAt: now })
        .onConflictDoUpdate({
          target: [liveActivityFollows.sessionId, liveActivityFollows.fcmTokenId],
          // An empty content forces the next sync to push the current one.
          set: { relayActivityId: input.activityId, content: ``, updatedAt: now },
        })
      kickLiveActivities([userId])
      return { ok: true }
    }),

  unregister: authedProcedure
    .input(z.object({ token: z.string().min(1) }))
    .mutation(async ({ ctx, input }) => {
      await ctx.db
        .delete(fcmTokens)
        .where(
          and(
            eq(fcmTokens.token, input.token),
            eq(fcmTokens.userId, ctx.session.user.id)
          )
        )
      return { ok: true }
    }),
})
