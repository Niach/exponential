import { and, desc, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
import type { CodingSessionBlocked, MessageCard, MessageEventKind } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { messages, sessions, type Session } from "@/db/schema"
import { insertMessage } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"

// NEW-1: THE server bus. Every transition of a session S is posted into
// S's PARENT conversation and into every live session whose `wake_on`
// names S (a watcher: `sessions.send({wakeOn})`, or the starter of a root
// started from inside a session). A row that must wake its reader is
// authored BY S (`author_session_id` = S) with `consumed_at` NULL: that is
// exactly the pending input the devices already deliver ("Message from
// <S> (session <id>): <body>", `pendingInput()`), so nothing on the device
// changes for the bus to work. Informational lines are pre-consumed. A
// reader that has ended keeps the row for display, pre-consumed.
//
//   S's turn ended      → its reply, agent-authored        wakes
//   S needs input       → `ask` card + the question text   wakes
//   S hit a wall        → event `session_blocked`          wakes
//   S opened a PR       → event `pr_opened`                wakes
//   S ended / died      → `report` card (its summary)      wakes
//   S claimed           → event `session_claimed`          no
//   watchdog            → `session_unclaimed`/`session_stuck` wake,
//                         `device_offline`/`session_interrupted` no
//                         (EXP-1374: an interruption continues by itself;
//                         `session_interrupted` wakes once it did not)
//
// With nobody listening (a root without watchers) a line lands on S
// itself, pre-consumed, so a person still sees it.

export type BusSession = Pick<
  Session,
  `id` | `parentId` | `rootId` | `teamId` | `shared` | `userId` | `status`
>

interface Listener {
  id: string
  rootId: string
  teamId: string
  shared: boolean
  userId: string
  status: Session[`status`]
}

const LISTENER = {
  id: sessions.id,
  rootId: sessions.rootId,
  teamId: sessions.teamId,
  shared: sessions.shared,
  userId: sessions.userId,
  status: sessions.status,
}

/** The sessions a transition of `child` reaches: its parent (whatever its
 *  status) and every LIVE watcher, parent first, each once. */
export async function busListeners(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>
): Promise<Listener[]> {
  const watching = and(
    ne(sessions.status, `ended`),
    ne(sessions.id, child.id),
    sql`${sessions.wakeOn} @> ${JSON.stringify([child.id])}::jsonb`
  )!
  const rows = await tx
    .select(LISTENER)
    .from(sessions)
    .where(child.parentId ? or(eq(sessions.id, child.parentId), watching) : watching)
  const parent = rows.find((row) => row.id === child.parentId)
  const rest = rows
    .filter((row) => row.id !== child.parentId)
    .sort((a, b) => (a.id < b.id ? -1 : 1))
  return parent ? [parent, ...rest] : rest
}

async function deliver(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  message: {
    authorKind: `agent` | `system`
    body: string
    card: MessageCard | null
    waking: boolean
    /** With no listener: post on the child itself (pre-consumed). */
    fallbackToSelf?: boolean
  }
): Promise<string[]> {
  const listeners = await busListeners(tx, child)
  const now = new Date()
  const ids: string[] = []
  for (const listener of listeners) {
    ids.push(
      await insertMessage(tx, listener, {
        authorKind: message.authorKind,
        authorSessionId: child.id,
        body: message.body,
        card: message.card,
        consumedAt: message.waking && listener.status !== `ended` ? null : now,
      })
    )
  }
  if (listeners.length === 0 && message.fallbackToSelf) {
    ids.push(
      await insertMessage(tx, child.id, {
        authorKind: message.authorKind,
        body: message.body,
        card: message.card,
        consumedAt: now,
      })
    )
  }
  return ids
}

/** A turn of `child` ended with a reply: the reply reaches its listeners
 *  as the child's own message. Blank = nothing. */
export async function busRelayTurn(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  body: string
): Promise<string[]> {
  const text = body.trim()
  if (!text) return []
  return deliver(tx, child, { authorKind: `agent`, body: text, card: null, waking: true })
}

const ASK_PLAN_MAX = 2000

function clip(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1).trimEnd()}…` : text
}

/** The text a listener's agent reads for a child's open card: the question
 *  (or plan), its options and how to answer. */
export function askText(
  child: Pick<Session, `id`>,
  card: Extract<MessageCard, { kind: `question` | `plan` }>
): string {
  const send = `exponential_sessions_send({sessionId: "${child.id}", answer: {…}})`
  if (card.kind === `question`) {
    const options = (card.options ?? []).filter((option) => typeof option === `string`)
    const flags = [
      card.multi ? `several allowed` : null,
      card.custom !== false ? `free text allowed` : null,
    ].filter(Boolean)
    return [
      `Asks: ${card.question}`,
      options.length
        ? `Options: ${options.join(` | `)}${flags.length ? ` (${flags.join(`, `)})` : ``}`
        : null,
      `Answer with ${send} where answer = {answers: ["<option>"]} or {answer: "<text>"} if you know it; otherwise end your turn, a person answers in the card.`,
    ]
      .filter(Boolean)
      .join(`\n`)
  }
  const questions = (card.questions ?? []).map(
    (question) =>
      `- ${question.id}: ${question.prompt} (${question.options.map((option) => option.label).join(` | `)})`
  )
  return [
    `Proposes a plan:`,
    clip(card.plan, ASK_PLAN_MAX),
    questions.length ? `Questions:\n${questions.join(`\n`)}` : null,
    `Decide with ${send} where answer = {decision: "approved" | "rejected", answers: {"<question id>": ["<option>"]}, feedback: "<text>"} if it is yours to decide; otherwise end your turn, a person decides in the card.`,
  ]
    .filter(Boolean)
    .join(`\n\n`)
}

/** `child` posted a question/plan card (`messageId` = THE card row): its
 *  listeners get the `ask` mirror (one row, two views) and wake. */
export async function busAsk(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  messageId: string,
  card: Extract<MessageCard, { kind: `question` | `plan` }>
): Promise<string[]> {
  return deliver(tx, child, {
    authorKind: `agent`,
    body: askText(child, card),
    card: { kind: `ask`, sessionId: child.id, messageId },
    waking: true,
  })
}

const minutes = contract.sessionWatchdog

/** The bodies of the bus's event lines, as the listener's agent reads them
 *  (behind "Message from <child> (session <id>): "). */
export const BUS_COPY = {
  session_claimed: `Started working on its device.`,
  session_blocked: (blocked: CodingSessionBlocked) => {
    const window = blocked.window ? ` (${blocked.window} window` : ``
    const resets = blocked.resetsAt ? `${window ? `, ` : ` (`}resets ${blocked.resetsAt})` : window ? `)` : ``
    return `Hit a usage limit${window}${resets}. It stays blocked until its login rotates or the limit resets; nothing to do unless it should move or end.`
  },
  session_unclaimed: `Not picked up by its device for ${minutes.unclaimedMinutes} minutes (still queued). The device may be offline; a person sees it under Needs you.`,
  // EXP-1374 (FEED-141): how long, and a check, never "stop it": a silent
  // gate or spec run is the normal case and killing one costs real work.
  session_stuck: (silent: number) =>
    `No sign of life for ${silent} minutes while busy (no message, tool call, push or background task). Check whether it is still needed.`,
  device_offline: `Interrupted: its device went offline. It continues by itself when the device is back.`,
  session_interrupted: `Interrupted: its runner restarted mid-turn. It continues by itself.`,
  session_unresumed: `Interrupted ${minutes.resumeMinutes}+ minutes ago and not continued by its device. A person sees it under Needs you; a message continues it.`,
  pr_opened: (number: number, url: string) => `Opened PR #${number}: ${url}`,
} as const

