import { and, desc, eq, inArray, isNull, ne, or, sql } from "drizzle-orm"
import { ASSISTANT_SILENT_EVENTS, assistantAgentIds } from "@/lib/agents/assistant-chat"
import type { CodingSessionBlocked, MessageCard, MessageEventKind } from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { sliceUnits } from "@exp/eagent/copy"
import { treeTeams } from "@/lib/sessions/tree-teams"
import {
  ASK_PLAN_LEAD,
  ASK_PRIVATE_PLAN_LEAD,
  ASK_PRIVATE_QUESTION_LEAD,
  ASK_QUESTION_LEAD,
  PRIVATE_PR_LEAD,
  PRIVATE_REPLY_LEAD,
  PRIVATE_REPORT_LEAD,
} from "@exp/domain-contract/chats"
import { agents, messages, sessions, teams, type Session } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import { insertMessage, inSession } from "@/lib/sessions/messages"
import { isSealedBody, isSealedCard } from "@/lib/sessions/sealed"
import type { Tx } from "@/lib/sessions/tx"

// NEW-1: THE server bus. Every transition of a session S is posted into
// S's PARENT conversation and into every live session whose `wake_on`
// names S (a watcher: `sessions.send({wakeOn})`, or the starter of a root
// started from inside a session, again once a message revives it:
// `rewatchRevived`, #FEED-206). A row that must wake its reader is
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
//   watchdog            → `session_unclaimed`/`session_stuck` wake;
//                         S-3: an interruption (device_offline, restarted)
//                         posts nothing (it continues by itself);
//                         `session_interrupted` wakes once it did not, or
//                         (#EXP-1675) once its device stayed away too long
//
// With nobody listening (a root without watchers) a line lands on S
// itself, pre-consumed, so a person still sees it.
//
// #EXP-1408: a listener whose readers may not see S (`askHidden`: S is
// private and the listener shared, or another member's) never gets S's
// TEXT: its reply, its report and its PR line become a NEUTRAL line naming
// the session id only (`privateText`), which its agent reads with its own
// auth; the ask mirror likewise (`askHiddenText`). Every other event body
// is the server's own copy (`BUS_COPY`) and carries nothing of S.
//
// EXP-1458 (sealed-content.json): a SEALED reply, ask or report is never
// read. A listener in S's own TREE gets a byte copy (one tree key); a
// listener in ANOTHER tree gets the copy the device sealed to that tree
// (`ListenerCopy`, `sessions.reply {listenerCopies}`), else the neutral
// line. `busListenerTargets` names those listeners so the device knows
// whom to seal for.

export type BusSession = Pick<
  Session,
  `id` | `parentId` | `rootId` | `teamId` | `shared` | `userId` | `status`
>

interface Listener {
  id: string
  /** S-3: a team agent's conversation (its root's agent). */
  agentId: string | null
  rootId: string
  teamId: string
  shared: boolean
  userId: string
  status: Session[`status`]
}

const LISTENER = {
  id: sessions.id,
  agentId: sessions.agentId,
  rootId: sessions.rootId,
  teamId: sessions.teamId,
  shared: sessions.shared,
  userId: sessions.userId,
  status: sessions.status,
}

/** The sessions a transition of `child` reaches: its parent (whatever its
 *  status) and every LIVE watcher, parent first, each once. #EXP-1731
 *  (eagent subagents.json): a CLI SUBAGENT's parent is the CLI process that
 *  ran it and has every line in-band: never a listener (no wake-up per
 *  subagent turn, not even its end report); its watchers still hear it. */
export async function busListeners(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>
): Promise<Listener[]> {
  const watching = and(
    ne(sessions.status, `ended`),
    ne(sessions.id, child.id),
    sql`${sessions.wakeOn} @> ${JSON.stringify([child.id])}::jsonb`
  )!
  const parentRow = child.parentId
    ? and(
        eq(sessions.id, child.parentId),
        sql`NOT EXISTS (SELECT 1 FROM sessions sub WHERE sub.id = ${child.id} AND sub.subagent_key IS NOT NULL)`
      )!
    : null
  const rows = await tx
    .select(LISTENER)
    .from(sessions)
    .where(parentRow ? or(parentRow, watching) : watching)
  const parent = rows.find((row) => row.id === child.parentId)
  const rest = rows
    .filter((row) => row.id !== child.parentId)
    .sort((a, b) => (a.id < b.id ? -1 : 1))
  return parent ? [parent, ...rest] : rest
}

