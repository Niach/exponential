import { z } from "zod"
import { and, eq } from "drizzle-orm"
import { router, authedProcedure } from "@/lib/trpc"
import { fcmTokens } from "@/db/schema"

// 32 bytes as unpadded base64url = 43 chars; the last char carries only 4
// significant bits, so it is one of [AEIMQUYcgkosw048].
const PUSH_PUBLIC_KEY_RE = /^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$/

export const pushTokensRouter = router({
  register: authedProcedure
    .input(
      z
        .object({
          token: z.string().min(1),
          // Only the native apps register push tokens — web push is
          // intentionally unimplemented (no service-worker push handler, no
          // PushManager subscription anywhere), so there is no `web` arm.
          platform: z.enum([`android`, `ios`]),
          // EXP-1428: the push relay's accountless mode. The device registered
          // itself with the relay, minted a capability for this server and
          // hands it over with its X25519 push public key; pushes to this row
          // are then sealed to that key (lib/push-seal.ts) and POSTed to
          // /push. Both or neither; released clients send neither.
          capability: z.string().startsWith(`cap_`).max(512).optional(),
          pushPublicKey: z
            .string()
            .regex(PUSH_PUBLIC_KEY_RE, `base64url of 32 bytes`)
            .optional(),
        })
        .refine(
          (v) => (v.capability === undefined) === (v.pushPublicKey === undefined),
          { message: `capability and pushPublicKey go together` }
        )
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      // One device token may be held by several signed-in accounts at once
      // (multi-account phones), so the conflict target is the (token, user)
      // pair: a re-register only refreshes this user's own row and never
      // steals the registration from another account on the same device.
      // Cleanup is three-layered: sign-out issues its own unregister (but that
      // call is best-effort — it can miss when offline or on old client
      // builds that never call it), rows FCM invalidates are swept by the
      // send path, and rows that stop being re-registered age out via the
      // staleness sweep (lib/fcm-token-sweep.ts) so a departed account's
      // pushes can never leak to the device indefinitely.
      await ctx.db
        .insert(fcmTokens)
        .values({
          userId,
          token: input.token,
          platform: input.platform,
          relayCapability: input.capability ?? null,
          pushPublicKey: input.pushPublicKey ?? null,
        })
        .onConflictDoUpdate({
          target: [fcmTokens.token, fcmTokens.userId],
          // The registration describes what the running build can open: a
          // re-register without a capability (e.g. an app downgrade) clears
          // it, so the row falls back to the /send shim.
          set: {
            platform: input.platform,
            relayCapability: input.capability ?? null,
            pushPublicKey: input.pushPublicKey ?? null,
            updatedAt: new Date(),
          },
        })
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
