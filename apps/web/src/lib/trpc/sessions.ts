import { isPersistent } from "@exp/domain-contract/session-kind"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
import type { db } from "@/db/connection"
import { clientAtOrBelow } from "@/lib/client-version"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  codingSessionBlockedSchema,
  MESSAGE_BODY_MAX,
  messageCardSchema,
  QUESTION_CARD_OPTIONS_MAX,
  questionCardOptionDescriptionsSchema,
  questionCardOptionSchema,
  sessionsConfigureInputSchema,
  sessionsSendInputSchema,
  sessionStatusSchema,
  type CodingSessionBlocked,
  type MessageCard,
  type SessionsSendResult,
} from "@exp/db-schema/domain"
import {
  attachments,
  messages,
  sessionReads,
  sessions,
  type Session,
} from "@/db/schema"
import {
  assertCanSteer,
  assertDeviceHost,
  loadSessionForUser,
} from "@/lib/sessions/access"
import { composeSessionBrief } from "@/lib/sessions/brief"
import { configureSession, mcpPicks } from "@/lib/sessions/configure"
import {
  cardAnswerInput,
  loadCardForAnswer,
  stampAnsweredCard,
} from "@/lib/sessions/answer"
import { BUS_COPY, busAsk, busLine, busRelayTurn } from "@/lib/sessions/bus"
import {
  issueIdentifiers,
  linkIssues,
  manualLinkText,
  unlinkIssue,
} from "@/lib/sessions/issues"
import {
  deleteSession,
  endDeadSessionInTx,
  endSession,
  stopSession,
} from "@/lib/sessions/lifecycle"
import { parseMcpSessionHeader } from "@/lib/mcp/session-header"
import { answeredOwnCard, pendingInput, postAgentMessage } from "@/lib/sessions/messages"
import { clearBlockedInTx } from "@/lib/sessions/watchdog"
import {
  dismissReplies,
  dismissSuggestion,
  fileSuggestion,
  sendReply,
  startAllSuggestions,
  startSuggestion,
} from "@/lib/sessions/suggestions"
import { issuesRouter } from "@/lib/trpc/issues"
import {
  effectiveCli,
  isRunnerModel,
  sendSession,
} from "@/lib/sessions/send"
import {
  fireAndForgetSessionMessageNotify,
  notifySessionBlocked,
} from "@/lib/integrations/notifications"

// EXP-1291/NEW-1: THE session router. A thread IS a root session that
// WORKS; a team agent is a root of kind `agent`; children are sub-sessions.
// ONE entrypoint starts and messages them all: `sessions.send` (the
// composer, Start on an issue, an action, a trigger firing, an agent spawn,
// a follow-up, an answer over the bus). The database is the bus: a new
// session is a `queued` row naming its device, a message a `messages` row;
// the device picks both up off the `sessions` / `messages` shapes and
// reports back through the DEVICE procedures below (claim/report/reply/
// consume), each of which posts the transition into the parent's
// conversation (`lib/sessions/bus.ts`). Reads ride the shapes; tRPC
// returns ids + txids only.

const uuid = z.string().uuid()

const suggestionRef = z.object({ messageId: uuid, itemId: z.string().min(1).max(64) })

/** The cards a DEVICE may post (the server owns event/session cards). */
const DEVICE_CARD_KINDS = new Set([`question`, `plan`, `subagent`, `picture`])

// COMPAT (EXP-1334): delete once CLIENT_MIN_VERSION_IOS > 0.14.61 AND
// CLIENT_MIN_VERSION_ANDROID > 0.14.63 AND CLIENT_MIN_VERSION_DESKTOP/_CLI >
// 0.14.71 (the last clients shipped before EXP-1334's link message). Those
// clients call `linkIssues` as a plain "Link an issue": they know nothing of
// the `Link #IDENT` message, and on an ENDED root that message would make
// `sendSession` revive the thread (FEED-134: the root resumes with the
// message pending, which never runs when the host is offline). For them the
// link on an ended thread stays silent; current clients, the web app
// included, keep the revive. The link itself always lands.
const LINK_MESSAGE_LEGACY_CLIENTS = {
  ios: `0.14.61`,
  android: `0.14.63`,
  desktop: `0.14.71`,
  cli: `0.14.71`,
} as const

