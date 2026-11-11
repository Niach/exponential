import { AsyncLocalStorage } from "node:async_hooks"
import { isPersistent, linksIssues } from "@exp/domain-contract/session-kind"
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
  conversations,
  agents,
  apps,
  attachments,
  boards,
  devices,
  installs,
  issues,
  mcpServers,
  messages,
  prompts,
  repositories,
  sessionIssues,
  sessions,
  userVoiceSettings,
  type Session,
} from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertSkillsInTeam } from "@/lib/skills-guard"
import { uniqueViolationConstraint } from "@/lib/trpc/db-errors"
import { startDiskWarning } from "@/lib/device-disk"
import { assertTeamMember, getMemberTeamIds, getPersonalTeamId, getTeamMember } from "@/lib/auth/membership"
import { rules as eagentRules } from "@exp/eagent/copy"
import { treeTeams } from "@/lib/sessions/tree-teams"
import { conversationInTx, loadAgentForUser } from "@/lib/agents/conversation"
import {
  briefedText,
  createEditorSessionInTx,
  defaultDeviceFor,
  findEditorSession,
  loadAppForUser,
} from "@/lib/apps/editor-session"
import { flattenAccounts } from "@/lib/accounts/account-option"
import { answerSessionCardInTx } from "@/lib/sessions/answer"
import { SEALED_CARD_PLAIN_ANSWER_MESSAGE } from "@/lib/sessions/card-answer"
import {
  assertCanSteer,
  isSessionPrincipal,
  loadSessionForUser,
  sessionDeviceOwner,
} from "@/lib/sessions/access"
import { closeAskMirrorsOf, rewatchRevived, watchSessions } from "@/lib/sessions/bus"
import { clearBlockedInTx } from "@/lib/sessions/watchdog"
import { SUBAGENT_NO_CHILDREN, subagentRedirectText } from "@/lib/sessions/subagent"
import {
  agentForwardLine,
  assertIssuesInTeam,
  forwardIssues,
  linkIssues,
  manualLinkText,
  retakeHeldIssues,
  takeOverIssues,
} from "@/lib/sessions/issues"
import { relayKillSessionsBestEffort } from "@/lib/sessions/lifecycle"
import {
  postMovedMessage,
  postPersonMessage,
  postSessionInput,
  postSystemCard,
  sessionConversation,
} from "@/lib/sessions/messages"
import { markTreesReadBy } from "@/lib/sessions/reads"
import {
  assertMovable,
  childAccountOnDevice,
  deviceName,
  loadDeviceRow,
  movedText,
  recordMoveInTx,
  resolveMoveTarget,
  type MoveTarget,
} from "@/lib/sessions/move"
import { resolveSessionHost, SHARING_ORDER } from "@/lib/sessions/host"
import { DEVICE_OTHER_TEAM_MESSAGE, deviceUsableInTeam } from "@/lib/session-device-move"
import { agentReuseRefusal } from "@/lib/sessions/agent-reuse"
import { meetingAgentIsPrivate, MEETING_SHARED_REFUSED } from "@/lib/sessions/meeting-access"
import type { Db, Tx } from "@/lib/sessions/tx"
import { buildAttachmentUrl } from "@/lib/storage/issue-attachments"
import { attachmentLine } from "@/lib/sessions/attachment-lines"
import { parseBodyEnvelope } from "@exp/domain-contract/sealed-content"
import { composeIssueBrief } from "@exp/domain-contract/session-brief"
import { isSealedBody, validSealedBody } from "@/lib/sessions/sealed"
import { forwardMessageFiles, retargetBody } from "@/lib/sessions/forward-attachments"
import { parseVersionTuple } from "@/lib/client-version"
import { createTreeKeyInTx, dropNonReaderKeysInTx } from "@/lib/sessions/tree-keys"
import { hidePrivateLinesInTx } from "@/lib/sessions/private-lines"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"
import { fireAndForgetSessionMessageNotify } from "@/lib/integrations/notifications"

// EXP-1291/NEW-1/NEW-2: `sessions.send` — THE entrypoint (tRPC
// `sessions.send` and MCP `exponential_sessions_send` both land here). Four
// forms, one shape:
//   - `sessionId`: a person's (or a session's) message into a session (the
//     DB is the bus: a `messages` row the device delivers, resuming an
//     idle session in place). `answer` answers its open card; `issueIds`
//     forward issues to it; `deviceId` moves it (a queued row re-targets, a
//     live ROOT re-queues there in place, a live child ends here and continues
//     there). An ENDED chat ROOT
//     revives (FEED-134): the SAME row goes idle with the message pending;
//     an ENDED child continues in a NEW session carrying the message, as
//     its sibling, on the same branch.
//   - `agentId`: a message to a team AGENT = into the caller's own
//     conversation session with it (one per agent and member, created on
//     first use on the agent's device, never ending).
//   - `parentId`: a child (sub-session) of any visible session, depth + 1,
//     inheriting its parent's runner picks.
//   - neither: a new ROOT — a chat that WORKS (the composer's picks, a
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

export const AGENT_TEAM_ONLY_MESSAGE = `A team agent works only in its own team`
/** #EXP-1720 (eagent rules.json copy.teamOutOfScope): a tree's new session
 *  or send outside its teams. */
export const TEAM_OUT_OF_SCOPE_MESSAGE = eagentRules.copy.teamOutOfScope
/** #EXP-1720 (copy.scopeAssistantOnly): a Teams pick into anything but the
 *  Assistant's conversation. */
export const SCOPE_ASSISTANT_ONLY_MESSAGE = eagentRules.copy.scopeAssistantOnly

/** #EXP-1383: a tree's ACTING agent: the agent of a conversation root,
 *  else the agent whose session started the root (`started_by_agent_id`). */
export function treeAgentId(root: Pick<Session, `agentId` | `startedByAgentId`>): string | null {
  return root.agentId ?? root.startedByAgentId ?? null
}

/** The acting agent of the caller's tree (null = not an agent tree). */
async function callerTreeAgentId(caller: Session | null): Promise<string | null> {
  if (!caller) return null
  if (caller.rootId === caller.id) return treeAgentId(caller)
  const [root] = await db
    .select({ agentId: sessions.agentId, startedByAgentId: sessions.startedByAgentId })
    .from(sessions)
    .where(eq(sessions.id, caller.rootId))
    .limit(1)
  return root ? treeAgentId(root) : null
}


/** The team a send lands in (null = its target does not exist: the form's
 *  own lookup answers NOT_FOUND). */
async function sendTargetTeam(input: SessionsSendInput, caller: Session | null): Promise<string | null> {
  const sessionTeam = async (id: string) =>
    (await db.select({ teamId: sessions.teamId }).from(sessions).where(eq(sessions.id, id)).limit(1))[0]?.teamId ??
    null
  if (input.sessionId) return sessionTeam(input.sessionId)
  if (input.agentId) {
    return (
      (await db.select({ teamId: agents.teamId }).from(agents).where(eq(agents.id, input.agentId)).limit(1))[0]
        ?.teamId ?? null
    )
  }
  if (input.appId) {
    return (
      (await db.select({ teamId: apps.teamId }).from(apps).where(eq(apps.id, input.appId)).limit(1))[0]?.teamId ??
      null
    )
  }
  if (input.parentId) return sessionTeam(input.parentId)
  return input.teamId ?? caller?.teamId ?? null
}

/** #EXP-1383: a caller speaking for an agent-tree session sends into, and
 *  starts sessions in, its tree's teams only (FORBIDDEN else). #EXP-1720:
 *  a team agent's tree = its team; the Assistant's = its scope. */
async function assertAgentTreeTeam(input: SessionsSendInput, caller: Session | null): Promise<void> {
  if (!caller || !(await callerTreeAgentId(caller))) return
  const allowed = await treeTeams(caller.rootId)
  const target = await sendTargetTeam(input, caller)
  if ((input.teamId && !allowed.includes(input.teamId)) || (target && !allowed.includes(target))) {
    throw new TRPCError({
      code: `FORBIDDEN`,
      message: allowed.length > 1 ? TEAM_OUT_OF_SCOPE_MESSAGE : AGENT_TEAM_ONLY_MESSAGE,
    })
  }
}

/** #EXP-1720 (eagent tree.json): a child starts in the named team when its
 *  tree works there (the Assistant's children spawn in their target team),
 *  else in its parent's. */
