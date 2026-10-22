import { isPersistent } from "@exp/domain-contract/session-kind"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, ne, sql } from "drizzle-orm"
import {
  SESSION_TREE_MAX_DEPTH,
  SESSION_TREE_MAX_SESSIONS,
  type SessionsSendInput,
  type SessionsSendResult,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import {
  attachments,
  boards,
  devices,
  issues,
  mcpServers,
  prompts,
  repositories,
  sessions,
  type Session,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertTeamMember, getTeamMember } from "@/lib/auth/membership"
import { conversationInTx, loadAgentForUser } from "@/lib/agents/conversation"
import {
  briefedText,
  createEditorSessionInTx,
  defaultDeviceFor,
  findEditorSession,
  loadAppForUser,
} from "@/lib/apps/editor-session"
import { answerSessionCardInTx } from "@/lib/sessions/answer"
import {
  assertCanSteer,
  isSessionPrincipal,
  loadSessionForUser,
  sessionDeviceOwner,
} from "@/lib/sessions/access"
import { watchSessions } from "@/lib/sessions/bus"
import {
  assertIssuesInTeam,
  forwardIssues,
  linkIssues,
  retakeHeldIssues,
  takeOverIssues,
} from "@/lib/sessions/issues"
import { relayKillSessionsBestEffort } from "@/lib/sessions/lifecycle"
import {
  postMovedMessage,
  postPersonMessage,
  postSessionInput,
  postSystemCard,
} from "@/lib/sessions/messages"
import {
  assertMovable,
  deviceName,
  loadDeviceRow,
  movedText,
  resolveMoveTarget,
  type MoveTarget,
} from "@/lib/sessions/move"
import type { Tx } from "@/lib/sessions/tx"
import { buildAttachmentUrl, isAcceptedImageContentType } from "@/lib/storage/issue-attachments"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"
import { fireAndForgetSessionMessageNotify } from "@/lib/integrations/notifications"

// EXP-1291/NEW-1/NEW-2: `sessions.send` — THE entrypoint (tRPC
// `sessions.send` and MCP `exponential_sessions_send` both land here). Four
// forms, one shape:
//   - `sessionId`: a person's (or a session's) message into a session (the
//     DB is the bus: a `messages` row the device delivers, resuming an
//     idle session in place). `answer` answers its open card; `issueIds`
//     forward issues to it; `deviceId` moves it (a queued row re-targets,
//     a live one ends here and continues there). An ENDED session continues
//     in a NEW session carrying the message: a root as a child of itself,
//     a child as its sibling, on the same branch.
//   - `agentId`: a message to a team AGENT = into the caller's own
//     conversation session with it (one per agent and member, created on
//     first use on the agent's device, never ending).
//   - `parentId`: a child (sub-session) of any visible session, depth + 1,
//     inheriting its parent's runner picks.
//   - neither: a new ROOT — a thread that WORKS (the composer's picks, a
//     worktree when a repo is set; `promptId` = a saved prompt's body plus
//     the text as its first message, titled after the prompt). Started from
//     inside a session, the caller WATCHES it (the bus delivers its
//     transitions there).
// Every new session is a `queued` row naming its device; the device picks
// it up off the sessions shape (a best-effort relay nudge kills the latency).

type SessionInsert = typeof sessions.$inferInsert

/** The calling session (MCP `X-Exp-Session-Id`), when the caller may act as
 *  it: its starter or the host of its device. Anything else is ignored,
 *  never trusted. */
async function loadCallerSession(userId: string, callerSessionId?: string | null) {
  if (!callerSessionId) return null
  const [row] = await db
    .select()
    .from(sessions)
    .where(eq(sessions.id, callerSessionId))
    .limit(1)
  if (!row || !(await isSessionPrincipal(userId, row))) return null
  return row
}

/**
 * The machine a new session runs on: the caller's own device, or one its
 * owner shared with the team (EXP-432). `shared` = that devices row shares
 * the team — the session's visibility for life. `ownerId` = the account
 * that runs it (the host).
 */
export async function resolveSessionDevice(
  userId: string,
  teamId: string,
  deviceId: string
): Promise<{ shared: boolean; ownerId: string }> {
  const rows = await db
    .select({ userId: devices.userId, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, deviceId))
  const own = rows.find((row) => row.userId === userId)
  if (own) return { shared: (own.sharedTeamIds ?? []).includes(teamId), ownerId: userId }
  for (const row of rows.sort((a, b) => (a.userId < b.userId ? -1 : 1))) {
    if (!(row.sharedTeamIds ?? []).includes(teamId)) continue
    // Sharing is the owner's consent while they are still a member.
    if (!(await getTeamMember(row.userId, teamId))) continue
    return { shared: true, ownerId: row.userId }
  }
  // NOT_FOUND, before anything else is looked at: a machine the caller may
  // not see is never described (not its version, not whether it is online).
  throw new TRPCError({
    code: `NOT_FOUND`,
    message: `This device is not yours and not shared with the team`,
  })
}

async function assertRepositoryInTeam(tx: Tx, teamId: string, repositoryId: string) {
  const [row] = await tx
    .select({ id: repositories.id })
    .from(repositories)
    .where(and(eq(repositories.id, repositoryId), eq(repositories.teamId, teamId)))
    .limit(1)
  if (!row) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Repository not found in this team` })
  }
}

async function assertMcpServersInTeam(tx: Tx, teamId: string, ids: string[] | undefined) {
  const unique = [...new Set(ids ?? [])]
  if (unique.length === 0) return null
  const rows = await tx
    .select({ id: mcpServers.id })
    .from(mcpServers)
    .where(and(eq(mcpServers.teamId, teamId), inArray(mcpServers.id, unique)))
  if (rows.length !== unique.length) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Unknown MCP server for this team` })
  }
  return unique
}