async function legacyLinkOnEndedRoot(
  ctx: { db: typeof db; request: Pick<Request, `headers`> },
  session: Pick<Session, `id` | `rootId` | `status`>
): Promise<boolean> {
  if (!clientAtOrBelow(ctx.request, LINK_MESSAGE_LEGACY_CLIENTS)) return false
  if (session.id === session.rootId) return session.status === `ended`
  const [root] = await ctx.db
    .select({ status: sessions.status })
    .from(sessions)
    .where(eq(sessions.id, session.rootId))
    .limit(1)
  return root?.status === `ended`
}

/** The device procedures' load (`report`, `reply`, `consume`): a session
 *  that no longer EXISTS (a deleted chat whose kill the device missed) is
 *  `null`, answered like an ended one (a no-op) so the device stops cleanly;
 *  an existing row keeps the full access check. */
async function loadDeviceSession(
  ctx: { db: typeof db; session: { user: { id: string } } },
  sessionId: string
): Promise<Session | null> {
  const [row] = await ctx.db.select({ id: sessions.id }).from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!row) return null
  const session = await loadSessionForUser(ctx.session.user.id, sessionId)
  await assertDeviceHost(ctx.session.user.id, session)
  return session
}

const noOpTx = (database: typeof db) => database.transaction(async (tx) => ({ txid: await generateTxId(tx) }))