async function childTeam(input: SessionsSendInput, parent: Session): Promise<string> {
  if (!input.teamId || input.teamId === parent.teamId) return parent.teamId
  if (!(await treeTeams(parent.rootId)).includes(input.teamId)) {
    throw new TRPCError({ code: `FORBIDDEN`, message: TEAM_OUT_OF_SCOPE_MESSAGE })
  }
  return input.teamId
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
  deviceId: string,
  exec: Db = db
): Promise<{ shared: boolean; ownerId: string }> {
  const rows = await exec
    .select({ userId: devices.userId, sharedTeamIds: devices.sharedTeamIds })
    .from(devices)
    .where(eq(devices.deviceId, deviceId))
    .orderBy(...SHARING_ORDER)
  const own = rows.find((row) => row.userId === userId)
  if (own) {
    // #EXP-1440: a machine shared with other teams belongs to them; only a
    // personal one (shared with no team) or one shared with THIS team runs
    // this team's sessions. #EXP-1720: the owner's personal team (their
    // Assistant) runs on any machine of theirs.
    if (!deviceUsableInTeam(own, teamId) && teamId !== (await getPersonalTeamId(userId))) {
      throw new TRPCError({ code: `FORBIDDEN`, message: DEVICE_OTHER_TEAM_MESSAGE })
    }
    return { shared: (own.sharedTeamIds ?? []).includes(teamId), ownerId: userId }
  }
  // FEED-148: the first sharing row (`SHARING_ORDER`), as every host read.
  for (const row of rows) {
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

/** #EXP-1259 (FEED-149): a start's device, read INSIDE its transaction
 *  under the machine's share lock, SHARED. `devices.setShared` and the
 *  roster leave take the same key EXCLUSIVELY (`lockDeviceShares`,
 *  lib/sessions/host.ts: `device-share:<deviceId>`, hashtextextended) and
 *  read the runs they end under it, so a start either commits before the
 *  withdrawal (and is in that set) or reads the box without its share
 *  (NOT_FOUND): never a run on a box whose consent is gone. */
export async function resolveSessionDeviceInTx(
  tx: Tx,
  userId: string,
  teamId: string,
  deviceId: string
): Promise<{ shared: boolean; ownerId: string }> {
  await tx.execute(sql`SELECT pg_advisory_xact_lock_shared(hashtextextended(${`device-share:${deviceId}`}, 0))`)
  return resolveSessionDevice(userId, teamId, deviceId, tx)
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

/** #EXP-1656: an agent's default skills (null = none). */
async function agentSkillIds(tx: Tx, agentId: string): Promise<string[] | null> {
  const [row] = await tx.select({ skillIds: agents.skillIds }).from(agents).where(eq(agents.id, agentId)).limit(1)
  return row?.skillIds?.length ? row.skillIds : null
}

/**
 * Claims eager uploads (team-scoped `attachments` rows with no owner yet,
 * uploaded by the sender) onto the message and names them in clear
 * (`messages.attachment_ids`, EXP-1458). #EXP-1682: an id of a file another
 * message already carries, which the sender may read, is FORWARDED: copied
 * onto this message under a new id (`forwardMessageFiles`), no re-upload.
 * A PLAINTEXT body also gets an embed line for every one it does not
 * already reference (attachments never sync, so the body is how older
 * clients and the device find them); a SEALED body is never touched: the
 * writer put its embeds inside.
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
  const requested = [...new Set(args.ids)]
  if (requested.length === 0) return
  const claimed = await tx
    .update(attachments)
    .set({ messageId: args.messageId })
    .where(
      and(
        inArray(attachments.id, requested),
        eq(attachments.teamId, args.teamId),
        isNull(attachments.issueId),
        isNull(attachments.draftId),
        isNull(attachments.messageId),
        // VAPP-221: an app's record file is never a message's.
        isNull(attachments.appId),
        inArray(attachments.uploaderId, args.uploaderIds)
      )
    )
    .returning({ id: attachments.id })
  const claimedIds = new Set(claimed.map((row) => row.id))
  const forwarded = await forwardMessageFiles(tx, {
    ids: requested.filter((id) => !claimedIds.has(id)),
    teamId: args.teamId,
    readerIds: args.uploaderIds,
    uploaderId: args.uploaderIds[0]!,
    owner: { kind: `message`, messageId: args.messageId },
  })
  if (claimed.length + forwarded.size !== requested.length) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Attach files you uploaded to this team, or files of messages you can read (each file once)`,
    })
  }
  const ids = requested.map((id) => forwarded.get(id) ?? id)
  await tx.update(messages).set({ attachmentIds: ids }).where(eq(messages.id, args.messageId))
  if (isSealedBody(args.body)) return
  const rows = await tx
    .select({ id: attachments.id, filename: attachments.filename, contentType: attachments.contentType })
    .from(attachments)
    .where(inArray(attachments.id, ids))
  const byId = new Map(rows.map((row) => [row.id, row]))
  const retargeted = retargetBody(args.body, forwarded)
  const lines = ids
    .filter((id) => !retargeted.includes(buildAttachmentUrl(id)))
    .map((id) => attachmentLine(byId.get(id)!))
  if (lines.length === 0 && retargeted === args.body) return
  const body = [retargeted.trimEnd(), ...lines].filter(Boolean).join(`\n\n`)
  await tx.execute(sql`UPDATE messages SET body = ${body} WHERE id = ${args.messageId}`)
}

export const SEALED_NO_KEY_MESSAGE = `This chat has no key for encrypted messages yet: pass seal with the first one`

/** EXP-1458: a sealed text names an epoch its tree has (a keyless tree, or
 *  a future epoch, could never be opened by anyone). */
async function assertSealedForTree(tx: Tx, rootId: string, text: string): Promise<void> {
  if (!validSealedBody(text)) throw new TRPCError({ code: `BAD_REQUEST`, message: `Invalid encrypted text` })
  const envelope = parseBodyEnvelope(text)
  if (envelope.kind !== `sealed`) return
  const [root] = await tx.select({ sealEpoch: sessions.sealEpoch }).from(sessions).where(eq(sessions.id, rootId)).limit(1)
  if (root?.sealEpoch == null) throw new TRPCError({ code: `BAD_REQUEST`, message: SEALED_NO_KEY_MESSAGE })
  if (envelope.epoch > root.sealEpoch) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The chat's key is at epoch ${root.sealEpoch}` })
  }
}

export const PLAIN_IN_SEALED_TREE_MESSAGE = `This chat is end-to-end encrypted: its messages must be encrypted too`

/** #FEED-184: the first desktop/CLI release whose agents seal every send
 *  and card answer, even when launched before the device's key holder ran. */
export const SEALING_WRITER_MIN_VERSION = `0.14.96`

/** #FEED-184: what a refused plaintext write says about its writer: the
 *  device (and its version) the session runs on, and what sealing needs. */
export async function plainWriterHint(
  database: Pick<Db, `select`>,
  writerSessionId: string | null | undefined
): Promise<string> {
  const needs = `the desktop app or CLI ${SEALING_WRITER_MIN_VERSION} or newer`
  // #FEED-186: no calling session = the sender is no Exponential session
  // (an MCP client outside one, or an app that did not seal): say so
  // instead of blaming devices that may well be current.
  if (!writerSessionId) {
    return `: this sender is not an Exponential session (an MCP client outside a session cannot encrypt). Send from the web, iOS or Android app, or from a session on ${needs}`
  }
  const [session] = await database
    .select({ userId: sessions.userId, deviceId: sessions.deviceId })
    .from(sessions)
    .where(eq(sessions.id, writerSessionId))
    .limit(1)
  if (!session) return `: encryption needs ${needs} (or an up-to-date web, iOS or Android app)`
  // #FEED-186: devices rows are per (user, deviceId): a teammate's session
  // on a shared device has no row under its starter, so read the device by
  // id, the starter's own row first, else the latest-seen one.
  const [writer] = await database
    .select({ label: devices.label, kind: devices.kind, version: devices.version })
    .from(devices)
    .where(eq(devices.deviceId, session.deviceId))
    .orderBy(sql`(${devices.userId} = ${session.userId}) DESC`, sql`${devices.lastSeenAt} DESC NULLS LAST`)
    .limit(1)
  if (!writer) {
    return `: its device ${session.deviceId} is not registered. Encryption needs ${needs}, signed in on that machine`
  }
  const app = writer.kind === `desktop` ? `desktop app` : `CLI`
  const version = writer.version ? ` ${writer.version}` : ``
  const restart = writer.kind === `desktop` ? `app` : `daemon`
  // #FEED-185: a writer already on a sealing release is never told to
  // update: its key holder was not running (signed out, offline, or its
  // install not registered) when its agent launched.
  if (writerSeals(writer.version)) {
    return `: device "${writer.label}" (${app}${version}) sent it in clear because its key holder was not running (signed out, offline or not registered when the session started). Restart its ${restart} while signed in and online, then send again`
  }
  return `: device "${writer.label}" (${app}${version}) sent it in clear. Encryption needs ${needs}: update it and restart its ${restart}`
}

/** #FEED-185: the writer's release seals every send (unknown = no). */
export function writerSeals(version: string | null | undefined): boolean {
  const have = version ? parseVersionTuple(version) : null
  const need = parseVersionTuple(SEALING_WRITER_MIN_VERSION)!
  if (!have) return false
  for (let i = 0; i < 3; i++) if (have[i] !== need[i]) return have[i]! > need[i]!
  return true
}

/** The tree has a key (`seal_epoch`). A tree started in clear has none until
 *  a client keys it: it is sealed from then on, and only then. */
export async function treeIsKeyed(tx: Pick<Db, `select`>, rootId: string): Promise<boolean> {
  const [root] = await tx.select({ sealEpoch: sessions.sealEpoch }).from(sessions).where(eq(sessions.id, rootId)).limit(1)
  return root?.sealEpoch != null
}

/** EXP-1472: a keyed tree takes no plaintext content from a writer (server
 *  lines are not writers'). Every client seals for a keyed root; a body that
 *  arrives in clear is a writer that failed open, refused before it lands.
 *  #FEED-184: the refusal names the writer's device and the version needed. */
export async function assertNotPlainInSealedTree(
  tx: Pick<Db, `select`>,
  rootId: string,
  writerSessionId?: string | null
): Promise<void> {
  const [root] = await tx.select({ sealEpoch: sessions.sealEpoch }).from(sessions).where(eq(sessions.id, rootId)).limit(1)
  if (root?.sealEpoch == null) return
  const hint = await plainWriterHint(tx, writerSessionId)
  throw new TRPCError({ code: `BAD_REQUEST`, message: `${PLAIN_IN_SEALED_TREE_MESSAGE}${hint}` })
}

/** The person message of a send (text and/or attachments), or null.
 *  EXP-1323: `from` = the calling session (its MCP key): stamped as
 *  `author_session_id`; `asSession` = the message is that SESSION's input
 *  into another one (a child asking its parent, #EXP-1452 an agent starting
 *  a session), agent-authored. */
/** #EXP-1720 (eagent tree.json, emcp access.json `assistantScope`): a
 *  Teams pick is the member's Assistant's alone: into its conversation ROOT,
 *  by its member. Stored ∩ their teams (deduplicated, the root's own team
 *  dropped); null = every team. Returns what was stored. */
async function applyTeamsPick(
  tx: Tx,
  session: Pick<Session, `id` | `rootId` | `teamId` | `userId`>,
  authorId: string,
  picked: string[] | null
): Promise<string[] | null> {
  const [root] = await tx
    .select({ parentId: sessions.parentId, userId: sessions.userId, role: agents.role })
    .from(sessions)
    .leftJoin(agents, eq(agents.id, sessions.agentId))
    .where(eq(sessions.id, session.id))
    .limit(1)
  if (!root || root.parentId !== null || root.role !== `assistant` || root.userId !== authorId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: SCOPE_ASSISTANT_ONLY_MESSAGE })
  }
  const member = new Set(await getMemberTeamIds(authorId))
  const scope = picked === null ? null : [...new Set(picked)].filter((id) => id !== session.teamId && member.has(id))
  await tx.update(sessions).set({ scopeTeamIds: scope, updatedAt: new Date() }).where(eq(sessions.id, session.id))
  return scope
}

