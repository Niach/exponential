import { TRPCError } from "@trpc/server"
import { and, asc, eq, isNotNull } from "drizzle-orm"
import {
  SUGGESTIONS_MAX,
  type MessageCard,
  type SuggestionItem,
} from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { boards, issues, messages, sessionIssues, sessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { boardVisible } from "@/lib/board-visibility"
import { assertCanSteer, loadSessionForUser } from "@/lib/sessions/access"
import { assertIssuesInTeam, linkIssues } from "@/lib/sessions/issues"
import { insertMessage } from "@/lib/sessions/messages"
import { sendSession } from "@/lib/sessions/send"
import type { Tx } from "@/lib/sessions/tx"
import {
  patchSuggestion,
  suggestionRequest,
  suggestionState,
} from "@/lib/sessions/suggestion-card"

// EXP-1328: the `suggestions` card. The agent posts it (MCP
// `exponential_sessions_suggest`, 1:1 with the card) on its OWN session,
// pre-consumed (it never wakes anything). A person acts per item: Start = a
// child of the card's session (`sessions.send`, the item's text as the
// request, its title on the child) · Do it here = a message to the card's
// session · File as issue · dismiss. Every stamp is a jsonb update under the
// message row's lock, in the same transaction as what it records, gated
// like answering a card (`assertCanSteer` on the card's session).

/** The card message + its session, steer-gated. */
async function loadSuggestionCard(userId: string, messageId: string) {
  const [message] = await db.select().from(messages).where(eq(messages.id, messageId)).limit(1)
  if (!message) throw new TRPCError({ code: `NOT_FOUND`, message: `Message not found` })
  const session = await loadSessionForUser(userId, message.sessionId)
  await assertCanSteer(userId, session)
  const card = message.card as MessageCard | null
  if (!card || card.kind !== `suggestions`) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `This message suggests nothing` })
  }
  return { message, session, card }
}

function openItem(card: Extract<MessageCard, { kind: `suggestions` }>, itemId: string) {
  const item = card.items.find((entry) => entry.id === itemId)
  if (!item) throw new TRPCError({ code: `NOT_FOUND`, message: `Suggestion not found` })
  if (suggestionState(item) !== `open`) {
    throw new TRPCError({ code: `CONFLICT`, message: `This suggestion was already handled` })
  }
  return item
}

/** Stamps an item under the message row's lock; a handled item = CONFLICT
 *  (rolls the caller's transaction back). */
async function stampItem(
  tx: Tx,
  messageId: string,
  itemId: string,
  patch: Partial<SuggestionItem>
): Promise<SuggestionItem> {
  const [row] = await tx
    .select({ card: messages.card })
    .from(messages)
    .where(eq(messages.id, messageId))
    .for(`update`)
  const patched = patchSuggestion((row?.card ?? null) as MessageCard | null, itemId, patch)
  if (!patched.ok) throw new TRPCError({ code: patched.code, message: patched.message })
  await tx
    .update(messages)
    .set({ card: patched.card, updatedAt: new Date() })
    .where(eq(messages.id, messageId))
  return patched.item
}

/** MCP `exponential_sessions_suggest`: the card on the CALLER's session. */
export async function postSuggestions(args: {
  userId: string
  callerSessionId: string
  tasks: Array<{ title: string; text: string; issueIds?: string[] }>
}): Promise<{ messageId: string; itemIds: string[] }> {
  if (args.tasks.length === 0 || args.tasks.length > SUGGESTIONS_MAX) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Suggest 1 to ${SUGGESTIONS_MAX} tasks`,
    })
  }
  const session = await loadSessionForUser(args.userId, args.callerSessionId)
  if (session.userId !== args.userId && session.hostUserId !== args.userId) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Suggest only on your own session` })
  }
  return db.transaction(async (tx) => {
    const items: SuggestionItem[] = []
    for (const task of args.tasks) {
      const issueIds = task.issueIds?.length
        ? (await assertIssuesInTeam(tx, session.teamId, task.issueIds)).map((row) => row.id)
        : null
      items.push({
        id: crypto.randomUUID(),
        title: task.title,
        text: task.text,
        ...(issueIds ? { issueIds } : {}),
      })
    }
    const messageId = await insertMessage(tx, session, {
      authorKind: `agent`,
      authorSessionId: session.id,
      card: { kind: `suggestions`, items },
      consumedAt: new Date(),
    })
    return { messageId, itemIds: items.map((item) => item.id) }
  })
}

export interface StartSuggestionInput {
  messageId: string
  itemId: string
  where: `child` | `here`
  deviceId?: string
  account?: string
  model?: string
}

/** Start = a child of the card's session; Do it here = a message to it.
 *  The stamp rides the send's transaction. */
