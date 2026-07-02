import { z } from "zod"
import { and, eq, isNull } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { notifications } from "@/db/schema"
import { notificationTypeValues } from "@/lib/domain"
import { emailEnabled } from "@/lib/email"
import { digestValues } from "@/lib/notification-email-policy"
import {
  getOrCreateEmailPrefs,
  updateEmailPrefs,
} from "@/lib/notification-prefs"

// Per-type email opt-outs: keys are notification_type values, value false =
// opted out (absent/true = on). partialRecord: zod v4 records with enum keys
// are exhaustive by default.
const typePrefsSchema = z.partialRecord(
  z.enum(notificationTypeValues),
  z.boolean()
)

// Inbox mark-read. Ownership-guarded on user_id so a caller can only touch their
// own rows. read_at updates re-stream over the per-user notifications shape.
export const notificationsRouter = router({
  markRead: authedProcedure
    .input(z.object({ id: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(notifications)
          .set({ readAt: new Date() })
          .where(
            and(
              eq(notifications.id, input.id),
              eq(notifications.userId, ctx.session.user.id)
            )
          )
        return { txId }
      })
    }),

  markAllRead: authedProcedure.mutation(async ({ ctx }) => {
    return await ctx.db.transaction(async (tx) => {
      const txId = await generateTxId(tx)
      await tx
        .update(notifications)
        .set({ readAt: new Date() })
        .where(
          and(
            eq(notifications.userId, ctx.session.user.id),
            isNull(notifications.readAt)
          )
        )
      return { txId }
    })
  }),

  // Email-notification prefs (user_notification_prefs is server-only — read
  // via tRPC, never synced). The row is auto-created with a random
  // unsubscribeToken on first read/write; a user who never touched the panel
  // simply has the defaults (email on, all types on, no digest).
  // `transportConfigured` lets the web panel hide/disable email affordances on
  // self-hosted instances without RESEND_API_KEY/SMTP_HOST (§6.6).
  emailPrefs: authedProcedure.query(async ({ ctx }) => {
    const prefs = await getOrCreateEmailPrefs(ctx.session.user.id)
    return {
      emailEnabled: prefs.emailEnabled,
      typePrefs: prefs.typePrefs,
      digest: prefs.digest,
      transportConfigured: emailEnabled,
    }
  }),

  updateEmailPrefs: authedProcedure
    .input(
      z.object({
        emailEnabled: z.boolean().optional(),
        typePrefs: typePrefsSchema.optional(),
        digest: z.enum(digestValues).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const prefs = await updateEmailPrefs(ctx.session.user.id, input)
      return {
        emailEnabled: prefs.emailEnabled,
        typePrefs: prefs.typePrefs,
        digest: prefs.digest,
        transportConfigured: emailEnabled,
      }
    }),
})