async function postSendMessage(
  tx: Tx,
  session: Pick<Session, `id` | `rootId` | `teamId` | `shared` | `userId`>,
  input: SessionsSendInput,
  authorId: string,
  uploaderIds: string[],
  from?: { sessionId: string; asSession: boolean } | null,
  issueBrief?: { brief: string; line: string | null } | null
): Promise<string | null> {
  // EXP-1458: the install named by `seal` is the CALLER's (a hosted run's
  // agent acts with its host's key, so the caller is the uploader set's
  // first entry, never the effective starter).
  const sealUserId = uploaderIds[0]!
  // #EXP-1720: the turn's Teams pick becomes the Assistant root's scope, even with nothing to post.
  const scope = input.scopeTeamIds === undefined ? undefined : await applyTeamsPick(tx, session, authorId, input.scopeTeamIds)
  let text = input.text ?? ``
  // EXP-1458: the tree's key arrives with its first sealed message.
  if (input.seal) await createTreeKeyInTx(tx, sealUserId, { ...input.seal, rootId: session.rootId })
  // #EXP-1584/#EXP-1686: a bare forward still wakes the agent, in a sealed
  // tree too: a server line, legal plaintext there (the identifiers ride in
  // the clear `issue_brief` anyway).
  const serverLine = !!issueBrief && !text.trim() && !input.attachmentIds?.length
  if (serverLine) text = issueBrief.line ?? ``
  if (!text.trim() && !input.attachmentIds?.length) return null
  if (isSealedBody(text)) await assertSealedForTree(tx, session.rootId, text)
  else if (text.trim() && !serverLine) await assertNotPlainInSealedTree(tx, session.rootId, from?.sessionId)
  if (input.refIssueIds?.length) await assertIssuesInTeam(tx, session.teamId, input.refIssueIds)
  const messageId = from?.asSession
    ? await postSessionInput(tx, session, from.sessionId, text)
    : await postPersonMessage(tx, session, authorId, text, from?.sessionId ?? null)
  if (input.mentionUserIds?.length || input.refIssueIds?.length || scope !== undefined) {
    await tx
      .update(messages)
      .set({
        ...(input.mentionUserIds?.length ? { mentionUserIds: [...new Set(input.mentionUserIds)] } : {}),
        ...(input.refIssueIds?.length ? { refIssueIds: [...new Set(input.refIssueIds)] } : {}),
        ...(scope !== undefined ? { scopeTeamIds: scope } : {}),
      })
      .where(eq(messages.id, messageId))
  }
  // #EXP-1452: what the member's agent wrote into a chat, the member read.
  if (from?.asSession) {
    const [sent] = await tx
      .select({ rootId: sessions.rootId, at: messages.createdAt })
      .from(messages)
      .innerJoin(sessions, eq(sessions.id, messages.authorSessionId))
      .where(eq(messages.id, messageId))
      .limit(1)
    if (sent && sent.rootId !== session.rootId) {
      await markTreesReadBy(tx, authorId, session.teamId, [session.rootId], sent.at)
    }
  }
  await claimAttachments(tx, {
    ids: input.attachmentIds ?? [],
    messageId,
    teamId: session.teamId,
    uploaderIds,
    body: text,
  })
  // #EXP-1584: the forwarded issues ride in the clear beside the body.
  if (issueBrief) await tx.update(messages).set({ issueBrief: issueBrief.brief }).where(eq(messages.id, messageId))
  return messageId
}

/** #EXP-1584 (fixtures/session-brief.json `issueBriefCases`): the Issues
 *  section of `issueIds` (input order) for a message forwarding them into
 *  a session already under way: a queued chat's brief lists its rows, a
 *  running or idle one would never see them. #EXP-1686: an agent's
 *  conversation holds no rows, so its message always carries the section. */
async function forwardedIssueBrief(
  tx: Tx,
  session: Pick<Session, `status` | `agentId`>,
  issueIds: string[] | undefined
): Promise<{ brief: string; line: string | null } | null> {
  if (!issueIds?.length || (session.status === `queued` && linksIssues(session))) return null
  const rows = await tx
    .select({ id: issues.id, identifier: issues.identifier, title: issues.title, description: issues.description })
    .from(issues)
    .where(inArray(issues.id, issueIds))
  const byId = new Map(rows.map((row) => [row.id, row]))
  const ordered = [...new Set(issueIds)].flatMap((id) => {
    const row = byId.get(id)
    return row ? [row] : []
  })
  const brief = composeIssueBrief(ordered)
  if (!brief) return null
  const identifiers = ordered.map((row) => row.identifier)
  return { brief, line: linksIssues(session) ? manualLinkText(identifiers) : agentForwardLine(identifiers) }
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
  input: Pick<SessionsSendInput, `cli` | `model` | `effort` | `ultracode` | `subagentModel`>
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
  if (input.subagentModel && (cli !== `claude` || !MODELS.claude!.includes(input.subagentModel))) {
    throw bad(`Unknown subagent model "${input.subagentModel}"`)
  }
}

type RunnerSource = Pick<
  Session,
  `cli` | `account` | `model` | `effort` | `subagentModel` | `computerUse` | `ultracode`
>

/** The runner pickers of a new session: the input's, else inherited from
 *  `from` (a child from its parent, a continuation from the ended row). A
 *  blank model/effort (`""`, an old client's "CLI default") is NULL: it is
 *  neither stored nor inherited. */
function runnerFields(input: Partial<SessionsSendInput>, from?: RunnerSource | null) {
  return {
    cli: input.cli ?? from?.cli ?? null,
    account: input.account ?? from?.account ?? null,
    model: input.model?.trim() || from?.model || null,
    effort: input.effort?.trim() || from?.effort || null,
    subagentModel: input.subagentModel?.trim() || from?.subagentModel || null,
    computerUse: input.computerUse ?? from?.computerUse ?? false,
    ultracode: input.ultracode ?? from?.ultracode ?? false,
  }
}

/** EXP-1376: the runner picks a message (or a configure) names for an
 *  EXISTING session; omitted = unchanged. A blank model is a stale client's
 *  "CLI default" (ignored), a blank effort the agent's default (NULL). The
 *  login (`account`) is the switch, handled beside. */
export interface RunnerPicks {
  model?: string
  effort?: string | null
  ultracode?: boolean
  computerUse?: boolean
}

export function runnerPicks(input: {
  model?: string | null
  effort?: string | null
  ultracode?: boolean
  computerUse?: boolean
}): RunnerPicks {
  const model = input.model?.trim()
  return {
    ...(model ? { model } : {}),
    ...(input.effort !== undefined ? { effort: input.effort?.trim() || null } : {}),
    ...(input.ultracode !== undefined ? { ultracode: input.ultracode } : {}),
    ...(input.computerUse !== undefined ? { computerUse: input.computerUse } : {}),
  }
}