export async function startSuggestion(
  userId: string,
  input: StartSuggestionInput
): Promise<{ sessionId: string; txid: number }> {
  const { message, card } = await loadSuggestionCard(userId, input.messageId)
  const item = openItem(card, input.itemId)
  const text = suggestionRequest(item)
  if (input.where === `here`) {
    const result = await sendSession({
      userId,
      input: { sessionId: message.sessionId, text },
      extras: {
        within: async (tx) => {
          await stampItem(tx, message.id, item.id, { here: true })
        },
      },
    })
    return { sessionId: result.sessionId, txid: result.txid }
  }
  const result = await sendSession({
    userId,
    input: {
      parentId: message.sessionId,
      text,
      ...(input.deviceId ? { deviceId: input.deviceId } : {}),
      ...(input.account ? { account: input.account } : {}),
      ...(input.model ? { model: input.model } : {}),
    },
    extras: {
      title: item.title,
      within: async (tx, started) => {
        await stampItem(tx, message.id, item.id, { sessionId: started.sessionId })
        if (item.issueIds?.length) {
          await linkIssues(tx, {
            rootId: started.rootId,
            issueIds: item.issueIds,
            source: `user`,
            addedBy: userId,
            addedBySessionId: null,
          })
        }
      },
    },
  })
  return { sessionId: result.sessionId, txid: result.txid }
}

/** Start all = the child branch for every open item, in card order. The
 *  tree caps refuse as-is (the items started before the refusal stay). */
export async function startAllSuggestions(
  userId: string,
  messageId: string
): Promise<{ sessionIds: string[]; txid: number | null }> {
  const { card } = await loadSuggestionCard(userId, messageId)
  const sessionIds: string[] = []
  let txid: number | null = null
  for (const item of card.items) {
    if (suggestionState(item) !== `open`) continue
    const started = await startSuggestion(userId, { messageId, itemId: item.id, where: `child` })
    sessionIds.push(started.sessionId)
    txid = started.txid
  }
  return { sessionIds, txid }
}

export async function dismissSuggestion(
  userId: string,
  input: { messageId: string; itemId: string }
): Promise<{ txid: number }> {
  await loadSuggestionCard(userId, input.messageId)
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    await stampItem(tx, input.messageId, input.itemId, { dismissed: true })
    return { txid }
  })
}

/** The board a thread files issues on: its first linked issue's board, else
 *  a visible board of the thread's repository. Null = none. */
export async function threadBoardId(rootId: string): Promise<string | null> {
  const [linked] = await db
    .select({ boardId: issues.boardId })
    .from(sessionIssues)
    .innerJoin(issues, eq(issues.id, sessionIssues.issueId))
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(and(eq(sessionIssues.sessionId, rootId), boardVisible()))
    .orderBy(asc(sessionIssues.sortOrder), asc(sessionIssues.createdAt))
    .limit(1)
  if (linked) return linked.boardId
  const [root] = await db
    .select({ teamId: sessions.teamId, repositoryId: sessions.repositoryId })
    .from(sessions)
    .where(eq(sessions.id, rootId))
    .limit(1)
  if (!root?.repositoryId) return null
  const [board] = await db
    .select({ id: boards.id })
    .from(boards)
    .where(
      and(
        eq(boards.teamId, root.teamId),
        eq(boards.repositoryId, root.repositoryId),
        isNotNull(boards.repositoryId),
        boardVisible()
      )
    )
    .orderBy(asc(boards.sortOrder), asc(boards.createdAt))
    .limit(1)
  return board?.id ?? null
}

/** File as issue: `issues.create` on the thread's board, linked to the root
 *  (its `sessionId`), then the item's `issueId`. `createIssue` = the issues
 *  router as the caller (its access + link rules). The create runs in its
 *  own transaction, so the item is LOCKED (FOR UPDATE) and re-checked open
 *  first and stamped under that same lock after: a concurrent Start/Dismiss
 *  waits on the lock and then sees the item filed (CONFLICT) instead of
 *  leaving an orphan issue behind a lost stamp. */
export async function fileSuggestion(
  userId: string,
  input: { messageId: string; itemId: string },
  createIssue: (args: {
    boardId: string
    title: string
    description: string
    sessionId: string
  }) => Promise<{ issueId: string }>
): Promise<{ issueId: string; txid: number }> {
  const { message, session, card } = await loadSuggestionCard(userId, input.messageId)
  const item = openItem(card, input.itemId)
  const boardId = await threadBoardId(session.rootId)
  if (!boardId) {
    throw new TRPCError({
      code: `PRECONDITION_FAILED`,
      message: `This thread has no board: link an issue first`,
    })
  }
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const [locked] = await tx
      .select({ card: messages.card })
      .from(messages)
      .where(eq(messages.id, message.id))
      .for(`update`)
    // The same open-state check the stamp makes, before anything is created.
    const open = patchSuggestion((locked?.card ?? null) as MessageCard | null, item.id, {})
    if (!open.ok) throw new TRPCError({ code: open.code, message: open.message })
    const { issueId } = await createIssue({
      boardId,
      title: item.title.slice(0, 500),
      description: item.text,
      sessionId: session.id,
    })
    await stampItem(tx, message.id, item.id, { issueId })
    return { issueId, txid }
  })
}
