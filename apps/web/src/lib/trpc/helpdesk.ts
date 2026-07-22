import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, desc, eq } from "drizzle-orm"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import { db } from "@/db/connection"
import {
  emailDeliveries,
  issues,
  supportMessages,
  supportThreads,
  teams,
} from "@/db/schema"
import {
  assertTeamMember,
  getBoardTeamId,
  getSoleHumanMemberId,
} from "@/lib/team-membership"
import { ensureSubscribed } from "@/lib/integrations/subscriptions"
import { deliveryStatus, sendSupportReplyEmail } from "@/lib/email"
import { mintSupportToken } from "@/lib/helpdesk/token"
import { isReporterActivelyViewing } from "@/lib/helpdesk/presence"
import {
  MAX_SUPPORT_MESSAGE_CHARS,
  closeThreadInTx,
  latestMessagesByThread,
  reopenThreadInTx,
  supportThreadUrl,
} from "@/lib/helpdesk/service"

const messageBodySchema = z
  .string()
  .trim()
  .min(1)
  .max(MAX_SUPPORT_MESSAGE_CHARS)

// Load a thread and gate on membership of its team. Every member handles
// support (permissions collapsed to membership-only) — no owner gating
// anywhere in this router.
async function loadThreadForMember(userId: string, threadId: string) {
  const [thread] = await db
    .select()
    .from(supportThreads)
    .where(eq(supportThreads.id, threadId))
    .limit(1)
  if (!thread) {
    throw new TRPCError({ code: `NOT_FOUND`, message: `Thread not found` })
  }
  await assertTeamMember(userId, thread.teamId)
  return thread
}

// Minimal linked-issue projection for the escalation chip. Web resolves the
// live row from Electric; natives read this from the tRPC response.
async function loadLinkedIssue(linkedIssueId: string | null) {
  if (!linkedIssueId) return null
  const [issue] = await db
    .select({
      id: issues.id,
      identifier: issues.identifier,
      title: issues.title,
      status: issues.status,
      boardId: issues.boardId,
    })
    .from(issues)
    .where(eq(issues.id, linkedIssueId))
    .limit(1)
  return issue ?? null
}