/** EXP-1376 (FEED-152): a model or login pick on a session behind a usage
 *  wall must lift it, never succeed silently into the same wall. A login
 *  switch is the host's to lift (it clears the wall as the run moves and
 *  puts it back on a refused move); a new model lifts a MODEL wall, which
 *  the caller then clears at once (`true`) so the queued messages run.
 *  Anything else naming a model refuses, saying what would lift it. */
export function modelLiftsWall(
  target: Pick<Session, `blocked` | `model`>,
  change: { switching: boolean; model?: string; hasMessage?: boolean }
): boolean {
  const wall = target.blocked
  if (wall?.kind !== `rate_limit` || change.switching || change.model === undefined) return false
  if (wall.window === `model` && change.model !== target.model) return true
  // A message naming the model it already runs on is no pick: it queues
  // behind the wall like any message.
  if (change.hasMessage && change.model === target.model) return false
  const until = wall.resetsAt ? ` until ${wall.resetsAt}` : ``
  throw new TRPCError({
    code: `BAD_REQUEST`,
    message:
      wall.window === `model`
        ? `This session is blocked by the ${change.model} usage limit${until}: pick another model, or pass account to switch logins.`
        : `This session is blocked by its login's ${wall.window ?? `usage`} limit${until}, which a model change does not lift: pass account to switch logins.`,
  })
}

/** EXP-1376 (review): the row's wall and model under its lock, then the
 *  verdict on them; a lifted MODEL wall is cleared only while it is still a
 *  model wall (a session/weekly wall the device reported in between stays). */
export async function liftWallInTx(
  tx: Tx,
  sessionId: string,
  change: { switching: boolean; model?: string; hasMessage?: boolean }
): Promise<void> {
  if (change.model === undefined || change.switching) return
  const [fresh] = await tx
    .select({ blocked: sessions.blocked, model: sessions.model })
    .from(sessions)
    .where(eq(sessions.id, sessionId))
    .for(`update`)
  if (fresh && modelLiftsWall(fresh, change)) {
    await clearBlockedInTx(tx, [sessionId], `rate_limit`, `model`)
  }
}

/** EXP-1376 (review): an account switch names a login signed in for the
 *  session's agent on its device. A device that reported no logins at all
 *  is unknowable here: its own switch check decides (and replies). */
async function assertSessionAccount(target: Session, account: string, cli: string | null) {
  const row = await loadDeviceRow(target.deviceId, await sessionDeviceOwner(target))
  if (!row?.agentAccounts || Object.keys(row.agentAccounts).length === 0) return
  const agent = cli ?? `claude`
  if (!flattenAccounts(row).some((option) => option.agent === agent && option.id === account)) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Account "${account}" is not signed in for ${agent} on this session's device`,
    })
  }
}

/** EXP-1376: an agent's composer `/model <alias>` over MCP is the model
 *  pick itself (what a person's ModelPicker asks for), never a turn: a
 *  walled session takes none, so the line would wait forever. EXP-1401: a
 *  PERSON's `/model <alias>` (any client: web, desktop, iOS, Android) is the
 *  same pick, with the same wall verdict (`liftWallInTx`). A person's bare
 *  `/model` stays a message (their CLI's own menu); an agent's is refused. */
const MODEL_COMMAND = /^\/model(?:\s+(\S+))?\s*$/

// EXP-1458: a sealed text never matches (its prefix): a sealed composer
// sends the pick as the clear `model` param instead.
export function modelCommand(input: SessionsSendInput, viaMcp: boolean): SessionsSendInput {
  const match = input.attachmentIds?.length ? null : MODEL_COMMAND.exec(input.text?.trim() ?? ``)
  if (!match) return input
  if (!match[1] && !input.model) {
    if (!viaMcp) return input
    throw new TRPCError({ code: `BAD_REQUEST`, message: `/model needs a model: pass model, e.g. "/model opus"` })
  }
  return { ...input, text: undefined, model: input.model || match[1] }
}

/** Refuses a child the follow-up caps forbid: depth below the root, and
 *  the tree's LIVE sessions (an ended branch frees its slot). Holds the
 *  root's row lock so two concurrent starts cannot both take the last slot.
 *  #EXP-1731: a CLI subagent row holds no slot (subagents.json; its own
 *  insert, `lib/sessions/subagent.ts`, never comes through here). */
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
    .where(and(eq(sessions.rootId, rootId), ne(sessions.status, `ended`), isNull(sessions.subagentKey)))
  if (Number(live) >= SESSION_TREE_MAX_SESSIONS) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `A chat runs at most ${SESSION_TREE_MAX_SESSIONS} sessions at once`,
    })
  }
}

/** VAPP-229: the app this send runs for (the apps broker's server-side
 *  context, `sendSession({viaAppId})`), scoped to that one call: every
 *  session row it inserts (a root, a child, an agent's fresh chat) reads
 *  "via <app>" (via-app.json). */
const sendViaApp = new AsyncLocalStorage<string | undefined>()

async function insertSession(tx: Tx, values: SessionInsert & { id: string }): Promise<Session> {
  const viaAppId = sendViaApp.getStore()
  const [row] = await tx.insert(sessions).values(viaAppId ? { ...values, viaAppId } : values).returning()
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

/** #FEED-173/#FEED-174: a send naming no session (sessionId, agentId,
 *  appId, parentId) and carrying no work for a new chat (text,
 *  attachments, a prompt, issues). Spec copy: packages/eagent rules
 *  `copy.startNeedsWork`. */
export const START_NEEDS_WORK = `Name the session to message (sessionId, agentId, appId or parentId), or give the new session a text or issues; wakeOn alone watches from your own session`

function startsNothing(input: SessionsSendInput): boolean {
  if (input.sessionId || input.agentId || input.appId || input.parentId) return false
  return !input.text?.trim() && !input.attachmentIds?.length && !input.promptId && !input.issueIds?.length
}

/** #FEED-173/#FEED-174: `{wakeOn}` with no target and nothing to start
 *  registers the wake on the CALLING session (the core's `send.watch`);
 *  without `wakeOn` such a send is refused. Never an empty chat. */
async function watchOnly(caller: Session | null, input: SessionsSendInput): Promise<SendOutcome> {
  // Anything beside the watch (a name, a seal, a device, runner picks) is a
  // start without work, never a watch: a sealing daemon's fresh `seal` must
  // not come back adopted under the CALLER's id.
  const extra = Object.entries(input).some(([key, value]) => value !== undefined && key !== `wakeOn` && key !== `teamId`)
  if (!input.wakeOn?.length || extra) throw new TRPCError({ code: `BAD_REQUEST`, message: START_NEEDS_WORK })
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    await applyWakeOn(tx, caller, input)
    return { sessionId: caller!.id, messageId: null, status: caller!.status, txid, queued: null }
  })
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
  /** NEW-57: a meeting transcript batch from the voice device (tRPC only,
   *  never on the MCP tool): its message expires after
   *  `contract.meeting.retentionDays` (`messages.expires_at`). */
  meeting?: boolean
  /** F21: the sender's idempotency key (tRPC only): a send naming a key
   *  its sender already posted returns that message's result, posting and
   *  starting nothing (a retried meeting flush after a timeout). Stamped
   *  on a message into a session, agent or app and a child's first one; a
   *  new root's start carries no key. */
  clientMessageId?: string
  /** VAPP-229: an app's exposed tool drives this call (tRPC ctx.viaAppId,
   *  never input): the sessions it starts read "via <app>". */
  viaAppId?: string
}): Promise<SessionsSendResult & { txid: number }> {
  return sendViaApp.run(args.viaAppId, () => sendSessionInner(args))
}