/** #EXP-1735 (eagent bus.json `leadOf`): the team LEAD Assistant `row`
 *  reports to, or null. `row` works in a team whose lead
 *  (`teams.lead_assistant_id`, a member's Assistant) is another member's
 *  Assistant than the one acting for its tree (the Assistant's children and
 *  the root sessions it started); never that Assistant's own conversation root,
 *  never a team agent's tree, never the lead's own work, never a lead whose
 *  member left the team. */
export async function leadOf(
  tx: Pick<Tx, `select`>,
  row: Pick<Session, `id` | `rootId` | `teamId`>
): Promise<{ id: string; ownerId: string } | null> {
  const [team] = await tx
    .select({ leadId: teams.leadAssistantId })
    .from(teams)
    .where(eq(teams.id, row.teamId))
    .limit(1)
  if (!team?.leadId) return null
  const [root] = await tx
    .select({ id: sessions.id, agentId: sessions.agentId, startedByAgentId: sessions.startedByAgentId })
    .from(sessions)
    .where(eq(sessions.id, row.rootId))
    .limit(1)
  const acting = root ? (root.agentId ?? root.startedByAgentId) : null
  if (!root || !acting || acting === team.leadId) return null
  if (row.id === root.id && root.agentId === acting) return null
  const roles = await tx
    .select({ id: agents.id, role: agents.role, ownerId: agents.ownerId })
    .from(agents)
    .where(inArray(agents.id, [acting, team.leadId]))
  const lead = roles.find((agent) => agent.id === team.leadId)
  if (lead?.role !== `assistant` || roles.find((agent) => agent.id === acting)?.role !== `assistant`) return null
  if (!(await getTeamMember(lead.ownerId, row.teamId))) return null
  return { id: lead.id, ownerId: lead.ownerId }
}

/** #EXP-1735: the lead's conversation (its member's root session with it)
 *  an ask or a report of `child` also reaches, or null (none yet: the pick
 *  opens it, `agents.setLeadAssistant`). */
async function leadListener(tx: Tx, child: Pick<Session, `id`>): Promise<Listener | null> {
  const [row] = await tx
    .select({ id: sessions.id, rootId: sessions.rootId, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.id, child.id))
    .limit(1)
  const lead = row ? await leadOf(tx, row) : null
  if (!lead) return null
  const [conversation] = await tx
    .select(LISTENER)
    .from(sessions)
    .where(and(eq(sessions.agentId, lead.id), eq(sessions.userId, lead.ownerId), isNull(sessions.parentId)))
    .limit(1)
  return conversation && conversation.status !== `ended` && conversation.id !== child.id ? conversation : null
}

type Line = { body: string; card: MessageCard | null }

/** EXP-1458: a sealed line's copy for the listeners in ANOTHER tree, sealed
 *  by the device to that tree's key (one per tree: `rootId`). A body copy
 *  (`body`) or, for a report, the card envelope (`card`). */
export interface ListenerCopy {
  rootId: string
  body?: string
  card?: MessageCard
}

/** EXP-1458: the TREES a sealed line of `child` reaches outside its own
 *  (listeners that may see it), with their current key: the device seals
 *  a `ListenerCopy` for each (`sessions.listeners`). A keyless tree has
 *  `epoch`/`publicKey` null (its listeners get the neutral line). */
export async function busListenerTrees(
  tx: Tx,
  child: Pick<Session, `id` | `parentId` | `rootId`>
): Promise<Array<{ rootId: string; epoch: number | null; publicKey: string | null }>> {
  const listeners = await busListeners(tx, child)
  if (listeners.length === 0) return []
  const origin = await busOrigin(tx, child.id)
  const rootIds = [
    ...new Set(
      listeners
        .filter((listener) => listener.rootId !== child.rootId && origin && !askHidden(listener, origin))
        .map((listener) => listener.rootId)
    ),
  ].sort()
  if (rootIds.length === 0) return []
  const roots = await tx
    .select({ id: sessions.id, epoch: sessions.sealEpoch, publicKey: sessions.sealPublicKey })
    .from(sessions)
    .where(inArray(sessions.id, rootIds))
  const byId = new Map(roots.map((root) => [root.id, root]))
  return rootIds.map((rootId) => ({
    rootId,
    epoch: byId.get(rootId)?.epoch ?? null,
    publicKey: byId.get(rootId)?.publicKey ?? null,
  }))
}