/**
 * Claims eager uploads (team-scoped `attachments` rows with no owner yet,
 * uploaded by the sender) onto the message, and appends an embed line for
 * every one the text does not already reference: attachments never sync, so
 * the body is how clients and the device find them.
 */
async function claimAttachments(
  tx: Tx,
  args: {
    ids: string[]
    messageId: string
    teamId: string
    uploaderIds: string[]
    body: string
  }
): Promise<void> {
  const ids = [...new Set(args.ids)]
  if (ids.length === 0) return
  const claimed = await tx
    .update(attachments)
    .set({ messageId: args.messageId })
    .where(
      and(
        inArray(attachments.id, ids),
        eq(attachments.teamId, args.teamId),
        isNull(attachments.issueId),
        isNull(attachments.draftId),
        isNull(attachments.messageId),
        inArray(attachments.uploaderId, args.uploaderIds)
      )
    )
    .returning({
      id: attachments.id,
      filename: attachments.filename,
      contentType: attachments.contentType,
    })
  if (claimed.length !== ids.length) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Attach files you uploaded to this team (each file once)`,
    })
  }
  const byId = new Map(claimed.map((row) => [row.id, row]))
  const lines = ids
    .filter((id) => !args.body.includes(buildAttachmentUrl(id)))
    .map((id) => {
      const row = byId.get(id)!
      const url = buildAttachmentUrl(id)
      const name = row.filename.replace(/[[\]]/g, ``)
      return isAcceptedImageContentType(row.contentType) ? `![${name}](${url})` : `[${name}](${url})`
    })
  if (lines.length === 0) return
  const body = [args.body.trimEnd(), ...lines].filter(Boolean).join(`\n\n`)
  await tx.execute(sql`UPDATE messages SET body = ${body} WHERE id = ${args.messageId}`)
}

/** The person message of a send (text and/or attachments), or null.
 *  EXP-1323: `from` = the calling session (its MCP key): stamped as
 *  `author_session_id`; `asSession` = the message is that SESSION's input
 *  into another live one (a child asking its parent), agent-authored. */
async function postSendMessage(
  tx: Tx,
  session: Pick<Session, `id` | `rootId` | `teamId` | `shared` | `userId`>,
  input: SessionsSendInput,
  authorId: string,
  uploaderIds: string[],
  from?: { sessionId: string; asSession: boolean } | null
): Promise<string | null> {
  const text = input.text ?? ``
  if (!text.trim() && !input.attachmentIds?.length) return null
  const messageId = from?.asSession
    ? await postSessionInput(tx, session, from.sessionId, text)
    : await postPersonMessage(tx, session, authorId, text, from?.sessionId ?? null)
  await claimAttachments(tx, {
    ids: input.attachmentIds ?? [],
    messageId,
    teamId: session.teamId,
    uploaderIds,
    body: text,
  })
  return messageId
}

/** Who a new session belongs to: the real person. A hosted run's agent acts
 *  with its HOST's key (EXP-432), so its spawns stay the requester's. */
function effectiveStarter(userId: string, caller: Session | null): string {
  return caller && caller.userId !== userId ? caller.userId : userId
}

// The composer's value sets (EXP-149/201): an agent CLI from the contract,
// and a model/effort from THAT agent's lists. Models are always explicit
// (contract `codingModel`, no blank); a stale client's `""` (its old "CLI
// default") is tolerated and stored as NULL, never as a value.
const MODELS: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: contract.codexModel.values,
}
const EFFORTS: Record<string, readonly string[]> = {
  claude: contract.codingEffort.values,
  codex: contract.codexEffort.values,
}

/** EXP-1334: whether `model` is one of `cli`'s models (NULL = claude). */
export function isRunnerModel(cli: string | null | undefined, model: string): boolean {
  return MODELS[cli ?? `claude`]?.includes(model) ?? false
}

/** Refuses a runner pick outside the contract vocabulary. */
export function assertRunnerVocabulary(
  input: Pick<SessionsSendInput, `cli` | `model` | `effort` | `ultracode`>
) {
  const cli = input.cli ?? `claude`
  const bad = (message: string) => new TRPCError({ code: `BAD_REQUEST`, message })
  if (!(contract.codingAgent.values as readonly string[]).includes(cli)) {
    throw bad(`Unknown agent "${cli}"`)
  }
  if (input.model && !MODELS[cli]!.includes(input.model)) {
    throw bad(`Unknown ${cli} model "${input.model}"`)
  }
  if (input.effort && !EFFORTS[cli]!.includes(input.effort)) {
    throw bad(`Unknown ${cli} effort "${input.effort}"`)
  }
  if (cli !== `claude` && input.ultracode) {
    throw bad(`Ultracode runs on claude only`)
  }
}

type RunnerSource = Pick<
  Session,
  `cli` | `account` | `model` | `effort` | `planMode` | `computerUse` | `ultracode`
>

/** The runner pickers of a new session: the input's, else inherited from
 *  `from` (a child from its parent, a continuation from the ended row). A
 *  blank model/effort (`""`, an old client's "CLI default") is NULL: it is
 *  neither stored nor inherited. */
function runnerFields(input: Partial<SessionsSendInput>, from?: RunnerSource | null) {
  return {
    cli: input.cli ?? from?.cli ?? null,
    account: input.account ?? from?.account ?? null,
    model: (input.model || from?.model) || null,
    effort: (input.effort || from?.effort) || null,
    planMode: input.planMode ?? from?.planMode ?? false,
    computerUse: input.computerUse ?? from?.computerUse ?? false,
    ultracode: input.ultracode ?? from?.ultracode ?? false,
  }
}

/** Refuses a child the follow-up caps forbid: depth below the root, and
 *  the tree's LIVE sessions (an ended branch frees its slot). Holds the
 *  root's row lock so two concurrent starts cannot both take the last slot. */
async function assertTreeRoom(tx: Tx, rootId: string, depth: number) {
  if (depth > SESSION_TREE_MAX_DEPTH) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Sub-sessions nest at most ${SESSION_TREE_MAX_DEPTH} deep`,
    })
  }
  await tx.select({ id: sessions.id }).from(sessions).where(eq(sessions.id, rootId)).for(`update`)
  const [{ live } = { live: 0 }] = await tx
    .select({ live: sql<number>`count(*)::int` })
    .from(sessions)
    .where(and(eq(sessions.rootId, rootId), ne(sessions.status, `ended`)))
  if (Number(live) >= SESSION_TREE_MAX_SESSIONS) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `A thread runs at most ${SESSION_TREE_MAX_SESSIONS} sessions at once`,
    })
  }
}

