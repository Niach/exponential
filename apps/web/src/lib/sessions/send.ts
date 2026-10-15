import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNull, ne, sql } from "drizzle-orm"
import type { z } from "zod"
import {
  SESSION_TREE_MAX_DEPTH,
  SESSION_TREE_MAX_SESSIONS,
  type AgentTrigger,
  type agentTriggersSchema,
  type SessionsSendInput,
  type SessionsSendResult,
} from "@exp/db-schema/domain"
import { contract } from "@exp/domain-contract"
import { db } from "@/db/connection"
import {
  actions,
  attachments,
  boards,
  devices,
  issues,
  mcpServers,
  pullRequests,
  repositories,
  sessions,
  type Session,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertTeamMember, assertTeamOwner, getTeamMember } from "@/lib/auth/membership"
import { resolveActionInputs } from "@/lib/action-inputs"
import { actionInputLookups } from "@/lib/sessions/action-lookups"
import { assertCanSteer, loadSessionForUser, sessionVisibleTo } from "@/lib/sessions/access"
import { assertIssuesInTeam, linkIssues } from "@/lib/sessions/issues"
import { postPersonMessage, postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"
import { buildAttachmentUrl, isAcceptedImageContentType } from "@/lib/storage/issue-attachments"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"
import { fireAndForgetSessionMessageNotify } from "@/lib/integrations/notifications"

// EXP-1291: `sessions.send` — THE entrypoint (tRPC `sessions.send` and MCP
// `exponential_sessions_send` both land here). Three forms, one shape:
//   - `sessionId`: a person's message into a session (the DB is the bus: a
//     `messages` row the device delivers, resuming an idle session in
//     place). An ENDED thread session continues in a NEW child of its root
//     carrying the message, announced by a `session` card on the root.
//   - `parentId`: a child (sub-session) of any visible session, depth + 1.
//   - neither: a new ROOT — a thread (issues / action / repo facets) or,
//     with `agent`, a team agent (owner-only).
// Every new session is a `queued` row naming its device; the device picks
// it up off the sessions shape (a best-effort relay nudge kills the latency).

type SessionInsert = typeof sessions.$inferInsert

/** The calling session (MCP `X-Exp-Session-Id`), when the caller may act as
 *  it: its starter or its host. Anything else is ignored, never trusted. */
async function loadCallerSession(userId: string, callerSessionId?: string | null) {
  if (!callerSessionId) return null
  const [row] = await db
    .select()
    .from(sessions)
    .where(eq(sessions.id, callerSessionId))
    .limit(1)
  if (!row || (row.userId !== userId && row.hostUserId !== userId)) return null
  return row
}

/**
 * The machine a new session runs on: the caller's own device, or one its
 * owner shared with the team (EXP-432). `shared` = that devices row shares
 * the team — the session's visibility for life. `hostUserId` = the owner
 * when it is not the starter.
 */
export async function resolveSessionDevice(
  userId: string,
  teamId: string,
  deviceId: string
): Promise<{ shared: boolean; hostUserId: string | null; ownerId: string }> {
  const rows = await db
    .select({ userId: devices.userId, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, deviceId))
  const own = rows.find((row) => row.userId === userId)
  if (own) {
    return {
      shared: (own.sharedTeamIds ?? []).includes(teamId),
      hostUserId: null,
      ownerId: userId,
    }
  }
  for (const row of rows) {
    if (!(row.sharedTeamIds ?? []).includes(teamId)) continue
    // Sharing is the owner's consent while they are still a member.
    if (!(await getTeamMember(row.userId, teamId))) continue
    return { shared: true, hostUserId: row.userId, ownerId: row.userId }
  }
  throw new TRPCError({
    code: `PRECONDITION_FAILED`,
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

/** Agent triggers as stored: every trigger carries an id (minted here). */
export function withTriggerIds(triggers: z.infer<typeof agentTriggersSchema>): AgentTrigger[] {
  return triggers.map(
    (trigger) => ({ ...trigger, id: trigger.id ?? crypto.randomUUID() }) as AgentTrigger
  )
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

/** The person message of a send (text and/or attachments), or null. */
async function postSendMessage(
  tx: Tx,
  session: Pick<Session, `id` | `rootId` | `teamId` | `shared` | `userId`>,
  input: SessionsSendInput,
  authorId: string,
  uploaderIds: string[]
): Promise<string | null> {
  const text = input.text ?? ``
  if (!text.trim() && !input.attachmentIds?.length) return null
  const messageId = await postPersonMessage(tx, session, authorId, text)
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
  return caller && caller.hostUserId === userId ? caller.userId : userId
}

// The composer's value sets (EXP-149/201): an agent CLI from the contract,
// and a model/effort from THAT agent's lists (blank = the CLI default).
const MODELS: Record<string, readonly string[]> = {
  claude: contract.codingModel.values,
  codex: contract.codexModel.values,
}
const EFFORTS: Record<string, readonly string[]> = {
  claude: contract.codingEffort.values,
  codex: contract.codexEffort.values,
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

/** The runner pickers of a new session, the input's or inherited ones. */
function runnerFields(input: SessionsSendInput, from?: Session | null) {
  return {
    cli: input.cli ?? from?.cli ?? null,
    account: input.account ?? from?.account ?? null,
    model: input.model ?? from?.model ?? null,
    effort: input.effort ?? from?.effort ?? null,
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

interface SendOutcome extends SessionsSendResult {
  txid: number
  /** For the post-commit nudge + notify. */
  queued: { ownerId: string; deviceId: string } | null
}

export async function sendSession(args: {
  userId: string
  input: SessionsSendInput
  callerSessionId?: string | null
}): Promise<SessionsSendResult & { txid: number }> {
  const caller = await loadCallerSession(args.userId, args.callerSessionId)
  const starter = effectiveStarter(args.userId, caller)
  const uploaderIds = [...new Set([args.userId, starter])]
  const { input } = args
  if (!input.sessionId) assertRunnerVocabulary(input)

  let outcome: SendOutcome
  if (input.sessionId) {
    outcome = await messageSession(starter, uploaderIds, input)
  } else if (input.parentId) {
    outcome = await startChild(starter, uploaderIds, input)
  } else {
    outcome = await startRoot(starter, uploaderIds, input, caller)
  }

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

async function messageSession(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput
): Promise<SendOutcome> {
  const target = await loadSessionForUser(userId, input.sessionId!)
  await assertCanSteer(userId, target)
  if (input.teamId && input.teamId !== target.teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The session is in another team` })
  }

  const hasMessage = !!input.text?.trim() || !!input.attachmentIds?.length
  if (target.status !== `ended` || target.kind === `agent`) {
    // EXP-849: `account` on a message = an account SWITCH. The synced
    // `sessions.account` is the login the session should run on; the host
    // device sees it move and resumes the SAME row on that login between
    // turns (`coding::pickup` Switch). An account-only send posts no message.
    const switching = input.account !== undefined && input.account !== target.account
    if (switching && target.cli === `codex`) {
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
      const messageId = await postSendMessage(tx, target, input, userId, uploaderIds)
      return { sessionId: target.id, messageId, status: target.status, txid, queued: null }
    })
  }
  if (!hasMessage) {
    // Nothing to continue an ended thread with: a switch alone has no run.
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `This session has ended: send a message to continue it`,
    })
  }

  // An ended thread continues: a new child of the ROOT on the same device
  // with the same pickers, its first message = this one.
  const device = await resolveSessionDevice(userId, target.teamId, target.deviceId)
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    await assertTreeRoom(tx, target.rootId, 1)
    const [root] = await tx.select().from(sessions).where(eq(sessions.id, target.rootId)).limit(1)
    if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
    const id = crypto.randomUUID()
    const child = await insertSession(tx, {
      id,
      teamId: root.teamId,
      parentId: root.id,
      rootId: root.id,
      depth: 1,
      kind: `thread`,
      userId,
      hostUserId: device.hostUserId,
      deviceId: target.deviceId,
      // A switch sent with the message carries into the continuation.
      ...runnerFields({ account: input.account }, target),
      mcpServerIds: target.mcpServerIds,
      repositoryId: target.repositoryId ?? root.repositoryId,
      status: `queued`,
      shared: root.shared,
    })
    const messageId = await postSendMessage(tx, child, input, userId, uploaderIds)
    await postSystemCard(tx, root, { kind: `session`, sessionId: child.id })
    return {
      sessionId: child.id,
      messageId,
      status: `queued`,
      txid,
      queued: { ownerId: device.ownerId, deviceId: child.deviceId },
    }
  })
}

async function startChild(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput
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
      hostUserId: device.hostUserId,
      deviceId,
      ...runnerFields(input, parent),
      mcpServerIds,
      repositoryId: parent.repositoryId ?? root.repositoryId,
      status: `queued`,
      // One visibility per tree: a child inherits its root's.
      shared: root.shared,
    })
    const messageId = await postSendMessage(tx, child, input, userId, uploaderIds)
    await postSystemCard(tx, parent, { kind: `session`, sessionId: child.id })
    return {
      sessionId: child.id,
      messageId,
      status: `queued`,
      txid,
      queued: { ownerId: device.ownerId, deviceId },
    }
  })
}

/** A new root's repository: the explicit pick, else what its facets name —
 *  a `repo` / `pr` action input, else the first issue's board repository. */
async function defaultRepository(
  tx: Tx,
  input: SessionsSendInput,
  actionInputs: Array<{ type: string; value: string }>
): Promise<string | null> {
  const repoInput = actionInputs.find((entry) => entry.type === `repo`)
  if (repoInput) return repoInput.value
  const prInput = actionInputs.find((entry) => entry.type === `pr`)
  if (prInput) {
    const [pr] = await tx
      .select({ repositoryId: pullRequests.repositoryId })
      .from(pullRequests)
      .where(eq(pullRequests.id, prInput.value))
      .limit(1)
    if (pr) return pr.repositoryId
  }
  const firstIssue =
    input.issueIds?.[0] ?? actionInputs.find((entry) => entry.type === `issue`)?.value
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
  if (input.agent) await assertTeamOwner(userId, teamId)
  else await assertTeamMember(userId, teamId)

  // An agent's spawn runs on the agent's own device unless it names one.
  const callerAgentRoot =
    caller && caller.teamId === teamId
      ? caller.kind === `agent`
        ? caller
        : caller.rootId !== caller.id
          ? (
              await db
                .select()
                .from(sessions)
                .where(and(eq(sessions.id, caller.rootId), eq(sessions.kind, `agent`)))
                .limit(1)
            )[0] ?? null
          : null
      : null
  const deviceId = input.deviceId ?? callerAgentRoot?.deviceId
  if (!deviceId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Pick a device for the new session` })
  }
  const device = await resolveSessionDevice(userId, teamId, deviceId)

  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    if (input.issueIds) await assertIssuesInTeam(tx, teamId, input.issueIds)
    if (input.repositoryId) await assertRepositoryInTeam(tx, teamId, input.repositoryId)
    const mcpServerIds = await assertMcpServersInTeam(tx, teamId, input.mcpServerIds)

    let action: { id: string; name: string } | null = null
    let actionInputs: Array<{ key: string; type: string; value: string }> = []
    if (input.actionId) {
      const [row] = await tx
        .select({ id: actions.id, name: actions.name, inputs: actions.inputs })
        .from(actions)
        .where(and(eq(actions.id, input.actionId), eq(actions.teamId, teamId)))
        .limit(1)
      if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Action not found` })
      const resolved = await resolveActionInputs(
        row.inputs ?? [],
        input.inputs ?? {},
        teamId,
        actionInputLookups(tx)
      )
      if (!resolved.ok) throw new TRPCError({ code: `BAD_REQUEST`, message: resolved.message })
      action = { id: row.id, name: row.name }
      actionInputs = resolved.inputs
    }

    const repositoryId =
      input.repositoryId ?? (input.agent ? null : await defaultRepository(tx, input, actionInputs))

    const id = crypto.randomUUID()
    const agent = input.agent
    const spawnedBy = callerAgentRoot?.id ?? caller?.id ?? null
    const root = await insertSession(tx, {
      id,
      teamId,
      parentId: null,
      rootId: id,
      depth: 0,
      kind: agent ? `agent` : `thread`,
      userId,
      hostUserId: device.hostUserId,
      deviceId,
      ...runnerFields(input),
      mcpServerIds,
      repositoryId,
      status: `queued`,
      shared: device.shared,
      actionId: action?.id ?? null,
      actionName: action?.name ?? null,
      actionInputs: action
        ? Object.fromEntries(actionInputs.map((entry) => [entry.key, entry.value]))
        : null,
      spawnedBySessionId: spawnedBy && caller && caller.teamId === teamId ? spawnedBy : null,
      ...(agent
        ? {
            name: agent.name,
            icon: agent.icon,
            color: agent.color,
            instructions: agent.instructions,
            triggers: withTriggerIds(agent.triggers),
          }
        : {}),
    })

    if (input.issueIds?.length) {
      await linkIssues(tx, {
        rootId: root.id,
        issueIds: input.issueIds,
        source: caller ? `agent` : `user`,
        addedBy: userId,
        addedBySessionId: caller?.id ?? null,
        quiet: true,
      })
    }
    const messageId = await postSendMessage(tx, root, input, userId, uploaderIds)
    if (callerAgentRoot && sessionVisibleTo(callerAgentRoot, userId)) {
      await postSystemCard(tx, callerAgentRoot, { kind: `thread_started`, sessionId: root.id })
    }
    return {
      sessionId: root.id,
      messageId,
      status: `queued`,
      txid,
      queued: { ownerId: device.ownerId, deviceId },
    }
  })
}