async function sendSessionInner(args: Parameters<typeof sendSession>[0]): Promise<SessionsSendResult & { txid: number }> {
  const caller = await loadCallerSession(args.userId, args.callerSessionId)
  const starter = effectiveStarter(args.userId, caller)
  const uploaderIds = [...new Set([args.userId, starter])]
  const { input } = args
  if (!input.sessionId && !input.agentId) assertRunnerVocabulary(input)
  await assertAgentTreeTeam(input, caller)
  if (args.meeting) await assertMeetingSend(args.userId, input, args.viaMcp === true)

  const clientMessageId = args.viaMcp ? undefined : args.clientMessageId
  if (clientMessageId) {
    const sent = await clientMessageResult(uploaderIds, clientMessageId)
    if (sent) return sent
  }
  let extras = args.meeting ? withMeetingExpiry(args.extras ?? {}) : (args.extras ?? {})
  if (clientMessageId) extras = withClientMessageId(extras, clientMessageId)
  let outcome: SendOutcome
  try {
    if (input.sessionId) {
      outcome = await messageSession(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
    } else if (input.agentId) {
      outcome = await messageAgent(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
    } else if (input.appId) {
      outcome = await messageApp(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
    } else if (input.parentId) {
      outcome = await startChild(starter, uploaderIds, input, caller, extras, args.viaMcp === true)
    } else if (startsNothing(input)) {
      outcome = await watchOnly(caller, input)
    } else {
      outcome = await startRoot(starter, uploaderIds, input, caller, args.viaMcp === true)
    }
  } catch (err) {
    // #FEED-184: a plaintext answer to a sealed card names its writer.
    if (err instanceof TRPCError && err.message === SEALED_CARD_PLAIN_ANSWER_MESSAGE) {
      const hint = await plainWriterHint(db, caller?.id)
      throw new TRPCError({ code: err.code, message: `${SEALED_CARD_PLAIN_ANSWER_MESSAGE}${hint}` })
    }
    // A concurrent retry committed the same key first: its result.
    if (clientMessageId && uniqueViolationConstraint(err) === CLIENT_MESSAGE_ID_UNIQUE) {
      const sent = await clientMessageResult(uploaderIds, clientMessageId)
      if (sent) return sent
    }
    throw err
  }

  if (outcome.endedIds?.length) await relayKillSessionsBestEffort(outcome.endedIds)
  if (outcome.queued) {
    const config = getSteerRelayConfig()
    if (config) {
      void relayPostNudge(config, outcome.queued.ownerId, outcome.queued.deviceId)
    }
  }
  if (outcome.messageId) fireAndForgetSessionMessageNotify({ messageId: outcome.messageId })
  // EXP-1375: a start on a device low on disk says so.
  const disk = outcome.queued ? await startDiskWarning(outcome.queued) : null
  const warning = [outcome.warning, disk].filter(Boolean).join(` `)
  return {
    sessionId: outcome.sessionId,
    messageId: outcome.messageId,
    status: outcome.status,
    txid: outcome.txid,
    ...(warning ? { warning } : {}),
    deliverTo: await deliverTo(outcome.sessionId),
  }
}

/** EXP-1458: the live install of the device executing `sessionId` (the
 *  host's devices row for its machine), most recently seen first. */
export async function deliverTo(sessionId: string): Promise<{ installId: string; endpointId: string } | null> {
  const [session] = await db
    .select({ deviceId: sessions.deviceId, userId: sessions.userId, teamId: sessions.teamId })
    .from(sessions)
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!session?.deviceId) return null
  const host = await resolveSessionHost({ deviceId: session.deviceId, userId: session.userId, teamId: session.teamId })
  if (!host) return null
  const [install] = await db
    .select({ installId: installs.id, endpointId: installs.endpointId })
    .from(installs)
    .innerJoin(devices, eq(devices.id, installs.deviceId))
    .where(and(eq(devices.userId, host), eq(devices.deviceId, session.deviceId), isNull(installs.revokedAt)))
    .orderBy(sql`${installs.lastSeenAt} DESC NULLS LAST`)
    .limit(1)
  return install ?? null
}

export const MEETING_SEND_REFUSED = `A meeting transcript goes to your voice agent's conversation only`

/** NEW-57 (meeting.json RESIDENCY): `meeting: true` = a transcript batch
 *  into the CALLER's own conversation with THEIR voice agent
 *  (`user_voice_settings.agent_id`): `agentId` = that agent, or `sessionId`
 *  = that conversation session. Text only (nothing else rides a batch);
 *  never over MCP; never while that agent sits on a shared or foreign
 *  device or the conversation is `shared` (`meetingAgentIsPrivate`). */
async function assertMeetingSend(userId: string, input: SessionsSendInput, viaMcp: boolean): Promise<void> {
  const refuse = (): never => {
    throw new TRPCError({ code: `BAD_REQUEST`, message: MEETING_SEND_REFUSED })
  }
  if (viaMcp || !input.text?.trim()) refuse()
  const allowed = new Set([`teamId`, `sessionId`, `agentId`, `text`])
  if (Object.entries(input).some(([key, value]) => value !== undefined && !allowed.has(key))) refuse()
  const [settings] = await db
    .select({ agentId: userVoiceSettings.agentId })
    .from(userVoiceSettings)
    .where(eq(userVoiceSettings.userId, userId))
    .limit(1)
  const voiceAgentId = settings?.agentId ?? null
  if (!voiceAgentId) refuse()
  if (input.agentId) {
    if (input.agentId !== voiceAgentId) refuse()
  } else {
    if (!input.sessionId) refuse()
    const [target] = await db
      .select({ agentId: sessions.agentId, userId: sessions.userId, parentId: sessions.parentId })
      .from(sessions)
      .where(eq(sessions.id, input.sessionId!))
      .limit(1)
    if (!target || target.agentId !== voiceAgentId || target.userId !== userId || target.parentId !== null) refuse()
  }
  // Never into a team-visible conversation or onto a shared machine.
  if (!(await meetingAgentIsPrivate(db, userId, voiceAgentId!))) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: MEETING_SHARED_REFUSED })
  }
}

const CLIENT_MESSAGE_ID_UNIQUE = `uq_messages_client_message_id`

/** F21: the message the sender already posted under `clientMessageId`, as
 *  the send's result (a fresh txid: nothing new to wait for). */
async function clientMessageResult(
  authorIds: string[],
  clientMessageId: string
): Promise<(SessionsSendResult & { txid: number }) | null> {
  const [row] = await db
    .select({ messageId: messages.id, sessionId: sessions.id, status: sessions.status })
    .from(messages)
    .innerJoin(conversations, eq(conversations.id, messages.conversationId))
    .innerJoin(sessions, eq(sessions.id, conversations.sessionId))
    .where(and(eq(messages.clientMessageId, clientMessageId), inArray(messages.authorId, authorIds)))
    .limit(1)
  if (!row) return null
  const txid = await db.transaction((tx) => generateTxId(tx))
  return { sessionId: row.sessionId, messageId: row.messageId, status: row.status, txid }
}

/** F21: stamp the send's message with the sender's idempotency key (in
 *  the send's transaction: a second insert under the key rolls it back). */
function withClientMessageId(extras: SendExtras, clientMessageId: string): SendExtras {
  return {
    ...extras,
    within: async (tx, result) => {
      await extras.within?.(tx, result)
      if (!result.messageId) return
      await tx.update(messages).set({ clientMessageId }).where(eq(messages.id, result.messageId))
    },
  }
}