async function insertSession(tx: Tx, values: SessionInsert & { id: string }): Promise<Session> {
  const [row] = await tx.insert(sessions).values(values).returning()
  return row!
}

/** NEW-1: `wakeOn` = the CALLING session watches those sessions. */
async function applyWakeOn(tx: Tx, caller: Session | null, input: SessionsSendInput) {
  if (!input.wakeOn?.length) return
  if (!caller) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `wakeOn needs a calling session (send it over your session's MCP key)`,
    })
  }
  await watchSessions(tx, caller.id, input.wakeOn)
}

/** EXP-1328: extra writes inside the send's own transaction (a suggestion
 *  card stamps its item there, so a refused stamp rolls the start back). */
export type SendWithin = (
  tx: Tx,
  result: { sessionId: string; messageId: string | null; rootId: string }
) => Promise<void>

/** The internal extras of a send (never on the wire). */
export interface SendExtras {
  /** A new child's stored title (a suggestion's title). */
  title?: string | null
  within?: SendWithin
}

interface SendOutcome extends SessionsSendResult {
  txid: number
  /** For the post-commit nudge + notify. */
  queued: { ownerId: string; deviceId: string } | null
  /** Rows this send ended (a move): relay kills post-commit. */
  endedIds?: string[]
}

export async function sendSession(args: {
  userId: string
  input: SessionsSendInput
  callerSessionId?: string | null
  /** The call came in over MCP (an agent's own key): only then may a message
   *  into ANOTHER session be that session's agent-authored input. Over tRPC
   *  the caller session merely stamps `author_session_id` on a person row. */
  viaMcp?: boolean
  extras?: SendExtras
}): Promise<SessionsSendResult & { txid: number }> {
  const caller = await loadCallerSession(args.userId, args.callerSessionId)
  const starter = effectiveStarter(args.userId, caller)
  const uploaderIds = [...new Set([args.userId, starter])]
  const { input } = args
  if (!input.sessionId && !input.agentId) assertRunnerVocabulary(input)

  const extras = args.extras ?? {}
  let outcome: SendOutcome
  if (input.sessionId) {
    outcome = await messageSession(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
  } else if (input.agentId) {
    outcome = await messageAgent(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
  } else if (input.appId) {
    outcome = await messageApp(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
  } else if (input.parentId) {
    outcome = await startChild(starter, uploaderIds, input, caller, extras)
  } else {
    outcome = await startRoot(starter, uploaderIds, input, caller)
  }

  if (outcome.endedIds?.length) await relayKillSessionsBestEffort(outcome.endedIds)
  if (outcome.queued) {
    const config = getSteerRelayConfig()
    if (config) {
      void relayPostNudge(config, outcome.queued.ownerId, outcome.queued.deviceId)
    }
  }
  if (outcome.messageId) fireAndForgetSessionMessageNotify({ messageId: outcome.messageId })
  return {
    sessionId: outcome.sessionId,
    messageId: outcome.messageId,
    status: outcome.status,
    txid: outcome.txid,
  }
}

/** EXP-849: the agent a session runs — its `cli`, else (NULL = the device's
 *  default agent) its host device's synced `launchDefaults.defaultAgent`.
 *  `null` = unknowable here; the device's own switch check decides. */
export async function effectiveCli(
  target: Pick<Session, `cli` | `deviceId` | `userId` | `teamId`>
): Promise<string | null> {
  if (target.cli) return target.cli
  const [device] = await db
    .select({ launchDefaults: devices.launchDefaults })
    .from(devices)
    .where(
      and(eq(devices.deviceId, target.deviceId), eq(devices.userId, await sessionDeviceOwner(target)))
    )
    .limit(1)
  return device?.launchDefaults?.defaultAgent ?? null
}

/** NEW-1: what a message's `deviceId` asks for. `none` = no device, or the
 *  current one; `direct` = a QUEUED session simply re-targets; `relocate` =
 *  a LIVE session ends here and continues on the target (same branch, the
 *  message pending there). An ENDED session's `deviceId` = the device its
 *  continuation starts on (handled by the continuation itself). */
type MovePlan =
  | { kind: `none` }
  | { kind: `direct` | `relocate`; target: MoveTarget }

async function planMove(session: Session, input: SessionsSendInput): Promise<MovePlan> {
  const deviceId = input.deviceId
  if (deviceId === undefined || deviceId === session.deviceId) return { kind: `none` }
  if (session.status === `ended` && !isPersistent(session)) return { kind: `none` }
  assertMovable(session)
  const target = await resolveMoveTarget(session, deviceId, {
    account: input.account ?? null,
    requireOnline: true,
  })
  return { kind: session.status === `queued` ? `direct` : `relocate`, target }
}

/** The row changed under the plan (a claim or an end raced the send). */
function moveRaced(): never {
  throw new TRPCError({ code: `CONFLICT`, message: `The session just changed; try again.` })
}

/** A queued row re-targets (guarded on the device and status the plan was
 *  made for: a row the device claimed since is CONFLICT, never re-targeted). */
async function applyDirectMove(tx: Tx, session: Session, target: MoveTarget) {
  const from = deviceName(
    await loadDeviceRow(session.deviceId, await sessionDeviceOwner(session)),
    session.deviceId
  )
  const updated = await tx
    .update(sessions)
    .set({
      deviceId: target.deviceId,
      account: target.account,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(sessions.id, session.id),
        eq(sessions.deviceId, session.deviceId),
        eq(sessions.status, `queued`)
      )
    )
    .returning({ id: sessions.id })
  if (updated.length === 0) moveRaced()
  await postMovedMessage(tx, session, movedText(from, target.name))
}

async function messageSession(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput,
  caller: Session | null,
  extras: SendExtras,
  viaMcp: boolean
): Promise<SendOutcome> {
  const target = await loadSessionForUser(userId, input.sessionId!)
  await assertCanSteer(userId, target)
  if (input.teamId && input.teamId !== target.teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The session is in another team` })
  }

  const hasMessage = !!input.text?.trim() || !!input.attachmentIds?.length
  const move = await planMove(target, input)
  // EXP-1323: another session's message over its MCP key is ITS input
  // (agent-authored, pending like a person's: the target wakes and reads
  // it). Over tRPC the header never speaks for a session: a person's
  // text stays a person row, the caller id only stamps it.
  const from = caller
    ? { sessionId: caller.id, asSession: viaMcp && caller.id !== target.id }
    : null

  if (move.kind === `relocate`) {
    // A live session moves: it ends here (its children re-parent onto the
    // continuation, its watchers follow) and continues on the target.
    const device = await resolveSessionDevice(userId, target.teamId, move.target.deviceId)
    const outcome = await db.transaction(async (tx) => {
      const txid = await generateTxId(tx)
      const [fresh] = await tx
        .select({ status: sessions.status, deviceId: sessions.deviceId })
        .from(sessions)
        .where(eq(sessions.id, target.id))
        .limit(1)
        .for(`update`)
      if (!fresh || fresh.status === `ended` || fresh.deviceId !== target.deviceId) moveRaced()
      const fromName = deviceName(
        await loadDeviceRow(target.deviceId, await sessionDeviceOwner(target)),
        target.deviceId
      )
      await postMovedMessage(tx, target, movedText(fromName, move.target.name))
      const ended = await tx
        .update(sessions)
        .set({ status: `ended`, endedAt: new Date(), busy: false, needsInput: false, blocked: null, updatedAt: new Date() })
        .where(eq(sessions.id, target.id))
        .returning({ id: sessions.id })
      const result = await continueSession(tx, {
        ended: target,
        userId,
        uploaderIds,
        input: { ...input, account: move.target.account ?? input.account },
        caller,
        from,
        extras,
        device: { deviceId: move.target.deviceId, ownerId: device.ownerId },
        adopt: true,
      })
      await applyWakeOn(tx, caller, input)
      return { ...result, txid, endedIds: ended.map((row) => row.id) }
    })
    return outcome
  }

  if (target.status !== `ended` || isPersistent(target)) {
    // EXP-849: `account` on a message = an account SWITCH. The synced
    // `sessions.account` is the login the session should run on; the host
    // device sees it move and resumes the SAME row on that login between
    // turns (`coding::pickup` Switch). An account-only send posts no message.
    // With a move `account` is the login to use on the TARGET.
    const switching =
      move.kind === `none` && input.account !== undefined && input.account !== target.account
    if (switching && (await effectiveCli(target)) === `codex`) {
      throw new TRPCError({
        code: `BAD_REQUEST`,
        message: `Only claude can switch accounts during a session.`,
      })
    }
    return db.transaction(async (tx) => {
      const txid = await generateTxId(tx)
      if (switching) {
        await tx
          .update(sessions)
          .set({ account: input.account, updatedAt: new Date() })
          .where(eq(sessions.id, target.id))
      }
      let queued: SendOutcome[`queued`] = null
      if (move.kind === `direct`) {
        await applyDirectMove(tx, target, move.target)
        queued = { ownerId: move.target.ownerId, deviceId: move.target.deviceId }
      }
      // NEW-1: the answer to the target's open card (first answer wins).
      if (input.answer) await answerSessionCardInTx(tx, target, input.answer)
      // NEW-1: issues on a message = FORWARDED to that session (the rows
      // move onto it; it takes them).
      if (input.issueIds?.length) {
        await forwardIssues(tx, {
          sessionId: target.id,
          issueIds: input.issueIds,
          source: caller ? `agent` : `user`,
          addedBy: userId,
          addedBySessionId: caller?.id ?? null,
        })
      }
      const messageId = await postSendMessage(tx, target, input, userId, uploaderIds, from)
      // A PERSON's message into a worker = it takes its issues again.
      if (messageId && !from?.asSession) await retakeHeldIssues(tx, target, userId)
      await applyWakeOn(tx, caller, input)
      await extras.within?.(tx, { sessionId: target.id, messageId, rootId: target.rootId })
      return { sessionId: target.id, messageId, status: target.status, txid, queued }
    })
  }
  if (!hasMessage && !input.issueIds?.length) {
    // Nothing to continue an ended session with: a switch alone has no run.
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `This session has ended: send a message to continue it`,
    })
  }
  if (input.answer) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `This session has ended: nothing to answer` })
  }

  // An ended session continues: a new session on its device (or the named
  // one), same pickers and branch, its first message = this one.
  const deviceId = input.deviceId ?? target.deviceId
  const device = await resolveSessionDevice(userId, target.teamId, deviceId)
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const result = await continueSession(tx, {
      ended: target,
      userId,
      uploaderIds,
      input,
      caller,
      from: caller ? { sessionId: caller.id, asSession: false } : null,
      extras,
      device: { deviceId, ownerId: device.ownerId },
      adopt: false,
    })
    await applyWakeOn(tx, caller, input)
    return { ...result, txid }
  })
}

/**
 * NEW-2: a message to an AGENT: the caller's own conversation session with
 * it (found, or created `queued` on the agent's device: own or shared with
 * the team). Issues on it are rows on the conversation; the message is the
 * conversation's pending input (agents never end: the row resumes).
 */
async function messageAgent(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput,
  caller: Session | null,
  extras: SendExtras,
  viaMcp: boolean
): Promise<SendOutcome> {
  const agent = await loadAgentForUser(userId, input.agentId!)
  if (input.teamId && input.teamId !== agent.teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The agent is in another team` })
  }
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const { session, created } = await conversationInTx(tx, agent, userId)
    const from = caller
      ? { sessionId: caller.id, asSession: viaMcp && caller.id !== session.id }
      : null
    if (input.issueIds?.length) {
      await assertIssuesInTeam(tx, agent.teamId, input.issueIds)
      await forwardIssues(tx, {
        sessionId: session.id,
        issueIds: input.issueIds,
        source: caller ? `agent` : `user`,
        addedBy: userId,
        addedBySessionId: caller?.id ?? null,
      })
    }
    const messageId = await postSendMessage(tx, session, input, userId, uploaderIds, from)
    await applyWakeOn(tx, caller, input)
    await extras.within?.(tx, { sessionId: session.id, messageId, rootId: session.id })
    return { sessionId: session.id, messageId, status: session.status, txid, queued: created }
  })
}

