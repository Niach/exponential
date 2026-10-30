import { isPersistent } from "@exp/domain-contract/session-kind"
import { isAssistantConversation, turnHasMemberInput } from "@/lib/agents/assistant-chat"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
import type { db } from "@/db/connection"
import { router, authedProcedure, generateTxId } from "@/lib/trpc"
import {
  bodyWithin,
  codingSessionBlockedSchema,
  MESSAGE_BODY_MAX,
  sealedBodyMax,
  SESSIONS_SEND_MAX_MENTIONS,
  SESSIONS_SEND_MAX_WAKE_ON,
  messageCardSchema,
  QUESTION_CARD_OPTIONS_MAX,
  questionCardOptionDescriptionsSchema,
  questionCardOptionSchema,
  sessionsConfigureInputSchema,
  sessionsSendInputSchema,
  sessionsSendClientMessageIdSchema,
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
import { composeSessionBrief, stampAgentInstructionsAt } from "@/lib/sessions/brief"
import { configureSession, mcpPicks } from "@/lib/sessions/configure"
import {
  cardAnswerInput,
  loadCardForAnswer,
  stampAnsweredCard,
} from "@/lib/sessions/answer"
import { BUS_COPY, busAsk, busLine, busListenerTrees, busRelayTurn } from "@/lib/sessions/bus"
import { isSealedBody, isSealedCard, validSealedBody, validSealedCard } from "@/lib/sessions/sealed"
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
  relayKillSessionsBestEffort,
  stopSession,
} from "@/lib/sessions/lifecycle"
import { parseMcpSessionHeader } from "@/lib/mcp/session-header"
import { escalateTarget } from "@/lib/sessions/escalate"
import { sendTarget } from "@/lib/sessions/send-target"
import { extractMentionEmails } from "@/lib/mention-refs"
import { answeredOwnCard, editAgentTurnText, patchBusCopies, pendingInput, postAgentMessage, inSession, sessionConversation } from "@/lib/sessions/messages"
import { clearBlockedInTx, interruption, INTERRUPTIONS } from "@/lib/sessions/watchdog"
import {
  dismissReplies,
  dismissSuggestion,
  fileSuggestion,
  sendReply,
  startAllSuggestions,
  startSuggestion,
} from "@/lib/sessions/suggestions"
import { issuesRouter } from "@/lib/trpc/issues"
import { yoloEdge } from "@/lib/yolo-edge"
import {
  effectiveCli,
  isRunnerModel,
  sendSession,
} from "@/lib/sessions/send"
import {
  fireAndForgetSessionMessageNotify,
  notifySessionAsk,
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

// NEW-57: `meeting: true` rides the tRPC send alone (meeting.json
// RESIDENCY); the shared schema (and with it the MCP tool) never has it.
// F21: so does `clientMessageId`, the sender's idempotency key.
const sessionsSendTrpcInputSchema = z.intersection(
  sessionsSendInputSchema,
  z.object({ meeting: z.literal(true).optional(), clientMessageId: sessionsSendClientMessageIdSchema.optional() })
)

const noOpTx = (database: typeof db) => database.transaction(async (tx) => ({ txid: await generateTxId(tx) }))

/** F32: `after` mentions someone `before` did not (no `before` = a new
 *  message: any mention counts). */
export function addsMention(before: string | null, after: string): boolean {
  if (before === null) return true
  const had = new Set(extractMentionEmails(before))
  return extractMentionEmails(after).some((email) => !had.has(email))
}

/** EXP-1458: `ids` names someone `before` did not (null = a new message:
 *  any id counts). Kept here, not imported from notifications (which the
 *  device suites mock whole). */
export function addsMentionId(before: readonly string[] | null, ids: readonly string[]): boolean {
  if (ids.length === 0) return false
  const had = new Set(before ?? [])
  return ids.some((id) => !had.has(id))
}

// EXP-1458: one copy per listening TREE (`sessions.listeners`).
const listenerCopiesSchema = z
  .array(z.object({ rootId: uuid, body: z.string().min(1).max(sealedBodyMax(MESSAGE_BODY_MAX)) }))
  .max(SESSIONS_SEND_MAX_WAKE_ON)

/** EXP-1458: a writer's body is plaintext or a well-formed envelope; an
 *  ask text and every listener copy are envelopes. */
function assertSealedWrite(body: string, askText?: string, copies?: ReadonlyArray<{ body?: string }>) {
  const bad = (what: string) => new TRPCError({ code: `BAD_REQUEST`, message: `Invalid sealed ${what}` })
  if (!validSealedBody(body)) throw bad(`body`)
  if (askText !== undefined && (!isSealedBody(askText) || !validSealedBody(askText))) throw bad(`ask text`)
  for (const copy of copies ?? []) {
    if (!copy.body || !isSealedBody(copy.body) || !validSealedBody(copy.body)) throw bad(`listener copy`)
  }
}

export const sessionsRouter = router({
  // ── people (and agents over MCP) ──────────────────────────────────────────

  /** THE entrypoint. → { sessionId, messageId, txid }. */
  send: authedProcedure
    .input(sessionsSendTrpcInputSchema)
    .mutation(({ ctx, input: { meeting, clientMessageId, ...input } }): Promise<SessionsSendResult & { txid: number }> =>
      sendSession({
        userId: ctx.session.user.id,
        input,
        clientMessageId,
        // NEW-57: a meeting transcript batch (tRPC only: the MCP tool never
        // carries it; `sendSession` refuses it over MCP anyway).
        meeting: meeting === true,
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

  /** S-3 (stories.json): the caller saw a story of this ROOT's tree; the
   *  root's `story_seen_at` keeps the greatest `at`. A new row's `read_at`
   *  is the epoch, so seeing a story never marks the chat read. */
  markStoriesSeen: authedProcedure
    .input(z.object({ sessionId: uuid, at: z.number().int().nonnegative() }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const userId = ctx.session.user.id
      const session = await loadSessionForUser(userId, input.sessionId)
      const at = new Date(Math.min(input.at, Date.now()))
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        await tx
          .insert(sessionReads)
          .values({ userId, sessionId: session.rootId, readAt: new Date(0), storySeenAt: at })
          .onConflictDoUpdate({
            target: [sessionReads.userId, sessionReads.sessionId],
            set: {
              storySeenAt: sql`greatest(coalesce(${sessionReads.storySeenAt}, 'epoch'::timestamptz), ${at.toISOString()}::timestamptz)`,
              updatedAt: new Date(),
            },
          })
        return { txid }
      })
    }),

  /** #EXP-1388: the Chats selection's Mark read / Mark unread. Read = the
   *  `markRead` upsert per chat; unread = the root's read just before its
   *  newest non-event agent message (one unread; none = unchanged). */
  setRead: authedProcedure
    .input(z.object({ sessionIds: z.array(uuid).min(1).max(200), read: z.boolean() }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const userId = ctx.session.user.id
      const chats: Session[] = []
      for (const id of new Set(input.sessionIds)) chats.push(await loadSessionForUser(userId, id))
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        for (const chat of chats) {
          let readAt = now
          if (!input.read) {
            const [newest] = await tx
              .select({ createdAt: messages.createdAt })
              .from(messages)
              .where(
                and(
                  inSession(chat.id),
                  eq(messages.authorKind, `agent`),
                  sql`(${messages.card} IS NULL OR ${messages.card}->>'kind' <> 'event')`
                )
              )
              .orderBy(sql`${messages.createdAt} DESC`)
              .limit(1)
            if (!newest) continue
            readAt = new Date(newest.createdAt.getTime() - 1)
          }
          await tx
            .insert(sessionReads)
            .values({ userId, sessionId: chat.id, readAt })
            .onConflictDoUpdate({
              target: [sessionReads.userId, sessionReads.sessionId],
              set: { readAt, updatedAt: now },
            })
        }
        return { txid }
      })
    }),

  /** #EXP-1388: archive / unarchive chats for the caller (per viewer, on
   *  the ROOT's read row; a first row starts unread, read_at = epoch). */
  setArchived: authedProcedure
    .input(z.object({ sessionIds: z.array(uuid).min(1).max(200), archived: z.boolean() }))
    .mutation(async ({ ctx, input }): Promise<{ txid: number }> => {
      const userId = ctx.session.user.id
      const chats: Session[] = []
      for (const id of new Set(input.sessionIds)) {
        const chat = await loadSessionForUser(userId, id)
        if (chat.parentId !== null) {
          throw new TRPCError({ code: `BAD_REQUEST`, message: `Only a chat (a root session) can be archived` })
        }
        chats.push(chat)
      }
      return ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const now = new Date()
        const archivedAt = input.archived ? now : null
        for (const chat of chats) {
          await tx
            .insert(sessionReads)
            .values({ userId, sessionId: chat.id, readAt: new Date(0), archivedAt })
            .onConflictDoUpdate({
              target: [sessionReads.userId, sessionReads.sessionId],
              set: { archivedAt, updatedAt: now },
            })
        }
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
      if (text) {
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
        input
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
        input
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
      const brief = await composeSessionBrief(session.id)
      // #EXP-1383 (chats.json `agentHint`): the instructions this launch
      // runs on are the ones of THIS moment (stamped after the compose read
      // them, so an edit racing the fetch still reads as changed).
      await stampAgentInstructionsAt(session.id)
      return brief
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
              WHERE conversation_id = ${sessionConversation(session.id)} AND author_kind = 'user'
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
   *  watchdog `stuck` flag (EXP-1374: and an interruption, `device_offline`
   *  or `restarted`: the device is back on it); an ending reports through
   *  the bus. EXP-1374: `background` = busy only because background tasks
   *  still run after the turn (a not-busy report clears it). */
  report: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        // EXP-1252: the reporting device; another device's report is a
        // no-op (a stale host after a move).
        deviceId: z.string().min(1).max(128).optional(),
        status: sessionStatusSchema.exclude([`queued`]).optional(),
        busy: z.boolean().optional(),
        background: z.boolean().optional(),
        needsInput: z.boolean().optional(),
        // `handled` = the device rotates past the wall itself (EXP-1005): no
        // push. Never stored.
        blocked: codingSessionBlockedSchema
          .extend({ handled: z.boolean().nullish() })
          .nullable()
          .optional(),
        branch: z.string().max(255).optional(),
        title: z.string().max(255).optional(),
        // EXP-1458: the CLI's auto-name sealed to the tree (a body
        // envelope; `sessions.sealed_title`), sent instead of `title` once
        // the root has a key. Fills a blank one only, like `title`.
        sealedTitle: z
          .string()
          .max(sealedBodyMax(255))
          .refine((value) => isSealedBody(value) && validSealedBody(value), `not a sealed title`)
          .optional(),
        // EXP-849: the login the run ACTUALLY runs on (a start pick or a
        // rotation may move it off the row's), so the row says it and a
        // person's later pick of the old one is a real switch.
        account: z.string().min(1).max(64).optional(),
        // S-3: plan mode is gone; an older device still reports it
        // (accepted, never stored).
        planMode: z.boolean().optional(),
        // EXP-1334: the agent's model as the run reports it (a `/model`
        // switch mid-session), so a resume launches on it. Outside the
        // agent's vocabulary (a full model id) = ignored, never refused.
        model: z.string().min(1).max(64).optional(),
        // EXP-1418: on a turn-end report, whether an interrupt (a person's
        // Stop) cut that turn. `false` = it finished by itself: a Stop that
        // raced it (stamped while the turn was ending) holds nothing.
        // Absent (older devices) = the Stop counts.
        interrupted: z.boolean().optional(),
        // NEW-17: the run's process went away WITHOUT the agent finishing
        // (`process_lost` = killed/OOM/crashed, `daemon_restart` = the host
        // restarted under it). Sent with `status: "ended"` or alone; the
        // session reads interrupted (`restarted`), never "done".
        lost: z.enum([`process_lost`, `daemon_restart`]).optional(),
        // NEW-17: with `lost`, the device's own word that a turn was still
        // open (the row can already read idle off a late status tick).
        midTurn: z.boolean().optional(),
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
      // NEW-17: the device says the run's PROCESS went away without the
      // agent finishing (killed / OOM / the daemon restarted under it). The
      // session never ends on it and never reads "done": idle, and when it
      // was mid-turn the `restarted` INTERRUPTION (resume: its device
      // continues it; the watchdog's `resumeMinutes` rule puts it on a
      // person if it does not), said once to its parent. Not a yolo edge.
      // The device's explicit `midTurn` wins over the row's state.
      const midTurn = input.midTurn ?? (session.busy || session.status === `running`)
      const lost = input.lost
      if (lost) {
        const now = new Date()
        // A usage wall the device wrote stays (it names the real blocker).
        const flaggable =
          session.blocked === null ||
          session.blocked.kind === `stuck` ||
          (INTERRUPTIONS as readonly string[]).includes(session.blocked.kind)
        const txid = await ctx.db.transaction(async (tx) => {
          const txid = await generateTxId(tx)
          // A watchdog `stuck` flag held needs_input: re-derived first.
          if (session.blocked?.kind === `stuck`) await clearBlockedInTx(tx, [session.id], `stuck`)
          await tx
            .update(sessions)
            .set({
              status: `idle`,
              busy: false,
              background: false,
              // A Stop's pending settle has no turn left to settle.
              stoppedAt: null,
              ...(midTurn && flaggable
                ? { blocked: interruption(`restarted`, now, true, now) }
                : {}),
              lastActivityAt: now,
              updatedAt: now,
            })
            .where(and(eq(sessions.id, session.id), ne(sessions.status, `ended`)))
          // S-3: the interruption is silent (no bus line ×4).
          return txid
        })
        return { txid }
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
      // no `blocked` clears a `stuck` flag (activity = not stuck) and an
      // interruption (EXP-1374: the device continued the run).
      const nextBlocked =
        input.blocked !== undefined
          ? (blocked ?? null)
          : session.blocked?.kind === `stuck` ||
              (INTERRUPTIONS as readonly string[]).includes(session.blocked?.kind ?? ``)
            ? null
            : session.blocked
      const model =
        input.model !== undefined &&
        isRunnerModel(session.cli ?? (await effectiveCli(session)), input.model)
          ? input.model
          : undefined
      const turnEnded =
        (input.busy === false && session.busy) || status === `idle` || ending
      // EXP-1413: the yolo edge = a REAL busy → idle transition (or the
      // process going); a repeated idle report is none, and neither is the
      // end of a turn a person's Stop interrupted (`stoppedAt`).
      const nowBusy =
        input.busy !== undefined ? input.busy : status === `idle` ? false : session.busy
      // EXP-1418: a turn = busy seen true OR the row `running` (a device
      // that never reports `busy: true` still flips running → idle); a Stop
      // the device says cut nothing (`interrupted: false`) holds nothing.
      const turnWasLive = session.busy || (session.status === `running` && status === `idle`)
      const stopSettled = !!session.stoppedAt && !nowBusy && input.interrupted === false
      const yoloTurnEnd = ending || (turnWasLive && !nowBusy && (!session.stoppedAt || stopSettled))
      let endedHere = false
      const txid = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        if (ending) {
          // The agent process went away = the session ended: like an
          // explicit End of this one row (once: only when THIS report flips
          // it). The bus posts its report.
          endedHere = await endDeadSessionInTx(tx, session)
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
            // EXP-1374: never background without busy.
            ...(input.busy === false || (input.busy === undefined && status === `idle`)
              ? { background: false }
              : input.background !== undefined
                ? { background: input.background }
                : {}),
            ...(needsInput !== undefined ? { needsInput } : {}),
            ...(nextBlocked !== session.blocked ? { blocked: nextBlocked } : {}),
            ...(input.branch ? { branch: input.branch } : {}),
            ...(input.account ? { account: input.account } : {}),
            ...(model ? { model } : {}),
            // The CLI's auto-name only fills a blank title (a person's or
            // the agent's own title wins).
            ...(input.title?.trim() && !session.title ? { title: input.title.trim() } : {}),
            ...(input.sealedTitle && !session.sealedTitle && !session.title ? { sealedTitle: input.sealedTitle } : {}),
            ...(turnEnded ? { lastTurnAt: now } : {}),
            // EXP-1413: working again = the Stop is settled.
            ...((nowBusy || stopSettled) && session.stoppedAt ? { stoppedAt: null } : {}),
            lastActivityAt: now,
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
      // EXP-1415: a watched room learns the end like any other End.
      if (endedHere) await relayKillSessionsBestEffort([session.id])
      // EXP-1413: a turn ending (or a child's process going) is a yolo edge.
      if (yoloTurnEnd) yoloEdge(session.rootId)
      return { txid }
    }),

  /** The agent's side of the conversation: a turn's FINAL reply text and/or
   *  a card (question, plan, subagent, picture). */
  reply: authedProcedure
    .input(
      z.object({
        sessionId: uuid,
        body: z
          .string()
          .max(sealedBodyMax(MESSAGE_BODY_MAX))
          .refine(bodyWithin(MESSAGE_BODY_MAX), `body is too long`)
          .default(``),
        card: messageCardSchema.optional(),
        // EXP-1458: the members a (sealed) body mentions (user ids).
        mentionUserIds: z.array(z.string().min(1).max(64)).max(SESSIONS_SEND_MAX_MENTIONS).optional(),
        // EXP-1458: a sealed reply's (or ask's) copies for the listeners in
        // OTHER trees (`busListeners`), each sealed to that tree.
        listenerCopies: listenerCopiesSchema.optional(),
        // EXP-1458: a sealed question/plan card's ask text, sealed to this
        // tree (what its parent's agent reads; absent = the neutral line).
        askText: z.string().max(sealedBodyMax(MESSAGE_BODY_MAX)).optional(),
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
      // EXP-1458: sealed content is stored as is; only its envelope and
      // the card's clear part are checked.
      assertSealedWrite(input.body, input.askText, input.listenerCopies)
      const sealedCard = card !== null && isSealedCard(card)
      if (sealedCard && !validSealedCard(card as Record<string, unknown>)) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid sealed card` })
      }
      // A question card's `options` are labels; their descriptions ride the
      // parallel `optionDescriptions` sidecar.
      if (
        !sealedCard &&
        card?.kind === `question` &&
        card.options !== undefined &&
        !z.array(questionCardOptionSchema).max(QUESTION_CARD_OPTIONS_MAX).safeParse(card.options).success
      ) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid question options` })
      }
      if (
        !sealedCard &&
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
      // #EXP-1439 (agent-settings.json `assistant.chat`): an Assistant's
      // text lands only in a turn its member started; a check-in or a
      // child's line updates its status card, never the chat.
      if (
        !card &&
        (await isAssistantConversation(ctx.db, session)) &&
        !(await turnHasMemberInput(ctx.db, session.id))
      ) {
        return { messageId: ``, ...(await noOpTx(ctx.db)) }
      }
      const result = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        // S-3: an agent conversation keeps ONE text per person message: a
        // later turn's text edits it (its bus copies follow), never a new
        // bubble.
        const edited =
          !card && session.agentId !== null && session.id === session.rootId
            ? await editAgentTurnText(tx, session.id, input.body)
            : null
        if (edited) await patchBusCopies(tx, edited.id, edited.before, input.body.trim(), input.listenerCopies)
        const messageId = edited?.id ?? (await postAgentMessage(tx, session, input.body, card))
        // EXP-1458: who the text mentions, in clear (a sealed body's
        // `@email`s are unreadable here); an edit replaces the set.
        if (input.mentionUserIds !== undefined || edited) {
          await tx
            .update(messages)
            .set({ mentionUserIds: input.mentionUserIds?.length ? input.mentionUserIds : null })
            .where(eq(messages.id, messageId))
        }
        // EXP-1374: a message is a sign of life (the stuck rule).
        await tx.update(sessions).set({ lastActivityAt: new Date() }).where(eq(sessions.id, session.id))
        if (asks) {
          await tx
            .update(sessions)
            .set({ needsInput: true, updatedAt: new Date() })
            .where(eq(sessions.id, session.id))
          // NEW-1: the card reaches the parent and watchers (one row, two
          // views); a parent may answer what it knows.
          await busAsk(tx, session, messageId, card as Extract<MessageCard, { kind: `question` | `plan` }>, {
            askText: input.askText,
            listenerCopies: input.listenerCopies,
          })
        } else if (!card && !edited) {
          // NEW-1: the turn's reply reaches the parent and watchers.
          await busRelayTurn(tx, session, input.body, messageId, input.listenerCopies)
        }
        // EXP-1458: the listeners in other trees a sealed line reaches,
        // so the device knows whom it seals copies for.
        const listeners =
          isSealedBody(input.body) || sealedCard ? await busListenerTrees(tx, session) : []
        return {
          messageId,
          txid,
          listeners,
          before: edited?.before ?? null,
          mentionsBefore: edited?.mentionUserIds ?? null,
        }
      })
      // An in-place edit re-notifies only for a mention it ADDED (a later
      // turn's text repeats the earlier ones). EXP-1458: a sealed body's
      // mentions are the writer's `mentionUserIds`.
      const mentions = isSealedBody(input.body)
        ? addsMentionId(result.before === null ? null : result.mentionsBefore, input.mentionUserIds ?? [])
        : (input.body.includes(`@`) && addsMention(result.before, input.body)) ||
          addsMentionId(result.before === null ? null : result.mentionsBefore, input.mentionUserIds ?? [])
      if (mentions) {
        fireAndForgetSessionMessageNotify({ messageId: result.messageId })
      }
      // #VAPP-45: a root's ask pushes its owner (the question text sealed).
      if (asks) {
        void notifySessionAsk({
          sessionId: session.id,
          card: card as Extract<MessageCard, { kind: `question` | `plan` }>,
          sealedAskText: input.askText,
        })
      }
      return {
        messageId: result.messageId,
        txid: result.txid,
        ...(result.listeners.length ? { listeners: result.listeners } : {}),
      }
    }),

  /** EXP-1458: the trees OUTSIDE this session's own that its sealed lines
   *  reach (its parent's, its watchers'), with their current key: the
   *  device seals one copy per tree (`reply {listenerCopies}`, `end
   *  {copies}`). A keyless tree (null key) gets the neutral line. */
  listeners: authedProcedure
    .input(z.object({ sessionId: uuid }))
    .query(async ({ ctx, input }) => {
      const session = await loadSessionForUser(ctx.session.user.id, input.sessionId)
      return busListenerTrees(ctx.db as never, session)
    }),

  /** EXP-1458: the tree a card's escalation would land in (its managing
   *  agent conversation's root): the device seals the card to its key
   *  (`sessionKeys.publicKeys`) and escalates with `sealedCard`. */
  escalateTarget: authedProcedure
    .input(z.object({ messageId: uuid }))
    .query(({ ctx, input }) => escalateTarget(ctx.session.user.id, input.messageId)),

  /** EXP-1458: the conversation a send to an agent / app lands in, with its
   *  key (`SendTarget`): the device seals the text to it (or sends `seal`
   *  for a keyless or new root) instead of sending it in plaintext. */
  sendTarget: authedProcedure
    .input(
      z
        .object({ agentId: uuid.optional(), appId: uuid.optional() })
        .refine((input) => !input.agentId !== !input.appId, `name agentId or appId`)
    )
    .query(({ ctx, input }) => sendTarget(ctx.session.user.id, input)),

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
              inSession(session.id),
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