/** NEW-57: stamp the send's message to expire with the meeting retention. */
function withMeetingExpiry(extras: SendExtras): SendExtras {
  return {
    ...extras,
    within: async (tx, result) => {
      await extras.within?.(tx, result)
      if (!result.messageId) return
      await tx.execute(sql`
        UPDATE messages
        SET expires_at = now() + make_interval(days => ${contract.meeting.retentionDays}::int)
        WHERE id = ${result.messageId}`)
    },
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
 *  current one; `direct` = the SAME row re-targets: a QUEUED session, or
 *  (#EXP-1737) a LIVE ROOT, so a tree keeps ONE worker and an editor its one
 *  root; `relocate` = a LIVE CHILD ends here and continues on the target
 *  (same branch, the message pending there). An ENDED session's `deviceId`
 *  = the device its continuation starts on (handled by the continuation
 *  itself). */
type MovePlan =
  | { kind: `none` }
  | { kind: `direct` | `relocate`; target: MoveTarget }

async function planMove(session: Session, input: SessionsSendInput, moverId: string): Promise<MovePlan> {
  const deviceId = input.deviceId
  if (deviceId === undefined || deviceId === session.deviceId) return { kind: `none` }
  if (session.status === `ended` && !isPersistent(session)) return { kind: `none` }
  assertMovable(session)
  const target = await resolveMoveTarget(session, deviceId, {
    account: input.account ?? null,
    requireOnline: true,
    moverId: session.status === `queued` ? moverId : undefined,
  })
  return { kind: session.status === `queued` || session.parentId === null ? `direct` : `relocate`, target }
}

/** The row changed under the plan (a claim or an end raced the send). */
function moveRaced(): never {
  throw new TRPCError({ code: `CONFLICT`, message: `The session just changed; try again.` })
}

/** A queued row re-targets (guarded on the device and status the plan was
 *  made for: a row the device claimed since is CONFLICT, never re-targeted).
 *  #EXP-1737: a LIVE root re-queues the same way, keeping its `startedAt`
 *  (the target launches it with its messages pending): its old device sees a
 *  row naming another device, releases the engine without writing to the row
 *  and hands the branch on (`session.moved`). */
async function applyDirectMove(tx: Tx, session: Session, target: MoveTarget) {
  // #EXP-1259: the plan read the target outside this transaction; the
  // share is re-read under its lock before the row names the box (a
  // visibility the plan adopted must still be the box's).
  const device = await resolveSessionDeviceInTx(tx, session.userId, session.teamId, target.deviceId)
  if (target.shared !== session.shared && device.shared !== target.shared) moveRaced()
  const from = deviceName(
    await loadDeviceRow(session.deviceId, await sessionDeviceOwner(session)),
    session.deviceId
  )
  const live = session.status !== `queued`
  const updated = await tx
    .update(sessions)
    .set({
      ...(live ? { status: `queued` as const, busy: false, background: false } : {}),
      deviceId: target.deviceId,
      account: target.account,
      // #FEED-177: a queued root takes its new box's sharing (`visibilityMove`).
      shared: target.shared,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(sessions.id, session.id),
        eq(sessions.deviceId, session.deviceId),
        live ? ne(sessions.status, `ended`) : eq(sessions.status, `queued`),
        // A sub-session started since the plan pins the tree's visibility.
        ...(target.shared === session.shared
          ? []
          : [sql`NOT EXISTS (SELECT 1 FROM sessions s WHERE s.root_id = ${session.id} AND s.id <> ${session.id})`])
      )
    )
    .returning({ id: sessions.id })
  if (updated.length === 0) moveRaced()
  await postMovedMessage(tx, session, movedText(from, target.name))
  if (live) {
    // A wall was the old device's; an open card still waits for its answer.
    await clearBlockedInTx(tx, [session.id])
    await recordMoveInTx(tx, {
      teamId: session.teamId,
      sessionId: session.id,
      continuationId: session.id,
      fromDeviceId: session.deviceId,
      toDeviceId: target.deviceId,
    })
  }
  if (target.shared !== session.shared) await restampRootVisibilityInTx(tx, session.id, target.shared)
}

/** #FEED-177: the visibility mirrors copied at insert time follow a queued
 *  root that adopted its new box's sharing (as an agent's move,
 *  trpc/agents.ts): its messages and issue links (conversations follow by
 *  trigger, re-stamped here too). Private → shared neutralises what only
 *  its starter could read (`hidePrivateLinesInTx`); shared → private drops
 *  the teammates' wraps of its key (`dropNonReaderKeysInTx`). */
async function restampRootVisibilityInTx(tx: Tx, rootId: string, shared: boolean) {
  const tree = tx.select({ id: sessions.id }).from(sessions).where(eq(sessions.rootId, rootId))
  await tx.update(messages).set({ shared }).where(eq(messages.rootId, rootId))
  await tx.update(conversations).set({ shared }).where(inArray(conversations.sessionId, tree))
  await tx.update(sessionIssues).set({ shared }).where(inArray(sessionIssues.sessionId, tree))
  if (shared) await hidePrivateLinesInTx(tx, [rootId])
  else await dropNonReaderKeysInTx(tx, [rootId])
}

/** #FEED-172 (packages/eagent/fixtures/reuse.json): the target's tree has
 *  the last word with the caller: its newest agent-authored line in the
 *  caller's conversation is newer than the caller's newest message into the
 *  target's tree. Such a send is a REPLY (the chat asked in a plain
 *  message), never new work, so it never forks. */
async function awaitsCaller(caller: Session, target: Session): Promise<boolean> {
  const result = await db.execute(sql`
    SELECT
      (SELECT max(m.created_at) FROM messages m
        WHERE m.conversation_id = ${sessionConversation(caller.id)} AND m.author_kind = 'agent'
          AND m.author_session_id IN (SELECT id FROM sessions WHERE root_id = ${target.rootId})) AS "from",
      (SELECT max(m.created_at) FROM messages m JOIN conversations c ON c.id = m.conversation_id
        WHERE m.author_session_id = ${caller.id}
          AND c.session_id IN (SELECT id FROM sessions WHERE root_id = ${target.rootId})) AS "to"
  `)
  const row = result.rows[0] as { from: string | Date | null; to: string | Date | null } | undefined
  const from = row?.from ? new Date(row.from).getTime() : null
  const to = row?.to ? new Date(row.to).getTime() : null
  return from !== null && (to === null || from > to)
}

/** S-3: an agent's message to a chat it may not reuse never fails: it
 *  starts a FRESH chat on the same device and repository with the same
 *  message (watched by the caller, `startRoot`), the result's `warning`
 *  naming why. */
async function agentFreshSession(
  userId: string,
  uploaderIds: string[],
  input: SessionsSendInput,
  caller: Session | null,
  target: Session,
  viaMcp: boolean
): Promise<SendOutcome | null> {
  if (!caller || caller.rootId === target.rootId) return null
  if (target.parentId !== null || target.agentId || target.appId || isPersistent(target)) return null
  if (input.answer || !(input.text?.trim() || input.attachmentIds?.length)) return null
  // EXP-1458: a text sealed to THIS tree could not move to a fresh one.
  if (isSealedBody(input.text)) return null
  // A chat waiting on an answer: the reply answers it, never forks.
  if (target.needsInput) return null
  if (!(await callerTreeAgentId(caller))) return null
  const [{ count }] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(sessions)
    // #EXP-1731: a CLI subagent is no sub-session (reuse.json maxChildren).
    .where(and(eq(sessions.rootId, target.id), ne(sessions.id, target.id), isNull(sessions.subagentKey)))
  const reason = agentReuseRefusal(target, count, new Date(), await awaitsCaller(caller, target))
  if (!reason) return null
  const fresh: SessionsSendInput = {
    ...input,
    sessionId: undefined,
    teamId: target.teamId,
    deviceId: input.deviceId ?? target.deviceId ?? undefined,
    repositoryId: target.repositoryId ?? undefined,
    cli: target.cli ?? undefined,
    model: input.model ?? target.model ?? undefined,
    effort: input.effort ?? target.effort ?? undefined,
    account: input.account ?? target.account ?? undefined,
  }
  // The send skipped the check (it named a session): the merged runner is
  // a new start's, so it is checked like one.
  assertRunnerVocabulary(fresh)
  const outcome = await startRoot(userId, uploaderIds, fresh, caller, viaMcp)
  return {
    ...outcome,
    warning: `Started a fresh session instead of reusing ${target.id}: ${reason}. You watch the new one.`,
  }
}

async function messageSession(
  userId: string,
  uploaderIds: string[],
  sent: SessionsSendInput,
  caller: Session | null,
  extras: SendExtras,
  viaMcp: boolean
): Promise<SendOutcome> {
  const target = await loadSessionForUser(userId, sent.sessionId!)
  if (target.subagentKey !== null && target.parentId) {
    // #EXP-1731: a subagent runs inside its parent's process and takes no
    // input of its own (a message mid-run would resume a second copy of
    // it): the send goes to its PARENT exactly as if sent there, warned.
    const out = await messageSession(userId, uploaderIds, { ...sent, sessionId: target.parentId }, caller, extras, viaMcp)
    const redirect = subagentRedirectText(target.id, target.parentId)
    return { ...out, warning: out.warning ? `${redirect} ${out.warning}` : redirect }
  }
  await assertCanSteer(userId, target)
  if (sent.teamId && sent.teamId !== target.teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The session is in another team` })
  }
  const input = modelCommand(sent, viaMcp)
  if (viaMcp) {
    const fresh = await agentFreshSession(userId, uploaderIds, input, caller, target, viaMcp)
    if (fresh) return fresh
  }
  // EXP-1376: the runner picks apply to the session (never dropped), in the
  // vocabulary of the agent it runs.
  const cli = await effectiveCli(target)
  assertRunnerVocabulary({
    cli: cli ?? undefined,
    model: input.model,
    effort: input.effort,
    ultracode: input.ultracode,
  })
  const picks = runnerPicks(input)

  const hasMessage = !!input.text?.trim() || !!input.attachmentIds?.length
  const move = await planMove(target, input, userId)
  // EXP-1323: another session's message over its MCP key is ITS input
  // (agent-authored, pending like a person's: the target wakes and reads
  // it). Over tRPC the header never speaks for a session: a person's
  // text stays a person row, the caller id only stamps it.
  const from = caller
    ? { sessionId: caller.id, asSession: viaMcp && caller.id !== target.id }
    : null

  if (move.kind === `relocate`) {
    // A live CHILD moves: it ends here (its children re-parent onto the
    // continuation, its watchers follow) and continues on the target.
    const outcome = await db.transaction(async (tx) => {
      const txid = await generateTxId(tx)
      const device = await resolveSessionDeviceInTx(tx, userId, target.teamId, move.target.deviceId)
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
        .set({ status: `ended`, endedAt: new Date(), busy: false, background: false, needsInput: false, blocked: null, updatedAt: new Date() })
        .where(eq(sessions.id, target.id))
        .returning({ id: sessions.id })
      await closeAskMirrorsOf(tx, [target.id])
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
      // EXP-1542: the source device hands the branch on (`session.moved`).
      await recordMoveInTx(tx, {
        teamId: target.teamId,
        sessionId: target.id,
        continuationId: result.sessionId,
        fromDeviceId: target.deviceId,
        toDeviceId: move.target.deviceId,
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
    if (switching && cli === `codex`) {
      throw new TRPCError({
        code: `BAD_REQUEST`,
        message: `Only claude can switch accounts during a session.`,
      })
    }
    if (switching) await assertSessionAccount(target, input.account!, cli)
    // EXP-1376: the pickers move WITH the login in one write (the host
    // resumes on both), a blocked session included.
    const wallChange = { switching, model: picks.model, hasMessage }
    return db.transaction(async (tx) => {
      const txid = await generateTxId(tx)
      // Under the row lock: the verdict reads the wall as it is now.
      await liftWallInTx(tx, target.id, wallChange)
      const mcpServerIds =
        input.mcpServerIds === undefined
          ? undefined
          : await assertMcpServersInTeam(tx, target.teamId, input.mcpServerIds)
      const skillIds =
        input.skillIds === undefined ? undefined : await assertSkillsInTeam(tx, target.teamId, input.skillIds)
      if (switching || Object.keys(picks).length > 0 || mcpServerIds !== undefined || skillIds !== undefined) {
        await tx
          .update(sessions)
          .set({
            ...(switching ? { account: input.account } : {}),
            ...picks,
            ...(mcpServerIds !== undefined ? { mcpServerIds } : {}),
            ...(skillIds !== undefined ? { skillIds } : {}),
            updatedAt: new Date(),
          })
          .where(eq(sessions.id, target.id))
      }
      let queued: SendOutcome[`queued`] = null
      let status = target.status
      if (move.kind === `direct`) {
        await applyDirectMove(tx, target, move.target)
        queued = { ownerId: move.target.ownerId, deviceId: move.target.deviceId }
        status = `queued`
      }
      // NEW-1: the answer to the target's open card (first answer wins).
      if (input.answer) await answerSessionCardInTx(tx, target, input.answer)
      // NEW-1: issues on a message = FORWARDED to that session (the rows
      // move onto it; it takes them). #EXP-1686: a conversation only reads them.
      if (input.issueIds?.length) {
        if (linksIssues(target)) {
          await forwardIssues(tx, {
            sessionId: target.id,
            issueIds: input.issueIds,
            source: caller ? `agent` : `user`,
            addedBy: userId,
            addedBySessionId: caller?.id ?? null,
          })
        } else {
          await assertIssuesInTeam(tx, target.teamId, input.issueIds)
        }
      }
      const issueBrief = await forwardedIssueBrief(tx, target, input.issueIds)
      const messageId = await postSendMessage(tx, target, input, userId, uploaderIds, from, issueBrief)
      // A PERSON's message into a worker = it takes its issues again.
      if (messageId && !from?.asSession) await retakeHeldIssues(tx, target, userId)
      await applyWakeOn(tx, caller, input)
      await extras.within?.(tx, { sessionId: target.id, messageId, rootId: target.rootId })
      return { sessionId: target.id, messageId, status, txid, queued }
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

  if (target.parentId === null) {
    // FEED-134: an ended chat ROOT revives in place (the conversation IS
    // the chat, so it never forks into a child). A named device is where
    // it resumes: nothing runs the row, so it simply re-targets.
    const moveTarget =
      input.deviceId !== undefined && input.deviceId !== target.deviceId
        ? await resolveMoveTarget(target, input.deviceId, {
            account: input.account ?? null,
            requireOnline: true,
          })
        : null
    const deviceId = moveTarget?.deviceId ?? target.deviceId
    if (!moveTarget && input.account !== undefined && input.account !== target.account) {
      await assertSessionAccount(target, input.account, cli)
    }
    return db.transaction(async (tx) => {
      const txid = await generateTxId(tx)
      const device = await resolveSessionDeviceInTx(tx, userId, target.teamId, deviceId)
      const result = await reviveRoot(tx, {
        root: target,
        userId,
        uploaderIds,
        input,
        caller,
        from,
        extras,
        device: { deviceId, ownerId: device.ownerId },
        moveTarget,
      })
      await applyWakeOn(tx, caller, input)
      return { ...result, txid }
    })
  }

  // An ended CHILD continues: a new session on its device (or the named
  // one), same pickers and branch, its first message = this one.
  const deviceId = input.deviceId ?? target.deviceId
  if (deviceId === target.deviceId && input.account !== undefined && input.account !== target.account) {
    await assertSessionAccount(target, input.account, cli)
  }
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const device = await resolveSessionDeviceInTx(tx, userId, target.teamId, deviceId)
    const result = await continueSession(tx, {
      ended: target,
      userId,
      uploaderIds,
      input,
      caller,
      from,
      extras,
      device: { deviceId, ownerId: device.ownerId },
      adopt: false,
    })
    // EXP-1542: continued on ANOTHER device: its old one hands on what it
    // parked when it ended.
    if (deviceId !== target.deviceId) {
      await recordMoveInTx(tx, {
        teamId: target.teamId,
        sessionId: target.id,
        continuationId: result.sessionId,
        fromDeviceId: target.deviceId,
        toDeviceId: deviceId,
      })
    }
    await applyWakeOn(tx, caller, input)
    return { ...result, txid }
  })
}

/**
 * NEW-2: a message to an AGENT: the caller's own conversation session with
 * it (found, or created `queued` on the agent's device: own or shared with
 * the team). The message is the conversation's pending input (agents never
 * end: the row resumes); #EXP-1686: issues on it ride on the message, never
 * as rows.
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
  // #EXP-1714: the member's Assistant answers from every team (clients send
  // the team they are in; the global one lives in their personal team).
  if (input.teamId && input.teamId !== agent.teamId && agent.role !== `assistant`) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The agent is in another team` })
  }
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const { session, created } = await conversationInTx(tx, agent, userId)
    // #VAPP-236: a conversation an app's hand-off starts reads "via <app>".
    const viaAppId = sendViaApp.getStore()
    if (created && viaAppId) await tx.update(sessions).set({ viaAppId }).where(eq(sessions.id, session.id))
    const from = caller
      ? { sessionId: caller.id, asSession: viaMcp && caller.id !== session.id }
      : null
    if (input.issueIds?.length) await assertIssuesInTeam(tx, agent.teamId, input.issueIds)
    const issueBrief = await forwardedIssueBrief(tx, session, input.issueIds)
    const messageId = await postSendMessage(tx, session, input, userId, uploaderIds, from, issueBrief)
    await applyWakeOn(tx, caller, input)
    await extras.within?.(tx, { sessionId: session.id, messageId, rootId: session.id })
    return { sessionId: session.id, messageId, status: session.status, txid, queued: created }
  })
}