/**
 * VAPP-110: a message to an APP = into the caller's own EDITOR SESSION with
 * it. None yet: a fresh root `queued` on `deviceId` (else the caller's
 * default device), its first message the app brief + the text. Live or
 * idle: the message resumes it. ENDED (the editor was left): the root
 * continues as a child of itself (`messageSession` → `continueSession`),
 * briefed again.
 */
async function messageApp(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput,
  caller: Session | null,
  extras: SendExtras,
  viaMcp: boolean
): Promise<SendOutcome> {
  const app = await loadAppForUser(userId, input.appId!)
  if (input.teamId && input.teamId !== app.teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The app is in another team` })
  }
  const existing = await findEditorSession(db, app.id, userId)
  if (existing) {
    const { appId: _appId, ...rest } = input
    const text = existing.status === `ended` ? briefedText(app, input.text) : input.text
    return messageSession(userId, uploaderIds, { ...rest, sessionId: existing.id, text }, caller, extras, viaMcp)
  }
  const deviceId = input.deviceId ?? (await defaultDeviceFor(userId))
  if (!deviceId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Start a device to edit this app` })
  }
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const { session, created } = await createEditorSessionInTx(tx, app, userId, deviceId, runnerFields(input))
    const from = caller ? { sessionId: caller.id, asSession: viaMcp && caller.id !== session.id } : null
    const messageId = await postSendMessage(tx, session, { ...input, text: briefedText(app, input.text) }, userId, uploaderIds, from)
    await extras.within?.(tx, { sessionId: session.id, messageId, rootId: session.id })
    return { sessionId: session.id, messageId, status: session.status, txid, queued: created }
  })
}