export const sessionsRouter = router({
  // ── people (and agents over MCP) ──────────────────────────────────────────

  /** THE entrypoint. → { sessionId, messageId, txid }. */
  send: authedProcedure
    .input(sessionsSendInputSchema)
    .mutation(({ ctx, input }): Promise<SessionsSendResult & { txid: number }> =>
      sendSession({
        userId: ctx.session.user.id,
        input,
        // EXP-1323: a session's own key stamps its messages (validated in
        // `sendSession`: only the session's starter or host counts). Over
        // tRPC the header only STAMPS `author_session_id` on a person's row;
        // the agent-authored session-input path is MCP's alone (`viaMcp`),
        // so a person's text can never turn into "from session A".
        callerSessionId: ctx.request ? parseMcpSessionHeader(ctx.request) : null,
        viaMcp: ctx.viaMcp === true,
      })
    ),

  /** EXP-1332: the caller read this session (a thread = its root's row).
   *  Any session the caller may see; upserts `read_at = now`. */
  markRead: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        await tx
          .insert(sessionReads)
          .values({ userId, sessionId: session.id, readAt: now })
          .onConflictDoUpdate({
            target: [sessionReads.userId, sessionReads.sessionId],
            set: { readAt: now, updatedAt: now },
          })
        return { txid }
      })
    }),

  // ── EXP-1328: the suggestions card (posted by MCP `sessions_suggest`) ──

  /** Start = a child of the card's session (`where: child`, the item's text
   *  as its request, its title on the child); Do it here = a message to the
   *  card's session. Stamps the item in the same transaction. */
  startSuggestion: authedProcedure
    .input(
      suggestionRef.extend({
        where: z.enum([`child`, `here`]),
        deviceId: z.string().min(1).max(128).optional(),
        account: z.string().min(1).max(64).optional(),
        model: z.string().max(64).optional(),
      })
    )
    .mutation(({ ctx, input }) => startSuggestion(ctx.session.user.id, input)),

  /** Start all = a child per open item. */
  startAllSuggestions: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(({ ctx, input }) => startAllSuggestions(ctx.session.user.id, input.messageId)),

  dismissSuggestion: authedProcedure
    .input(suggestionRef)
    .mutation(({ ctx, input }) => dismissSuggestion(ctx.session.user.id, input)),

  /** File as issue: on the thread's board, linked to the thread. */
  fileSuggestion: authedProcedure
    .input(suggestionRef)
    .mutation(({ ctx, input }) =>
      fileSuggestion(ctx.session.user.id, input, async (args) => {
        const { issue } = await issuesRouter.createCaller(ctx).create(args)
        return { issueId: issue.id }
      })
    ),

  // ── EXP-1211: quick replies (the `replies` card above the composer) ──

  /** A tap: the reply's text as the next message to the card's session. */
  sendReply: authedProcedure
    .input(suggestionRef)
    .mutation(({ ctx, input }) => sendReply(ctx.session.user.id, input)),

  dismissReplies: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(({ ctx, input }) => dismissReplies(ctx.session.user.id, input.messageId)),

  /** EXP-1334: change a live session's launch options mid-session (plan
   *  mode, effort, ultracode, computer use, MCP picks). Same rule as
   *  messaging it; an ended session refuses. The host device applies them:
   *  claude's plan mode at once, the rest at the next turn. */
  configure: authedProcedure
    .input(sessionsConfigureInputSchema)
    .mutation(({ ctx, input }) => configureSession(ctx.session.user.id, input)),

  /** Interrupt the session's current turn (it stays live, goes idle). Every
   *  member may Stop a SHARED session; a private one only its starter.
   *  → {txid, delivered}: false = the relay reached no device. */
  stop: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(({ ctx, input }) =>
      stopSession({ userId: ctx.session.user.id, sessionId: input.sessionId })
    ),

  /** End a session explicitly (status `ended`; agents refuse). Ending a
   *  ROOT ends its whole tree. */
  end: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const { txid } = await endSession({
        userId: ctx.session.user.id,
        sessionId: input.sessionId,
      })
      return { txid }
    }),

  /** Chats: delete a ROOT chat and its whole tree (what is live ends
   *  first). The starter or a team owner; agent conversations and app
   *  sessions refuse (BAD_REQUEST). */
  delete: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .mutation(({ ctx, input }): Promise<{ txid: number }> =>
      deleteSession({ userId: ctx.session.user.id, sessionId: input.sessionId })
    ),

  /** Link issues to a session (NEW-1: the rows sit on the session that
   *  holds them; the thread header's "+" names the root). */
  linkIssues: authedProcedure
    .input(z.object({ sessionId: uuid, issueIds: z.array(uuid).min(1).max(30) }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      await assertCanSteer(userId, session)
      const { txid, identifiers } = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const { linked } = await linkIssues(tx, {
          sessionId: session.id,
          issueIds: input.issueIds,
          source: `user`,
          addedBy: userId,
          addedBySessionId: null,
        })
        return { txid, identifiers: await issueIdentifiers(tx, linked) }
      })
      // EXP-1334: a manual link is the person asking for the work: `Link
      // #A #B` wakes the session, which takes it (MCP and start links stay
      // quiet). The link stands even if the message fails.
      const text = manualLinkText(identifiers)
      if (text && !(await legacyLinkOnEndedRoot(ctx, session))) {
        try {
          await sendSession({ userId, input: { sessionId: session.id, text } })
        } catch (err) {
          console.warn(`[sessions.linkIssues] the link message to ${session.id} failed`, err)
        }
      }
      return { txid }
    }),

  unlinkIssue: authedProcedure
    .input(z.object({ sessionId: uuid, issueId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      await assertCanSteer(userId, session)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await unlinkIssue(tx, { sessionId: session.id, issueId: input.issueId, removedBy: userId })
        return { txid }
      })
    }),

  /** Answer a `question` card (text) or decide a `plan` card IN the card;
   *  the device delivers it to the agent off the messages shape. */
  answer: authedProcedure
    .input(cardAnswerInput)
    .mutation(async ({ ctx, input }) => {
      const { message, session, card } = await loadCardForAnswer(
        ctx.db,
        ctx.session.user.id,
        input,
        ctx.request
      )
      if (message.answeredAt) {
        throw new TRPCError({ code: `CONFLICT`, message: `Already answered` })
      }
      // The answer lives IN the card (answered_at flips): the device feeds it
      // to the agent off the messages shape, so no person message is posted.
      const result = await stampAnsweredCard(ctx.db, message.id, session.id, card)
      if (!result.claimed) {
        throw new TRPCError({ code: `CONFLICT`, message: `Already answered` })
      }
      return { txid: result.txid }
    }),

  /** EXP-1316: the DEVICE's write-back when a question/plan card was
   *  answered in the session view (over the relay), so the thread card
   *  closes too. Same card shape and auth as `answer`; idempotent (an
   *  answered card = ok, never CONFLICT). Enqueues nothing: the engine
   *  already resolved the question, so it left the engine's open cards and
   *  the device's pickup never feeds it again. Ask dock F1: a FRESH approval
   *  the device could not honour (it ran on in place) comes back as a plain
   *  approval and downgrades the stored card's `fresh` — never the reverse. */
  resolveCard: authedProcedure
    .input(cardAnswerInput)
    .mutation(async ({ ctx, input }): Promise<{ ok: true; txid?: number }> => {
      const { message, session, card } = await loadCardForAnswer(
        ctx.db,
        ctx.session.user.id,
        input,
        ctx.request
      )
      if (message.answeredAt) {
        const stored = message.card as MessageCard | null
        const downgrade =
          stored?.kind === `plan` &&
          stored.fresh === true &&
          input.decision === `approved` &&
          input.fresh !== true
        if (!downgrade) return { ok: true }
        const txid = await ctx.db.transaction(async (tx) => {
          const txid = await generateTxId(tx)
          await tx
            .update(messages)
            .set({ card: { ...stored, fresh: false }, updatedAt: new Date() })
            .where(eq(messages.id, message.id))
          return txid
        })
        return { ok: true, txid }
      }
      const result = await stampAnsweredCard(ctx.db, message.id, session.id, card)
      return result.claimed ? { ok: true, txid: result.txid } : { ok: true }
    }),

  /** Revoke a person's own message the device has not consumed yet. Its
   *  files fall back to owner-less uploads the orphan sweep reclaims. */
  unqueue: authedProcedure
    .input(z.object({ messageId: uuid }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx
          .update(attachments)
          .set({ messageId: null })
          .where(eq(attachments.messageId, input.messageId))
        const deleted = await tx
          .delete(messages)
          .where(
            and(
              eq(messages.id, input.messageId),
              eq(messages.authorKind, `user`),
              eq(messages.authorId, userId),
              isNull(messages.consumedAt)
            )
          )
          .returning({ id: messages.id })
        if (deleted.length === 0) {
          throw new TRPCError({
            code: `CONFLICT`,
            message: `The message was already delivered`,
          })
        }
        return { txid }
      })
    }),

  // NEW-2: team agents are definition rows: the `agents` router (Settings →
  // Agents); a message to one = `sessions.send({agentId})`.

  // ── the device (the daemon / desktop that runs the session) ───────────────

  /** The server-composed first prompt (fixtures/session-brief.json) and the
   *  system-append layers: runbook + team prompt + agent instructions. */
  brief: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      await assertDeviceHost(ctx.session.user.id, session)
      return composeSessionBrief(session.id)
    }),

  /** EXP-1334: the session's server-only MCP pick (sorted ids) — for
   *  whoever may steer it (the composer's MCP submenu; the host device
   *  compares it with what its run launched with between turns). The ids
   *  stay off the synced shape. */
  mcpPicks: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async ({ ctx, input }): Promise<{ mcpServerIds: string[] }> => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      await assertCanSteer(ctx.session.user.id, session)
      return { mcpServerIds: mcpPicks(session) }
    }),

  /** queued → running: the device took it (idempotent for its own device). */
  claim: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        // EXP-1252: the claiming device; another device's claim is refused
        // (the row moved away from it).
        deviceId: z.string().min(1).max(128),
        branch: z.string().max(255).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      await assertDeviceHost(ctx.session.user.id, session)
      // CONFLICT (not FORBIDDEN): the daemon's `claim_lost` treats 404/409
      // as "moved away", anything else as a start failure it posts about.
      if (input.deviceId !== session.deviceId) {
        throw new TRPCError({
          code: `CONFLICT`,
          message: `This session runs on another device`,
        })
      }
      if (session.status === `ended`) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `This session has ended` })
      }
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        // Guarded on the device the row was read on: a direct re-target
        // (`sessions.send({deviceId})` on a queued row) committing between
        // the read and this write would otherwise flip a row now queued on
        // ANOTHER device to running here — two engines on one session.
        // CONFLICT = `claim_lost`.
        // NEW-1: the watchdog's `unclaimed` flag clears with the claim.
        const unclaimed = session.blocked?.kind === `unclaimed`
        const claimed = await tx
          .update(sessions)
          .set({
            ...(session.status === `queued` ? { status: `running` as const } : {}),
            startedAt: session.startedAt ?? now,
            ...(input.branch ? { branch: input.branch } : {}),
            ...(unclaimed ? { blocked: null, needsInput: false } : {}),
            updatedAt: now,
          })
          .where(
            and(
              eq(sessions.id, session.id),
              eq(sessions.deviceId, input.deviceId),
              ne(sessions.status, `ended`)
            )
          )
          .returning({ id: sessions.id })
        if (claimed.length === 0) {
          throw new TRPCError({
            code: `CONFLICT`,
            message: `This session runs on another device`,
          })
        }
        // The brief's Request IS the session's first person message: it is
        // delivered by the claim, so the device never feeds it twice. Only a
        // FIRST start: a moved session re-queues with its messages pending.
        if (session.status === `queued` && !session.startedAt) {
          await tx.execute(sql`
            UPDATE ${messages} SET consumed_at = ${now}, updated_at = ${now}
            WHERE id = (
              SELECT id FROM ${messages}
              WHERE session_id = ${session.id} AND author_kind = 'user'
              ORDER BY created_at, id LIMIT 1
            ) AND consumed_at IS NULL`)
        }
        // NEW-1: the bus's `session_claimed` line (informational) on the
        // FIRST claim only.
        if (session.status === `queued`) {
          await busLine(tx, session, `session_claimed`, BUS_COPY.session_claimed, {
            waking: false,
            // A root nobody listens to needs no "started" line of its own.
            selfFallback: false,
          })
        }
        return { txid }
      })
    }),

  /** Device-written state: running ⇄ idle, busy, needs_input, blocked,
   *  branch, the CLI's auto-title; `ended` when the agent process is gone.
   *  NEW-1: a wall is said ONCE in the parent (the bus); a blocked row
   *  stays under Needs you until the device clears it; any report clears a
   *  watchdog `stuck` flag; an ending reports through the bus. */
  report: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        // EXP-1252: the reporting device; another device's report is a
        // no-op (a stale host after a move).
        deviceId: z.string().min(1).max(128).optional(),
        status: sessionStatusSchema.exclude([`queued`]).optional(),
        busy: z.boolean().optional(),
        needsInput: z.boolean().optional(),
        // `handled` = the device rotates past the wall itself (EXP-1005): no
        // push. Never stored.
        blocked: codingSessionBlockedSchema
          .extend({ handled: z.boolean().nullish() })
          .nullable()
          .optional(),
        branch: z.string().max(255).optional(),
        title: z.string().max(255).optional(),
        // EXP-849: the login the run ACTUALLY runs on (a start pick or a
        // rotation may move it off the row's), so the row says it and a
        // person's later pick of the old one is a real switch.
        account: z.string().min(1).max(64).optional(),
        // EXP-1334: the agent's OWN plan-mode move (claude leaving plan mode
        // once a plan is approved, or entering it): the row follows the
        // run, so every client's toggle says the mode it is really in.
        planMode: z.boolean().optional(),
        // EXP-1334: the agent's model as the run reports it (a `/model`
        // switch mid-session), so a resume launches on it. Outside the
        // agent's vocabulary (a full model id) = ignored, never refused.
        model: z.string().min(1).max(64).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadDeviceSession(ctx, input.sessionId)
      if (
        !session ||
        session.status === `ended` ||
        (input.deviceId !== undefined && input.deviceId !== session.deviceId)
      ) {
        // An ended row stays ended; the device's late reports are no-ops,
        // and so are a device's the session moved away from (and a deleted
        // row's).
        return noOpTx(ctx.db)
      }
      // A session that SURVIVES its process: an agent's conversation never
      // ends, and neither does a thread's ROOT (FEED-134: the conversation
      // IS the thread, so an app quit, a daemon stop or a crash leaves it
      // idle; only a person's End or the agent's own `sessions end` ends
      // it). A child's process going away still ends it: a worker, whose
      // report reaches its parent.
      // Release train 2026-10-23 (FEED-134 review): a root that never
      // finished a turn (`lastTurnAt` is written only below, when a turn
      // ends) is a REFUSED or failed launch: it ends, or the watchdog
      // nudges its unconsumed first message forever and the device
      // refuses again on every nudge.
      const status =
        input.status === `ended` &&
        (isPersistent(session) || (session.parentId === null && session.lastTurnAt !== null))
          ? `idle`
          : input.status
      const ending = status === `ended`
      const now = new Date()
      const { handled = false, ...wall } = input.blocked ?? {}
      const blocked = (input.blocked ? wall : input.blocked) as CodingSessionBlocked | null | undefined
      const newlyBlocked = !!blocked && !session.blocked && !handled
      // The device's wall replaces a watchdog flag; its next report with
      // no `blocked` clears a `stuck` flag (activity = not stuck).
      const nextBlocked =
        input.blocked !== undefined
          ? (blocked ?? null)
          : session.blocked?.kind === `stuck`
            ? null
            : session.blocked
      const model =
        input.model !== undefined &&
        isRunnerModel(session.cli ?? (await effectiveCli(session)), input.model)
          ? input.model
          : undefined
      const turnEnded =
        (input.busy === false && session.busy) || status === `idle` || ending
      const txid = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        if (ending) {
          // The agent process went away = the session ended: like an
          // explicit End of this one row (once: only when THIS report flips
          // it). The bus posts its report.
          await endDeadSessionInTx(tx, session)
          return txid
        }
        const needsInput =
          nextBlocked !== null
            ? true
            : input.needsInput !== undefined
              ? input.needsInput
              : session.blocked !== null && nextBlocked === null
                ? false
                : undefined
        await tx
          .update(sessions)
          .set({
            ...(status ? { status } : {}),
            // #FEED-85: idle with no explicit busy = the turn is over.
            ...(input.busy !== undefined
              ? { busy: input.busy }
              : status === `idle`
                ? { busy: false }
                : {}),
            ...(needsInput !== undefined ? { needsInput } : {}),
            ...(nextBlocked !== session.blocked ? { blocked: nextBlocked } : {}),
            ...(input.branch ? { branch: input.branch } : {}),
            ...(input.account ? { account: input.account } : {}),
            ...(input.planMode !== undefined ? { planMode: input.planMode } : {}),
            ...(model ? { model } : {}),
            // The CLI's auto-name only fills a blank title (a person's or
            // the agent's own title wins).
            ...(input.title?.trim() && !session.title ? { title: input.title.trim() } : {}),
            ...(turnEnded ? { lastTurnAt: now } : {}),
            updatedAt: now,
          })
          .where(and(eq(sessions.id, session.id), ne(sessions.status, `ended`)))
        if (session.blocked !== null && nextBlocked === null) {
          // An open card still waits: needs_input re-derives.
          await clearBlockedInTx(tx, [session.id])
        }
        if (newlyBlocked) {
          await busLine(tx, session, `session_blocked`, BUS_COPY.session_blocked(blocked!), {
            waking: true,
          })
        }
        return txid
      })
      if (newlyBlocked) void notifySessionBlocked(session.id, blocked!)
      return { txid }
    }),

  /** The agent's side of the conversation: a turn's FINAL reply text and/or
   *  a card (question, plan, subagent, picture). */
  reply: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        body: z.string().max(MESSAGE_BODY_MAX).default(``),
        card: messageCardSchema.optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const session = await loadDeviceSession(ctx, input.sessionId)
      // A deleted session: nothing to post into, a no-op (no message id).
      if (!session) return { messageId: ``, ...(await noOpTx(ctx.db)) }
      const card = (input.card ?? null) as MessageCard | null
      if (card && !DEVICE_CARD_KINDS.has(card.kind)) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `A device cannot post ${card.kind} cards` })
      }
      // A question card's `options` are labels; their descriptions ride the
      // parallel `optionDescriptions` sidecar.
      if (
        card?.kind === `question` &&
        card.options !== undefined &&
        !z.array(questionCardOptionSchema).max(QUESTION_CARD_OPTIONS_MAX).safeParse(card.options).success
      ) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid question options` })
      }
      if (
        card?.kind === `question` &&
        (card as { optionDescriptions?: unknown }).optionDescriptions !== undefined &&
        !questionCardOptionDescriptionsSchema.safeParse(
          (card as { optionDescriptions?: unknown }).optionDescriptions
        ).success
      ) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid question option descriptions` })
      }
      if (!card && !input.body.trim()) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `A reply needs a body or a card` })
      }
      const asks = card?.kind === `question` || card?.kind === `plan`
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const messageId = await postAgentMessage(tx, session, input.body, card)
        if (asks) {
          await tx
            .update(sessions)
            .set({ needsInput: true, updatedAt: new Date() })
            .where(eq(sessions.id, session.id))
          // NEW-1: the card reaches the parent and watchers (one row, two
          // views); a parent may answer what it knows.
          await busAsk(tx, session, messageId, card as Extract<MessageCard, { kind: `question` | `plan` }>)
        } else if (!card) {
          // NEW-1: the turn's reply reaches the parent and watchers.
          await busRelayTurn(tx, session, input.body)
        }
        return { messageId, txid }
      })
      if (input.body.includes(`@`)) fireAndForgetSessionMessageNotify({ messageId: result.messageId })
      return result
    }),

  /** The device delivered these pending inputs (person messages, another
   *  session's question or report) into the agent. EXP-1280: also the
   *  session's OWN answered plan/question card (`exponential_plan_propose`,
   *  `exponential_sessions_ask`) once its answer text was delivered. */
  consume: authedProcedure
    .input(z.object({ sessionId: uuid, messageIds: z.array(uuid).min(1).max(100) }))
    .mutation(async ({ ctx, input }) => {
      const session = await loadDeviceSession(ctx, input.sessionId)
      if (!session) return noOpTx(ctx.db)
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        await tx
          .update(messages)
          .set({ consumedAt: now, updatedAt: now })
          .where(
            and(
              eq(messages.sessionId, session.id),
              inArray(messages.id, input.messageIds),
              // EXP-1323: a person's message or another session's input.
              or(pendingInput(), answeredOwnCard()),
              isNull(messages.consumedAt)
            )
          )
        return { txid }
      })
    }),
})