/**
 * VAPP-110: a message to an APP = into the caller's own EDITOR SESSION with
 * it. None yet: a fresh root `queued` on `deviceId` (else the caller's
 * default device), its first message the app brief + the text. Live or
 * idle: the message resumes it. ENDED (the editor was left): the SAME root
 * revives (`messageSession` → `reviveRoot`, FEED-134), keeping its context;
 * the brief rides again so it sees the current head version.
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
    // EXP-1458: a sealed text is the writer's whole message (it composed
    // the brief in).
    const text = existing.status === `ended` && !isSealedBody(input.text) ? briefedText(app, input.text) : input.text
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
    const text = isSealedBody(input.text) ? input.text : briefedText(app, input.text)
    const messageId = await postSendMessage(tx, session, { ...input, text }, userId, uploaderIds, from)
    await extras.within?.(tx, { sessionId: session.id, messageId, rootId: session.id })
    return { sessionId: session.id, messageId, status: session.status, txid, queued: created }
  })
}

/**
 * The continuation of an ENDED session: a new session on `device` with the
 * ended row's pickers, branch (the device reuses its worktree) and title,
 * taking over its issue rows, as its SIBLING (#FEED-84): same parent and
 * depth. `adopt` (a move): the ended row's live children re-parent onto it
 * and its watchers follow.
 * FEED-134: an ended chat ROOT revives: the SAME row goes `idle` again
 * (`endedAt` cleared, not busy, nothing flagged), keeping its `startedAt`,
 * title, branch, pickers and issue rows, with this message pending on it,
 * so its device resumes it from its run record (the agent keeps its context)
 * and the chat keeps ONE worker (`liveSessionId` stays the root). A
 * `moveTarget` re-targets the row first (the queued-row rule: no engine
 * runs it, so it moves and says so); a bare `account` switches its login.
 * A concurrent revive wins the row: the loser is CONFLICT (`moveRaced`).
 */
async function reviveRoot(
  tx: Tx,
  args: {
    root: Session
    userId: string
    uploaderIds: string[]
    input: SessionsSendInput
    caller: Session | null
    from: { sessionId: string; asSession: boolean } | null
    extras: SendExtras
    device: { deviceId: string; ownerId: string }
    moveTarget: MoveTarget | null
  }
): Promise<Omit<SendOutcome, `txid`>> {
  const { root, userId, input, caller, moveTarget } = args
  const now = new Date()
  const [revived] = await tx
    .update(sessions)
    .set({
      status: `idle`,
      endedAt: null,
      busy: false,
      background: false,
      needsInput: false,
      blocked: null,
      deviceId: args.device.deviceId,
      account: moveTarget ? moveTarget.account : (input.account ?? root.account),
      ...runnerPicks(input),
      ...(input.mcpServerIds !== undefined
        ? { mcpServerIds: await assertMcpServersInTeam(tx, root.teamId, input.mcpServerIds) }
        : {}),
      ...(input.skillIds !== undefined
        ? { skillIds: await assertSkillsInTeam(tx, root.teamId, input.skillIds) }
        : {}),
      updatedAt: now,
    })
    .where(and(eq(sessions.id, root.id), eq(sessions.status, `ended`)))
    .returning()
  if (!revived) moveRaced()
  // #FEED-206: its starter (and the caller) hear it again.
  await rewatchRevived(tx, revived, caller)
  if (moveTarget) {
    const fromName = deviceName(
      await loadDeviceRow(root.deviceId, await sessionDeviceOwner(root)),
      root.deviceId
    )
    await postMovedMessage(tx, revived, movedText(fromName, moveTarget.name))
    // EXP-1542: its old device still holds the worktree: it hands it on.
    await recordMoveInTx(tx, {
      teamId: root.teamId,
      sessionId: root.id,
      continuationId: root.id,
      fromDeviceId: root.deviceId,
      toDeviceId: revived.deviceId,
    })
  }
  if (input.issueIds?.length) {
    await forwardIssues(tx, {
      sessionId: revived.id,
      issueIds: input.issueIds,
      source: caller ? `agent` : `user`,
      addedBy: userId,
      addedBySessionId: caller?.id ?? null,
    })
  }
  // #EXP-1584: a revived root resumes its run: no brief carries them.
  const issueBrief = await forwardedIssueBrief(tx, revived, input.issueIds)
  const messageId = await postSendMessage(tx, revived, input, userId, args.uploaderIds, args.from, issueBrief)
  // A PERSON's message into a worker = it takes its issues again.
  if (messageId && !args.from?.asSession) await retakeHeldIssues(tx, revived, userId)
  await args.extras.within?.(tx, { sessionId: revived.id, messageId, rootId: revived.id })
  return {
    sessionId: revived.id,
    messageId,
    status: `idle`,
    queued: { ownerId: args.device.ownerId, deviceId: revived.deviceId },
  }
}

