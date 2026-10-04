import { z } from "zod"
import { and, eq, isNull } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { notifications } from "@/db/schema"
import { notificationTypeValues } from "@/lib/domain"
import { emailEnabled } from "@/lib/email-enabled"
import { digestValues } from "@/lib/notification-email-policy"
import {
  getOrCreateEmailPrefs,
  updateEmailPrefs,
} from "@/lib/notification-prefs"

// Per-type email opt-outs: keys are notification_type values, value false =
// opted out (absent/true = on). The stored shape stays a partial record of
// known types only.
// Compat shim (release train 2026-10-04): desktop <= 0.14.60 sends support_reply; retire when CLIENT_MIN_VERSION_DESKTOP/CLI > 0.14.60.
// Those builds still list "Support reply", and a strict enum record rejected
// that key and with it every later toggle in the pane. So: accept any key,
// map support_reply to its successor reporter_reply (unless the payload names
// reporter_reply itself), and drop whatever is not a known type.
const knownNotificationTypes = new Set<string>(notificationTypeValues)

type TypePrefs = Partial<Record<(typeof notificationTypeValues)[number], boolean>>

const typePrefsSchema = z
  .record(z.string(), z.boolean())
  .transform((raw): TypePrefs => {
    const prefs: Record<string, boolean> = {}
    for (const [key, value] of Object.entries(raw)) {
      if (knownNotificationTypes.has(key)) prefs[key] = value
    }
    if (`support_reply` in raw && !(`reporter_reply` in raw)) {
      prefs.reporter_reply = raw.support_reply!
    }
    return prefs as TypePrefs
  })

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

  // Opening an issue clears its inbox entries (EXP-92): push taps and email
  // deep links land on the issue detail without passing through the inbox, so
  // the detail views on every client fire this on open. Server-side by design —
  // it also clears rows the client hasn't synced yet, and a row read here
  // escapes the hourly unread-email digest.
  markReadByIssue: authedProcedure
    .input(z.object({ issueId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      return await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        await tx
          .update(notifications)
          .set({ readAt: new Date() })
          .where(
            and(
              eq(notifications.issueId, input.issueId),
              eq(notifications.userId, ctx.session.user.id),
              isNull(notifications.readAt)
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
  // simply has the defaults (email on, all types on, daily digest).
  // `transportConfigured` lets the web panel hide/disable email affordances on
  // self-hosted instances without AWS_SES_REGION/SMTP_HOST (§6.6).
  emailPrefs: authedProcedure.query(async ({ ctx }) => {
    const prefs = await getOrCreateEmailPrefs(ctx.session.user.id)
    return {
      emailEnabled: prefs.emailEnabled,
      typePrefs: prefs.typePrefs,
      digest: prefs.digest,
      digestHour: prefs.digestHour,
      allowAgentMessages: prefs.allowAgentMessages,
      transportConfigured: emailEnabled,
    }
  }),

  updateEmailPrefs: authedProcedure
    .input(
      z.object({
        emailEnabled: z.boolean().optional(),
        typePrefs: typePrefsSchema.optional(),
        digest: z.enum(digestValues).optional(),
        // Local hour the daily digest goes out at — FULL HOURS only, so the
        // 10-minute sweep can resolve every user's send point.
        digestHour: z.number().int().min(0).max(23).optional(),
        // EXP-801: false BLOCKS other members' agents from messaging this
        // user over MCP (no inbox row, no push) — not a delivery mute.
        allowAgentMessages: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const prefs = await updateEmailPrefs(ctx.session.user.id, input)
      return {
        emailEnabled: prefs.emailEnabled,
        typePrefs: prefs.typePrefs,
        digest: prefs.digest,
        digestHour: prefs.digestHour,
        allowAgentMessages: prefs.allowAgentMessages,
        transportConfigured: emailEnabled,
      }
    }),
})