export const helpdeskRouter = router({
  // The inbox list: one row per ticket in the team, filtered
  // open/resolved by the thread's own status, newest activity first.
  // `unread` = the reporter spoke last — there is no per-member read state.
  listThreads: authedProcedure
    .input(
      z.object({
        teamId: z.string().uuid(),
        filter: z.enum([`open`, `resolved`]).default(`open`),
      })
    )
    .query(async ({ ctx, input }) => {
      await assertTeamMember(ctx.session.user.id, input.teamId)

      const rows = await ctx.db
        .select({
          id: supportThreads.id,
          teamId: supportThreads.teamId,
          title: supportThreads.title,
          status: supportThreads.status,
          linkedIssueId: supportThreads.linkedIssueId,
          reporterEmail: supportThreads.reporterEmail,
          reporterName: supportThreads.reporterName,
          lastReporterSeenAt: supportThreads.lastReporterSeenAt,
          createdAt: supportThreads.createdAt,
          updatedAt: supportThreads.updatedAt,
        })
        .from(supportThreads)
        .where(
          and(
            eq(supportThreads.teamId, input.teamId),
            eq(
              supportThreads.status,
              input.filter === `resolved` ? `resolved` : `open`
            )
          )
        )
        .orderBy(desc(supportThreads.updatedAt))

      const latest = await latestMessagesByThread(rows.map((row) => row.id))
      return rows.map((row) => {
        const last = latest.get(row.id) ?? null
        return {
          ...row,
          lastMessage: last,
          unread: last?.direction === `inbound`,
        }
      })
    }),

  // Full conversation including internal notes (member-only surface).
  getThread: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .query(async ({ ctx, input }) => {
      const thread = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      const messages = await ctx.db
        .select()
        .from(supportMessages)
        .where(eq(supportMessages.threadId, thread.id))
        .orderBy(supportMessages.createdAt)
      const linkedIssue = await loadLinkedIssue(thread.linkedIssueId)
      // The magic-link token is the reporter's credential — it is never
      // stored (recomputed per outbound email) and must never reach a
      // member's browser.
      return { thread, messages, linkedIssue }
    }),

  // Public reply: insert the outbound message and email the reporter with the
  // thread's one stable magic link (the token is recomputed from the thread
  // id — nothing stored; close revokes it via token_revoked_at, reopen
  // reinstates it — the link itself never changes).
  reply: authedProcedure
    .input(
      z.object({ threadId: z.string().uuid(), body: messageBodySchema })
    )
    .mutation(async ({ ctx, input }) => {
      const thread = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )

      const message = await ctx.db.transaction(async (tx) => {
        const [inserted] = await tx
          .insert(supportMessages)
          .values({
            threadId: thread.id,
            authorUserId: ctx.session.user.id,
            direction: `outbound`,
            visibility: `public`,
            body: input.body,
          })
          .returning()
        await tx
          .update(supportThreads)
          .set({ updatedAt: new Date() })
          .where(eq(supportThreads.id, thread.id))
        return inserted
      })

      const [teamRow] = await ctx.db
        .select({ name: teams.name })
        .from(teams)
        .where(eq(teams.id, thread.teamId))
        .limit(1)

      // Email outside the transaction; a failed send never loses the message.
      // Every reply carries the thread's one stable link. The delivery ledger
      // row stores no thread URL — the token exists only inside the email.
      //
      // Engagement gate (de-facto confirmed opt-in): replies are only emailed
      // once the reporter has opened their magic conversation link at least
      // once (last_reporter_seen_at, stamped by the /support/$token page). An
      // address whose owner never engages receives at most ONE email ever —
      // the submit confirmation that carried the link. The reply itself still
      // lands on the thread page either way.
      //
      // Presence gate (EXP-237): while the reporter's tab is live-polling the
      // page (heartbeat within REPORTER_PRESENCE_WINDOW_MS), this reply
      // appears in their browser within seconds — emailing it too would spam
      // a rapid back-and-forth, so it is skipped. The moment the tab hides or
      // closes the heartbeat lapses and emails resume. Worst case a reporter
      // closes the tab in the seconds before a reply lands: one missed email
      // for a conversation they were watching moments earlier, still
      // reachable through the stable magic link in every prior email.
      const reporterViewing = isReporterActivelyViewing(
        thread.lastReporterSeenAt
      )
      let reporterEmailed = false
      if (thread.lastReporterSeenAt && !reporterViewing) {
        try {
          const result = await sendSupportReplyEmail({
            to: thread.reporterEmail,
            boardName: teamRow?.name ?? `the team`,
            replyText: input.body,
            threadUrl: supportThreadUrl(mintSupportToken(thread.id)),
          })
          reporterEmailed = result.delivered
          const [delivery] = await ctx.db
            .insert(emailDeliveries)
            .values({
              userId: null,
              toEmail: thread.reporterEmail,
              issueId: null,
              kind: `support_reply`,
              status: deliveryStatus(result),
              provider: result.provider,
              providerMessageId: result.messageId,
              sentAt: result.delivered ? new Date() : null,
            })
            .returning({ id: emailDeliveries.id })
          if (delivery) {
            await ctx.db
              .update(supportMessages)
              .set({ emailDeliveryId: delivery.id })
              .where(eq(supportMessages.id, message.id))
          }
        } catch (err) {
          console.error(`[helpdesk] reply email failed:`, err)
        }
      }

      return { message, reporterEmailed, reporterViewing }
    }),

  // Internal note: member-only annotation. Never emailed, never visible on
  // the anonymous page, no token churn.
  note: authedProcedure
    .input(
      z.object({ threadId: z.string().uuid(), body: messageBodySchema })
    )
    .mutation(async ({ ctx, input }) => {
      const thread = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      const [message] = await ctx.db
        .insert(supportMessages)
        .values({
          threadId: thread.id,
          authorUserId: ctx.session.user.id,
          direction: `outbound`,
          visibility: `internal`,
          body: input.body,
        })
        .returning()
      return { message }
    }),

  // Close: resolve the ticket + revoke the magic link (transcript stays
  // readable, replies rejected). A linked escalated issue is deliberately
  // untouched — its lifecycle is the board's business.
  close: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const thread = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      await ctx.db.transaction(async (tx) => {
        await closeThreadInTx(tx, thread.id)
      })
      return { ok: true as const }
    }),

  // Reopen: ticket back to open + reinstate the revoked magic link — the
  // reporter's existing emails work again (the token never changes).
  reopen: authedProcedure
    .input(z.object({ threadId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const thread = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      await ctx.db.transaction(async (tx) => {
        await reopenThreadInTx(tx, thread.id)
      })
      return { ok: true as const }
    }),

  // Escalate: file an ordinary issue on a board of this team and link it
  // to the ticket. The issue is a normal tracker citizen from then on (its
  // status never mirrors the thread's). One escalation per ticket.
  escalate: authedProcedure
    .input(
      z.object({
        threadId: z.string().uuid(),
        boardId: z.string().uuid(),
        title: z.string().trim().min(1).max(500).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const thread = await loadThreadForMember(
        ctx.session.user.id,
        input.threadId
      )
      if (thread.linkedIssueId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `This ticket already has a linked issue`,
        })
      }
      const board = await getBoardTeamId(input.boardId)
      if (board.teamId !== thread.teamId) {
        throw new TRPCError({
          code: `BAD_REQUEST`,
          message: `Board must belong to the ticket's team`,
        })
      }

      // The escalated issue opens with the reporter's opening message as its
      // description (plain text is valid GFM) plus a provenance line.
      const [opening] = await ctx.db
        .select({ body: supportMessages.body })
        .from(supportMessages)
        .where(eq(supportMessages.threadId, thread.id))
        .orderBy(supportMessages.createdAt)
        .limit(1)
      const reporter =
        thread.reporterName?.trim() || thread.reporterEmail || `a reporter`
      const description = [
        opening?.body ?? ``,
        ``,
        `---`,
        ``,
        `Escalated from a support ticket from ${reporter}.`,
      ]
        .join(`\n`)
        .trim()

      // EXP-50 parity with issues.create: solo teams default-assign
      // their only human member.
      const assigneeId = await getSoleHumanMemberId(thread.teamId)

      const result = await ctx.db.transaction(async (tx) => {
        const txId = await generateTxId(tx)
        const [issue] = await tx
          .insert(issues)
          .values({
            boardId: input.boardId,
            // populate_issue_board_context overwrites with board-derived
            // truth; passed to satisfy the NOT NULL insert contract.
            teamId: thread.teamId,
            title: input.title ?? thread.title,
            status: `backlog`,
            priority: `none`,
            assigneeId,
            description,
            creatorId: ctx.session.user.id,
          })
          .returning({
            id: issues.id,
            identifier: issues.identifier,
            title: issues.title,
          })
        await ensureSubscribed(tx, {
          issueId: issue.id,
          userId: ctx.session.user.id,
          teamId: thread.teamId,
          source: `creator`,
        })
        await tx
          .update(supportThreads)
          .set({ linkedIssueId: issue.id, updatedAt: new Date() })
          .where(eq(supportThreads.id, thread.id))
        return { issue, txId }
      })

      return result
    }),
})