/**
 * The continuation of an ENDED CHILD (#FEED-84): a new session on `device`
 * with the ended row's pickers, branch (the device reuses its worktree) and
 * title, taking over its issue rows, as its SIBLING: same parent and depth.
 * `adopt` (a live child's MOVE): its live children re-parent onto it and its
 * watchers follow. A ROOT never gets here: it revives (`reviveRoot`) or moves
 * in place (`applyDirectMove`), so a tree keeps ONE worker.
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
  const depth = ended.depth || 1
  await assertTreeRoom(tx, ended.rootId, depth)
  const [root] = await tx.select().from(sessions).where(eq(sessions.id, ended.rootId)).limit(1)
  if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  const parentId = ended.parentId!
  const id = crypto.randomUUID()
  const child = await insertSession(tx, {
    id,
    teamId: root.teamId,
    parentId,
    rootId: root.id,
    depth,
    // #EXP-1402: the lineage a logical session walks (Guide + result ownership).
    continuedFromId: ended.id,
    branch: ended.branch,
    title: ended.title,
    sealedTitle: ended.sealedTitle,
    userId,
    deviceId: args.device.deviceId,
    // EXP-1376: the message's pickers win over the ended row's (its agent
    // stays).
    ...runnerFields({ ...input, cli: undefined }, ended),
    mcpServerIds:
      input.mcpServerIds !== undefined
        ? await assertMcpServersInTeam(tx, root.teamId, input.mcpServerIds)
        : ended.mcpServerIds,
    skillIds:
      input.skillIds !== undefined
        ? await assertSkillsInTeam(tx, root.teamId, input.skillIds)
        : ended.skillIds,
    repositoryId: ended.repositoryId ?? root.repositoryId,
    status: `queued`,
    shared: root.shared,
  })
  await takeOverIssues(tx, ended, child, userId)
  if (args.adopt) {
    await tx
      .update(sessions)
      .set({ parentId: child.id, updatedAt: new Date() })
      .where(and(eq(sessions.parentId, ended.id), ne(sessions.status, `ended`)))
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
  await postSystemCard(tx, parentId, { kind: `session`, sessionId: child.id })
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
  extras: SendExtras,
  viaMcp: boolean
): Promise<SendOutcome> {
  const parent = await loadSessionForUser(userId, input.parentId!)
  // #EXP-1731: the CLI's subagents attach to the session that runs the CLI.
  if (parent.subagentKey !== null) throw new TRPCError({ code: `BAD_REQUEST`, message: SUBAGENT_NO_CHILDREN })
  await assertCanSteer(userId, parent)
  const teamId = await childTeam(input, parent)
  await assertTeamMember(userId, teamId)
  const deviceId = input.deviceId ?? parent.deviceId
  // #FEED-182: profile ids are device-local — a child on ANOTHER device gets
  // a login of THAT device (the named one, or the parent's by email).
  const crossDevice = deviceId !== parent.deviceId
  const childAccount = crossDevice
    ? await childAccountOnDevice(
        parent,
        input.cli ?? (await effectiveCli(parent)) ?? `claude`,
        { deviceId, ownerId: (await resolveSessionDevice(userId, teamId, deviceId)).ownerId },
        input.account
      )
    : undefined

  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const device = await resolveSessionDeviceInTx(tx, userId, teamId, deviceId)
    await assertTreeRoom(tx, parent.rootId, parent.depth + 1)
    const [root] = await tx.select().from(sessions).where(eq(sessions.id, parent.rootId)).limit(1)
    if (!root) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
    // A private tree's child runs on the caller's OWN device only: on a
    // teammate's shared box its daemon would host prompts that account
    // cannot read (the tree is private to the caller).
    if (!root.shared && device.ownerId !== userId) {
      throw new TRPCError({ code: `BAD_REQUEST`, message: `A private session's child runs on your own device` })
    }
    // #EXP-1720: a child in another team than its parent takes nothing team-bound from it.
    const sameTeam = teamId === parent.teamId
    const mcpServerIds =
      (await assertMcpServersInTeam(tx, teamId, input.mcpServerIds)) ?? (sameTeam ? parent.mcpServerIds : null)
    // #EXP-1656: a child without its own pick inherits its parent's skills.
    const skillIds = (await assertSkillsInTeam(tx, teamId, input.skillIds)) ?? (sameTeam ? parent.skillIds : null)
    const id = crypto.randomUUID()
    const child = await insertSession(tx, {
      id,
      teamId,
      parentId: parent.id,
      rootId: parent.rootId,
      depth: parent.depth + 1,
      userId,
      deviceId,
      ...runnerFields(input, parent),
      ...(crossDevice ? { account: childAccount } : {}),
      // EXP-1341: the parent's `name`, else a suggestion's title; omitted =
      // the session names itself (session-title.json).
      ...((input.name ?? extras.title)?.trim()
        ? { title: (input.name ?? extras.title)!.trim().slice(0, 255) }
        : {}),
      ...(input.sealedName ? { sealedTitle: input.sealedName } : {}),
      mcpServerIds,
      skillIds,
      repositoryId: sameTeam ? (parent.repositoryId ?? root.repositoryId) : null,
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
      // #EXP-1452: an agent's start is ITS message (its avatar on the brief).
      caller ? { sessionId: caller.id, asSession: viaMcp } : null
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
  caller: Session | null,
  viaMcp: boolean
): Promise<SendOutcome> {
  const teamId = input.teamId ?? caller?.teamId
  if (!teamId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `teamId is required for a new session` })
  }
  await assertTeamMember(userId, teamId)

  // An agent's spawn runs on the agent's own device unless it names one:
  // the caller's root is a conversation session (its device = the agent's).
  // #EXP-1720: also across teams (the Assistant starts chats in its scope teams).
  const callerRoot =
    caller
      ? caller.rootId === caller.id
        ? caller
        : ((await db.select().from(sessions).where(eq(sessions.id, caller.rootId)).limit(1))[0] ?? null)
      : null
  const deviceId = input.deviceId ?? (callerRoot && treeAgentId(callerRoot) ? callerRoot.deviceId : undefined)
  // #EXP-1383: a root an agent tree starts stays that agent's tree (its
  // team boundary, its actor stamps, its team key on the device).
  const startedByAgentId = await callerTreeAgentId(caller)
  if (!deviceId) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Pick a device for the new session` })
  }
  return db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    const device = await resolveSessionDeviceInTx(tx, userId, teamId, deviceId)
    if (input.issueIds) await assertIssuesInTeam(tx, teamId, input.issueIds)
    if (input.repositoryId) await assertRepositoryInTeam(tx, teamId, input.repositoryId)
    const mcpServerIds = await assertMcpServersInTeam(tx, teamId, input.mcpServerIds)
    // #EXP-1656: the pick, else (a root an agent tree starts) the agent's.
    const skillIds =
      (await assertSkillsInTeam(tx, teamId, input.skillIds)) ??
      (startedByAgentId ? await agentSkillIds(tx, startedByAgentId) : null)

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
    // EXP-1458: a sealed text is the writer's whole first message (it
    // composed the prompt's body in); the prompt still names the session.
    const text =
      prompt && !isSealedBody(input.text)
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
      userId,
      deviceId,
      startedByAgentId,
      // NEW-1: the root WORKS: the composer's picks are its own.
      ...runnerFields(input),
      mcpServerIds,
      skillIds,
      repositoryId,
      status: `queued`,
      shared: device.shared,
      ...(title ? { title: title.slice(0, 255) } : {}),
      ...(input.sealedName ? { sealedTitle: input.sealedName } : {}),
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
    // #EXP-1452: a root an agent starts opens with ITS message.
    const messageId = await postSendMessage(
      tx,
      root,
      { ...input, text },
      userId,
      uploaderIds,
      caller && caller.teamId === teamId ? { sessionId: caller.id, asSession: viaMcp } : null
    )
    if (caller && caller.teamId === teamId) {
      // NEW-1: a root started from inside a session is WATCHED by its
      // starter: the bus delivers its transitions there (an agent's spawned
      // chats report back into its conversation). The card names it.
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