async function deliver(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  message: {
    authorKind: `agent` | `system`
    body: string
    card: MessageCard | null
    /** #EXP-1399 review / #EXP-1408: the line a listener whose readers may
     *  not see the child gets instead (`askHidden`); absent = the line
     *  carries nothing of the child. */
    hidden?: Line
    waking: boolean
    /** With no listener: post on the child itself (pre-consumed). */
    fallbackToSelf?: boolean
    /** S-3: the child's own row this line copies (an edit patches it). */
    originMessageId?: string | null
    /** S-3: listeners this line skips (still counted as listening). */
    skip?: (listener: Listener) => boolean
    /** EXP-1458: the line is sealed to the child's tree: listeners in it
     *  get `inTree` (default: the line itself, a byte copy), listeners in
     *  another tree their `copies` entry, else `hidden`. */
    sealed?: { inTree?: Line; copies?: readonly ListenerCopy[] }
    /** #EXP-1735: the team lead's conversation hears it too (after the
     *  others, once; a sealed line reaches it as `hidden`: no copy is
     *  sealed for it). */
    lead?: boolean
  }
): Promise<string[]> {
  const all = await busListeners(tx, child)
  const lead = message.lead ? await leadListener(tx, child) : null
  if (lead && !all.some((listener) => listener.id === lead.id)) all.push(lead)
  const listeners = message.skip ? all.filter((listener) => !message.skip!(listener)) : all
  const origin =
    (message.hidden || message.sealed) && listeners.length > 0 ? await busOrigin(tx, child.id) : null
  const copies = new Map((message.sealed?.copies ?? []).map((copy) => [copy.rootId, copy]))
  const now = new Date()
  const ids: string[] = []
  for (const listener of listeners) {
    let line: Line = message.hidden && (!origin || askHidden(listener, origin)) ? message.hidden : message
    if (message.sealed && line !== message.hidden) {
      const copy = copies.get(listener.rootId)
      line =
        origin && listener.rootId === origin.rootId
          ? (message.sealed.inTree ?? message)
          : copy !== undefined
            ? { body: copy.body ?? message.body, card: copy.card ?? message.card }
            : (message.hidden ?? { body: ``, card: message.card })
    }
    ids.push(
      await insertMessage(tx, listener, {
        authorKind: message.authorKind,
        authorSessionId: child.id,
        body: line.body,
        card: line.card,
        consumedAt: message.waking && listener.status !== `ended` ? null : now,
        originMessageId: message.originMessageId ?? null,
      })
    )
  }
  if (all.length === 0 && message.fallbackToSelf) {
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

/** Who may read `child` (the visibility `askHidden` compares). Gone =
 *  null: every listener gets the hidden line. */
async function busOrigin(tx: Tx, childId: string): Promise<Pick<Session, `shared` | `userId` | `rootId`> | null> {
  const [row] = await tx
    .select({ shared: sessions.shared, userId: sessions.userId, rootId: sessions.rootId })
    .from(sessions)
    .where(eq(sessions.id, childId))
    .limit(1)
  return row ?? null
}

/** #EXP-1408: the NEUTRAL line for a private chat's reply / report / PR
 *  (`askHidden`): the lead and the session id only; the agent reads the
 *  text itself if its account may see it. */
export function privateText(child: Pick<Session, `id`>, lead: string): string {
  return `${lead}\nRead it with exponential_sessions({op: "get", id: "${child.id}"}) if your account may see it.`
}

/** A turn of `child` ended with a reply: the reply reaches its listeners
 *  as the child's own message. Blank = nothing. S-3: a team agent's
 *  conversation that WATCHES the child (not its parent) is not woken per
 *  turn: it hears the child's ask, wall, PR and report only, so a busy
 *  chat never spams its agent. #FEED-194: the agent that STARTED the
 *  chat does hear each turn: it asked for the work and waits on it. */
export async function busRelayTurn(
  tx: Tx,
  child: Pick<Session, `id` | `parentId` | `startedByAgentId`>,
  body: string,
  originMessageId: string | null = null,
  /** EXP-1458: a sealed body's copies for listeners in other trees. */
  copies?: readonly ListenerCopy[]
): Promise<string[]> {
  const text = body.trim()
  if (!text) return []
  return deliver(tx, child, {
    ...(isSealedBody(text) ? { sealed: { copies } } : {}),
    authorKind: `agent`,
    body: text,
    card: null,
    hidden: { body: privateText(child, PRIVATE_REPLY_LEAD), card: null },
    waking: true,
    originMessageId,
    skip: (listener) =>
      listener.agentId !== null && listener.id !== child.parentId && listener.agentId !== child.startedByAgentId,
  })
}

const ASK_PLAN_MAX = 2000

function clip(text: string, max: number): string {
  return text.length > max ? `${sliceUnits(text, max - 1).trimEnd()}…` : text
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
      `${ASK_QUESTION_LEAD}${card.question}`,
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
    ASK_PLAN_LEAD,
    clip(card.plan, ASK_PLAN_MAX),
    questions.length ? `Questions:\n${questions.join(`\n`)}` : null,
    `Decide with ${send} where answer = {decision: "approved" | "rejected", answers: {"<question id>": ["<option>"]}, feedback: "<text>"} if it is yours to decide; otherwise end your turn, a person decides in the card.`,
  ]
    .filter(Boolean)
    .join(`\n\n`)
}

/** #EXP-1399 review: a reader of `listener` may not see `origin` (a
 *  private session mirrored into a shared conversation, or into another
 *  member's): its ask mirror then carries no card text. */
export function askHidden(
  listener: Pick<Session, `shared` | `userId`>,
  origin: Pick<Session, `shared` | `userId`>
): boolean {
  return !origin.shared && (listener.shared || listener.userId !== origin.userId)
}

/** The NEUTRAL ask text (`askHidden`): the kind and the session id only;
 *  the agent reads the card itself if its account may see it. */
export function askHiddenText(
  child: Pick<Session, `id`>,
  card: Extract<MessageCard, { kind: `question` | `plan` }>
): string {
  return [
    card.kind === `plan` ? ASK_PRIVATE_PLAN_LEAD : ASK_PRIVATE_QUESTION_LEAD,
    `Read it with exponential_sessions({op: "get", id: "${child.id}"}) and answer with exponential_sessions_send({sessionId: "${child.id}", answer: {…}}) if you may see it; otherwise end your turn, a person answers in the card.`,
  ].join(`\n`)
}

/** `child` posted a question/plan card (`messageId` = THE card row): its
 *  listeners get the `ask` mirror (one row, two views) and wake. */
export async function busAsk(
  tx: Tx,
  child: Pick<Session, `id` | `parentId` | `shared` | `userId`>,
  messageId: string,
  card: Extract<MessageCard, { kind: `question` | `plan` }>,
  /** EXP-1458: a SEALED card's ask text, sealed by the device: `askText`
   *  to its own tree, `listenerCopies` to the other trees listening. */
  sealed?: { askText?: string | null; listenerCopies?: readonly ListenerCopy[] }
): Promise<string[]> {
  const mirror: MessageCard = { kind: `ask`, sessionId: child.id, messageId }
  if (isSealedCard(card)) {
    const neutral = askHiddenText(child, card)
    return deliver(tx, child, {
      authorKind: `agent`,
      body: neutral,
      card: mirror,
      hidden: { body: neutral, card: mirror },
      sealed: {
        inTree: { body: sealed?.askText?.trim() || neutral, card: mirror },
        copies: sealed?.listenerCopies,
      },
      waking: true,
      lead: true,
    })
  }
  return deliver(tx, child, {
    authorKind: `agent`,
    body: askText(child, card),
    card: mirror,
    hidden: { body: askHiddenText(child, card), card: mirror },
    waking: true,
    lead: true,
  })
}

/** #EXP-1399: `child`'s card(s) `messageIds` were answered (or superseded):
 *  every `ask` mirror of them closes too (`answered_at`), so a list or a
 *  Dock reading the mirror alone stops waiting on it. */
export async function closeAskMirrors(
  tx: Tx,
  childId: string,
  messageIds: readonly string[],
  at: Date = new Date()
): Promise<void> {
  if (messageIds.length === 0) return
  await tx
    .update(messages)
    .set({ answeredAt: at, updatedAt: at })
    .where(
      and(
        eq(messages.authorSessionId, childId),
        isNull(messages.answeredAt),
        sql`${messages.card}->>'kind' = 'ask'`,
        inArray(sql`${messages.card}->>'messageId'`, [...messageIds])
      )
    )
}

/** #EXP-1399 review: `sessionIds` ended (or are about to be deleted):
 *  their still-open `ask` mirrors close, so no list waits on a card nobody
 *  can answer any more. */
export async function closeAskMirrorsOf(
  tx: Pick<Tx, `update`>,
  sessionIds: readonly string[],
  at: Date = new Date()
): Promise<void> {
  if (sessionIds.length === 0) return
  await tx
    .update(messages)
    .set({ answeredAt: at, updatedAt: at })
    .where(
      and(
        isNull(messages.answeredAt),
        sql`${messages.card}->>'kind' = 'ask'`,
        inArray(sql`${messages.card}->>'sessionId'`, [...sessionIds])
      )
    )
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
  // #FEED-235: a claimed session whose ONLINE device ran no turn on its input.
  session_undelivered: `Not picked up by its device for ${minutes.undeliveredMinutes} minutes (no turn started on its input). The device may need a restart; a person sees it under Needs you.`,
  // EXP-1374 (FEED-141): how long, and a check, never "stop it": a silent
  // gate or spec run is the normal case and killing one costs real work.
  session_stuck: (silent: number) =>
    `No sign of life for ${silent} minutes while busy (no message, tool call, push or background task). Check whether it is still needed.`,
  session_unresumed: `Interrupted ${minutes.resumeMinutes}+ minutes ago and not continued by its device. A person sees it under Needs you; a message continues it.`,
  session_offline: `Interrupted: its device has been offline for ${minutes.offlineMinutes}+ minutes. A person sees it under Needs you; it continues when the device is back.`,
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
    | `session_interrupted`
    | `pr_opened`
  >,
  body: string,
  opts: { waking: boolean; pullRequestId?: string | null; selfFallback?: boolean }
): Promise<string[]> {
  const card: MessageCard = { kind: `event`, event, sessionId: child.id }
  // #EXP-1439: routine lines never land in a member's Assistant
  // conversation (`assistant.chat.silentEvents`); its check-in reads them.
  const silentFor = ASSISTANT_SILENT_EVENTS.has(event)
    ? await assistantAgentIds(
        tx,
        (await busListeners(tx, child)).flatMap((listener) => (listener.agentId ? [listener.agentId] : []))
      )
    : null
  return deliver(tx, child, {
    authorKind: `system`,
    body,
    card: opts.pullRequestId ? { ...card, pullRequestId: opts.pullRequestId } : card,
    // #EXP-1408: a PR line names the PR; hidden, it names the session only
    // (no pullRequestId: the client draws no `Opened PR` line for it).
    ...(event === `pr_opened` ? { hidden: { body: privateText(child, PRIVATE_PR_LEAD), card } } : {}),
    waking: opts.waking,
    fallbackToSelf: opts.selfFallback !== false,
    ...(silentFor?.size
      ? { skip: (listener: Listener) => listener.agentId !== null && listener.id === listener.rootId && silentFor.has(listener.agentId) }
      : {}),
  })
}

export const REPORT_FALLBACK = `Ended without a summary.`
const REPORT_MAX = 4000

/** The child's closing summary, else its last turn reply, else the
 *  fallback. EXP-1458: a sealed summary or last reply is never read (the
 *  fallback instead: a sealed tree reports with `sealedReport`). */
export async function reportSummary(
  tx: Tx,
  child: Pick<Session, `id`>,
  summary?: string | null
): Promise<string> {
  let text = summary?.trim() ?? ``
  // A sealed summary belongs in a sealed report card (`sealedReport`).
  if (isSealedBody(text)) text = ``
  if (!text) {
    const [last] = await tx
      .select({ body: messages.body })
      .from(messages)
      .where(
        and(
          inSession(child.id),
          eq(messages.authorKind, `agent`),
          isNull(messages.card),
          isNull(messages.authorSessionId)
        )
      )
      .orderBy(desc(messages.createdAt))
      .limit(1)
    text = last?.body.trim() ?? ``
    if (isSealedBody(text)) text = ``
  }
  return sliceUnits(text || REPORT_FALLBACK, REPORT_MAX)
}

/** `child` ended: its `report` card reaches every listener and wakes it,
 *  then nobody watches it any more. */
export async function busReport(
  tx: Tx,
  child: Pick<Session, `id` | `parentId`>,
  summary?: string | null,
  /** EXP-1458: the device-built report card sealed to the child's tree
   *  (`{kind: report, sessionId, status, final, summary}`) and its copies
   *  sealed to the other listening trees (`copies[].card`). */
  sealedReport?: { card: MessageCard; copies?: readonly ListenerCopy[] } | null
): Promise<string[]> {
  if (sealedReport) {
    // Never read: the card is copied whole, the body stays empty.
    const card = { ...sealedReport.card, sessionId: child.id, final: true } as MessageCard
    const ids = await deliver(tx, child, {
      authorKind: `agent`,
      body: ``,
      card,
      hidden: {
        body: privateText(child, PRIVATE_REPORT_LEAD),
        card: { kind: `report`, sessionId: child.id, summary: PRIVATE_REPORT_LEAD, final: true },
      },
      sealed: { copies: sealedReport.copies?.map((copy) => ({ ...copy, card: copy.card && ({ ...copy.card, sessionId: child.id, final: true } as MessageCard) })) },
      waking: true,
      lead: true,
    })
    await forgetWatched(tx, [child.id])
    return ids
  }
  const text = await reportSummary(tx, child, summary)
  const ids = await deliver(tx, child, {
    authorKind: `agent`,
    body: text,
    card: { kind: `report`, sessionId: child.id, summary: text, final: true },
    hidden: {
      body: privateText(child, PRIVATE_REPORT_LEAD),
      card: { kind: `report`, sessionId: child.id, summary: PRIVATE_REPORT_LEAD, final: true },
    },
    waking: true,
    lead: true,
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

/** `watcher` watches `ids` (live sessions other than itself; idempotent).
 *  #EXP-1399 review: only sessions of its team its own account may see
 *  (shared, or the watcher's user started them); others drop silently. */
export async function watchSessions(
  tx: Tx,
  watcherId: string,
  ids: readonly string[]
): Promise<string[]> {
  const wanted = [...new Set(ids)].filter((id) => id !== watcherId)
  if (wanted.length === 0) return []
  const [row] = await tx
    .select({ wakeOn: sessions.wakeOn, rootId: sessions.rootId, userId: sessions.userId })
    .from(sessions)
    .where(eq(sessions.id, watcherId))
    .limit(1)
    .for(`update`)
  if (!row) return []
  // #EXP-1720: any team of the watcher's tree (the Assistant's: its scope).
  const teams = await treeTeams(row.rootId, tx)
  if (teams.length === 0) return []
  const live = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        inArray(sessions.id, wanted),
        ne(sessions.status, `ended`),
        inArray(sessions.teamId, teams),
        or(eq(sessions.shared, true), eq(sessions.userId, row.userId))
      )
    )
  const next = [...new Set([...(row.wakeOn ?? []), ...live.map((entry) => entry.id)])]
  await tx.update(sessions).set({ wakeOn: next }).where(eq(sessions.id, watcherId))
  return live.map((entry) => entry.id)
}

/** #FEED-206: a revived chat root (`reviveRoot`) is WATCHED again, as at
 *  its start: its report had dropped every watch (`forgetWatched`), so
 *  without this its new turns and its next report reach nobody. Its
 *  STARTER (the live session carrying its `thread_started` card: the team
 *  agent's conversation that asked for the work, whoever resumes it) and
 *  the calling session (a session outside its tree) watch it. The starter
 *  is looked up among the root's agent's conversations and the sessions
 *  that wrote into the root, so the card check stays on indexed
 *  conversations. */
export async function rewatchRevived(
  tx: Tx,
  root: Pick<Session, `id` | `startedByAgentId`>,
  caller: Pick<Session, `id` | `rootId`> | null
): Promise<void> {
  const card = JSON.stringify({ kind: `thread_started`, sessionId: root.id })
  const starters = await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(
      and(
        ne(sessions.status, `ended`),
        ne(sessions.rootId, root.id),
        or(
          root.startedByAgentId ? eq(sessions.agentId, root.startedByAgentId) : undefined,
          inArray(
            sessions.id,
            tx
              .select({ id: sql<string>`${messages.authorSessionId}` })
              .from(messages)
              .where(and(inSession(root.id), sql`${messages.authorSessionId} IS NOT NULL`))
          )
        ),
        sql`EXISTS (SELECT 1 FROM ${messages} WHERE ${inSession(sessions.id)} AND ${messages.card} @> ${card}::jsonb)`
      )
    )
  const watchers = new Set(starters.map((row) => row.id))
  if (caller && caller.rootId !== root.id) watchers.add(caller.id)
  for (const watcherId of [...watchers].sort()) await watchSessions(tx, watcherId, [root.id])
}