/**
 * The continuation of an ENDED session: a new session on `device` with the
 * ended row's pickers, branch (the device reuses its worktree) and title,
 * taking over its issue rows. An ended ROOT continues as a child of itself;
 * an ended CHILD (#FEED-84) as its SIBLING: same parent and depth. `adopt`
 * (a move): the ended row's live children re-parent onto it and its
 * watchers follow.
 */
async function continueSession(
  tx: Tx,
  args: {
    ended: Session
    userId: string
    uploaderIds: string[]
    input: SessionsSendInput
    caller: Session | null
    from: { sessionId: string; asSession: boolean } | null
    extras: SendExtras
    device: { deviceId: string; ownerId: string }
    adopt: boolean
  }
): Promise<Omit<SendOutcome, `txid`>> {
  const { ended, userId, input, caller } = args
  const endedChild = ended.parentId !== null
  const depth = endedChild ? ended.depth || 1 : 1
  await assertTreeRoom(tx, ended.rootId, depth)
  const [root] = await tx.select().from(sessions).where(eq(sessions.id, ended.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const parentId = ended.parentId ?? root.id
  const id = crypto.randomUUID()
  const child = await insertSession(tx, {
    id,
    teamId: root.teamId,
    parentId,
    rootId: root.id,
    depth,
    kind: `thread`,
    branch: ended.branch,
    title: ended.title,
    userId,
    deviceId: args.device.deviceId,
    ...runnerFields({ account: input.account }, ended),
    mcpServerIds: ended.mcpServerIds,
    repositoryId: ended.repositoryId ?? root.repositoryId,
    status: `queued`,
    shared: root.shared,
  })
  await takeOverIssues(tx, ended, child, userId)
  if (args.adopt) {
    // (A moved ROOT's continuation is itself a child of the ended row.)
    await tx
      .update(sessions)
      .set({ parentId: child.id, updatedAt: new Date() })
      .where(
        and(eq(sessions.parentId, ended.id), ne(sessions.id, child.id), ne(sessions.status, `ended`))
      )
    await tx
      .update(sessions)
      .set({ wakeOn: sql`(${sessions.wakeOn} - ${ended.id}::text) || ${JSON.stringify([child.id])}::jsonb` })
      .where(sql`${sessions.wakeOn} @> ${JSON.stringify([ended.id])}::jsonb`)
    if (ended.wakeOn?.length) {
      await tx.update(sessions).set({ wakeOn: ended.wakeOn }).where(eq(sessions.id, child.id))
    }
  }
  if (input.issueIds?.length) {
    await assertIssuesInTeam(tx, root.teamId, input.issueIds)
    await forwardIssues(tx, {
      sessionId: child.id,
      issueIds: input.issueIds,
      source: caller ? `agent` : `user`,
      addedBy: userId,
      addedBySessionId: caller?.id ?? null,
    })
  }
  const messageId = await postSendMessage(tx, child, input, userId, args.uploaderIds, args.from)
  await postSystemCard(tx, endedChild ? parentId : root, { kind: `session`, sessionId: child.id })
  await args.extras.within?.(tx, { sessionId: child.id, messageId, rootId: root.id })
  return {
    sessionId: child.id,
    messageId,
    status: `queued`,
    queued: { ownerId: args.device.ownerId, deviceId: child.deviceId },
  }
}

async function startChild(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput,
  caller: Session | null,
  extras: SendExtras
): Promise<SendOutcome> {
  const parent = await loadSessionForUser(userId, input.parentId!)
  await assertCanSteer(userId, parent)
  if (input.teamId && input.teamId !== parent.teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The parent is in another team` })
  }
  const deviceId = input.deviceId ?? parent.deviceId
  const device = await resolveSessionDevice(userId, parent.teamId, deviceId)

  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    await assertTreeRoom(tx, parent.rootId, parent.depth + 1)
    const [root] = await tx.select().from(sessions).where(eq(sessions.id, parent.rootId)).limit(1)
    if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
    const mcpServerIds =
      (await assertMcpServersInTeam(tx, parent.teamId, input.mcpServerIds)) ?? parent.mcpServerIds
    const id = crypto.randomUUID()
    const child = await insertSession(tx, {
      id,
      teamId: parent.teamId,
      parentId: parent.id,
      rootId: parent.rootId,
      depth: parent.depth + 1,
      kind: `thread`,
      userId,
      deviceId,
      ...runnerFields(input, parent),
      // NEW-6 (N-1): plan mode is the ROOT's stance, never inherited: a
      // child plans only when its start says so.
      planMode: input.planMode ?? false,
      // EXP-1341: the parent's `name`, else a suggestion's title; omitted =
      // the session names itself (session-title.json).
      ...((input.name ?? extras.title)?.trim()
        ? { title: (input.name ?? extras.title)!.trim().slice(0, 255) }
        : {}),
      mcpServerIds,
      repositoryId: parent.repositoryId ?? root.repositoryId,
      status: `queued`,
      // One visibility per tree: a child inherits its root's.
      shared: root.shared,
    })
    if (input.issueIds?.length) {
      await linkIssues(tx, {
        sessionId: child.id,
        issueIds: input.issueIds,
        source: caller ? `agent` : `user`,
        addedBy: userId,
        addedBySessionId: caller?.id ?? null,
        quiet: true,
      })
    }
    const messageId = await postSendMessage(
      tx,
      child,
      input,
      userId,
      uploaderIds,
      caller ? { sessionId: caller.id, asSession: false } : null
    )
    await postSystemCard(tx, parent, { kind: `session`, sessionId: child.id })
    await applyWakeOn(tx, caller, input)
    await extras.within?.(tx, { sessionId: child.id, messageId, rootId: root.id })
    return {
      sessionId: child.id,
      messageId,
      status: `queued`,
      txid,
      queued: { ownerId: device.ownerId, deviceId },
    }
  })
}

/** A new root's repository: the explicit pick, else the first issue's board
 *  repository. */
async function defaultRepository(tx: Tx, input: SessionsSendInput): Promise<string | null> {
  const firstIssue = input.issueIds?.[0]
  if (firstIssue) {
    const [board] = await tx
      .select({ repositoryId: boards.repositoryId })
      .from(issues)
      .innerJoin(boards, eq(boards.id, issues.boardId))
      .where(eq(issues.id, firstIssue))
      .limit(1)
    if (board?.repositoryId) return board.repositoryId
  }
  return null
}

async function startRoot(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput,
  caller: Session | null
): Promise<SendOutcome> {
  const teamId = input.teamId ?? caller?.teamId
  if (!teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `teamId is required for a new session` })
  }
  await assertTeamMember(userId, teamId)

  // An agent's spawn runs on the agent's own device unless it names one:
  // the caller's root is a conversation session (its device = the agent's).
  const callerRoot =
    caller && caller.teamId === teamId
      ? caller.rootId === caller.id
        ? caller
        : ((await db.select().from(sessions).where(eq(sessions.id, caller.rootId)).limit(1))[0] ?? null)
      : null
  const deviceId = input.deviceId ?? (callerRoot?.agentId ? callerRoot.deviceId : undefined)
  if (!deviceId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Pick a device for the new session` })
  }
  const device = await resolveSessionDevice(userId, teamId, deviceId)

  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    if (input.issueIds) await assertIssuesInTeam(tx, teamId, input.issueIds)
    if (input.repositoryId) await assertRepositoryInTeam(tx, teamId, input.repositoryId)
    const mcpServerIds = await assertMcpServersInTeam(tx, teamId, input.mcpServerIds)

    // NEW-2: a prompt run: the body plus what was typed is the first
    // message; the prompt names the session unless a name was passed.
    let prompt: { name: string; body: string } | null = null
    if (input.promptId) {
      const [row] = await tx
        .select({ name: prompts.name, body: prompts.body })
        .from(prompts)
        .where(and(eq(prompts.id, input.promptId), eq(prompts.teamId, teamId)))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Prompt not found` })
      prompt = row
    }
    const text = prompt
      ? [prompt.body.trim(), input.text?.trim() ?? ``].filter(Boolean).join(`\n\n`)
      : input.text
    const title = input.name ?? prompt?.name

    const repositoryId = input.repositoryId ?? (await defaultRepository(tx, input))

    const id = crypto.randomUUID()
    const root = await insertSession(tx, {
      id,
      teamId,
      parentId: null,
      rootId: id,
      depth: 0,
      kind: `thread`,
      userId,
      deviceId,
      // NEW-1: the root WORKS: the composer's picks are its own.
      ...runnerFields(input),
      mcpServerIds,
      repositoryId,
      status: `queued`,
      shared: device.shared,
      ...(title ? { title: title.slice(0, 255) } : {}),
    })

    if (input.issueIds?.length) {
      await linkIssues(tx, {
        sessionId: root.id,
        issueIds: input.issueIds,
        source: caller ? `agent` : `user`,
        addedBy: userId,
        addedBySessionId: caller?.id ?? null,
        quiet: true,
      })
    }
    const messageId = await postSendMessage(tx, root, { ...input, text }, userId, uploaderIds)
    if (caller && caller.teamId === teamId) {
      // NEW-1: a root started from inside a session is WATCHED by its
      // starter: the bus delivers its transitions there (an agent's spawned
      // threads report back into its conversation). The card names it.
      await watchSessions(tx, caller.id, [root.id])
      await postSystemCard(tx, caller, { kind: `thread_started`, sessionId: root.id })
    }
    await applyWakeOn(tx, caller, input)
    return {
      sessionId: root.id,
      messageId,
      status: `queued`,
      txid,
      queued: { ownerId: device.ownerId, deviceId },
    }
  })
}