/** One event line about `child` in its listeners' conversations (`waking`
 *  per the table above). With nobody listening it lands on the child. */
export async function busLine(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  event: Extract<
    MessageEventKind,
    | `session_claimed`
    | `session_blocked`
    | `session_unclaimed`
    | `session_stuck`
    | `device_offline`
    | `session_interrupted`
    | `pr_opened`
  >,
  body: string,
  opts: { waking: boolean; pullRequestId?: string | null; selfFallback?: boolean }
): Promise<string[]> {
  return deliver(tx, child, {
    authorKind: `system`,
    body,
    card: {
      kind: `event`,
      event,
      sessionId: child.id,
      ...(opts.pullRequestId ? { pullRequestId: opts.pullRequestId } : {}),
    },
    waking: opts.waking,
    fallbackToSelf: opts.selfFallback !== false,
  })
}

export const REPORT_FALLBACK = `Ended without a summary.`
const REPORT_MAX = 4000

/** The child's closing summary, else its last turn reply, else the fallback. */
export async function reportSummary(
  tx: Tx,
  child: Pick<Session, `id`>,
  summary?: string | null
): Promise<string> {
  let text = summary?.trim() ?? ``
  if (!text) {
    const [last] = await tx
      .select({ body: messages.body })
      .from(messages)
      .where(
        and(
          eq(messages.sessionId, child.id),
          eq(messages.authorKind, `agent`),
          isNull(messages.card),
          isNull(messages.authorSessionId)
        )
      )
      .orderBy(desc(messages.createdAt))
      .limit(1)
    text = last?.body.trim() ?? ``
  }
  return (text || REPORT_FALLBACK).slice(0, REPORT_MAX)
}

/** `child` ended: its `report` card reaches every listener and wakes it,
 *  then nobody watches it any more. */
export async function busReport(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  summary?: string | null
): Promise<string[]> {
  const text = await reportSummary(tx, child, summary)
  const ids = await deliver(tx, child, {
    authorKind: `agent`,
    body: text,
    card: { kind: `report`, sessionId: child.id, summary: text, final: true },
    waking: true,
  })
  await forgetWatched(tx, [child.id])
  return ids
}

/** The ended sessions leave every `wake_on`. */
export async function forgetWatched(tx: Tx, endedIds: readonly string[]): Promise<void> {
  for (const id of endedIds) {
    await tx
      .update(sessions)
      .set({ wakeOn: sql`${sessions.wakeOn} - ${id}::text` })
      .where(sql`${sessions.wakeOn} @> ${JSON.stringify([id])}::jsonb`)
  }
}

/** `watcher` watches `ids` (live sessions other than itself; idempotent). */
export async function watchSessions(
  tx: Tx,
  watcherId: string,
  ids: readonly string[]
): Promise<string[]> {
  const wanted = [...new Set(ids)].filter((id) => id !== watcherId)
  if (wanted.length === 0) return []
  const live = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(and(inArray(sessions.id, wanted), ne(sessions.status, `ended`)))
  const [row] = await tx
    .select({ wakeOn: sessions.wakeOn })
    .from(sessions)
    .where(eq(sessions.id, watcherId))
    .limit(1)
    .for(`update`)
  const next = [...new Set([...(row?.wakeOn ?? []), ...live.map((entry) => entry.id)])]
  await tx.update(sessions).set({ wakeOn: next }).where(eq(sessions.id, watcherId))
  return live.map((entry) => entry.id)
}
