// @vitest-environment node
// #EXP-1440 (agents strictly scoped to their team): against the REAL migrated
// Postgres, through the noun router. A member U owns teams A and B; an MCP
// caller speaking for U's conversation with an agent of team A (an
// agent-tree session: `confineMcpAccessToActingSession`) calls EVERY op of
// every noun, and both verbs, with team B's ids. Each call errors, or (a
// read) answers nothing of B: no marker, no id of B's rows; a write never
// changes a row of B. A new op without a case fails the meta-test. Plus the
// device rule: one of U's own machines shared only with B neither lists nor
// runs (or receives) a session of A, for an agent caller and a person alike.
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import {
  agents,
  appRecords,
  apps,
  attachments,
  comments,
  devices,
  events,
  issueLabels,
  issueRelations,
  issueStatuses,
  issues,
  labels,
  mcpServers,
  notifications,
  prompts,
  pullRequests,
  sessions,
  teamInvites,
  teamMembers,
  teams,
} from "@/db/schema"
import { probeRealDb } from "@/test/real-db"
import { createTestBoard, ensureTestRepository } from "@/test/factories"
import type { McpUser } from "./server"

vi.mock(`@/lib/steer`, async (importOriginal) => {
  const real = await importOriginal<typeof import("@/lib/steer")>()
  return {
    ...real,
    getSteerRelayConfig: () => ({ url: `http://relay.test`, secret: `s` }),
    relayPostNudge: async () => {},
    relayPostKill: async () => {},
    relayPostInterrupt: async () => ({ delivered: true }),
  }
})
vi.mock(`@/lib/integrations/notifications`, () => ({
  fireAndForgetSessionMessageNotify: () => {},
  notifySessionBlocked: async () => {},
  fireAndForgetAssignmentNotify: () => {},
  fireAndForgetIssueMentionNotify: () => {},
  fireAndForgetCommentNotify: () => {},
  fireAndForgetStatusChangeNotify: () => {},
  fireAndForgetReporterResolution: () => {},
  fireAndForgetPrNotify: () => {},
}))
vi.mock(`@/lib/conversion/events`, () => ({ recordConversionEvent: async () => {} }))

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'sessions' AND column_name = 'started_by_agent_id')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors')
  ) AS ok
`)
if (!ready.ok) console.info(`[team-isolation.real-db] skipped: ${ready.reason}`)

const { NOUN_TOOLS, registerNounTools } = await import(`./nouns`)
const { FULL_ACCESS } = await import(`./scope`)
const { conversationInTx } = await import(`@/lib/agents/conversation`)
const { sendSession, resolveSessionDevice } = await import(`@/lib/sessions/send`)
const { confineMcpAccessToActingSession } = await import(`@/lib/sessions/acting`)
const { DEVICE_OTHER_TEAM_MESSAGE } = await import(`@/lib/session-device-move`)
const { insertMessage } = await import(`@/lib/sessions/messages`)

type ToolResult = { isError?: boolean; content: Array<{ type: string; text?: string }> }
type Call = (args: Record<string, unknown>) => Promise<ToolResult>

const MARK = `TEAMB-SECRET`
const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(name = `Tester`): Promise<McpUser> {
  const id = `iso-test-${id8()}`
  const email = `${id}@iso.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, ${name}, ${email}, true, false, now(), now())
  `)
  made.users.push(id)
  return { id, email, name, image: null, emailVerified: true, isAdmin: false } as unknown as McpUser
}

async function team(name: string, ...owners: string[]) {
  const [row] = await db.insert(teams).values({ name, slug: `iso-${id8()}` }).returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values(owners.map((userId) => ({ teamId: row!.id, userId, role: `owner` as const })))
  return row!.id
}

type World = Awaited<ReturnType<typeof world>>

async function world() {
  const u = await user()
  const mate = await user(`${MARK} mate`)
  const teamA = await team(`Alpha`, u.id)
  const teamB = await team(`${MARK} team`, u.id)
  await db.insert(teamMembers).values({ teamId: teamB, userId: mate.id, role: `member` })

  // Team A: a board, an issue, an agent on U's personal machine, U's
  // conversation with it (THE caller) and a plain thread of U's.
  const boardA = await createTestBoard({ teamId: teamA, prefix: `ISA` })
  const [issueA] = await db
    .insert(issues)
    .values({ boardId: boardA.id, teamId: teamA, title: `Alpha issue`, creatorId: u.id })
    .returning({ id: issues.id, identifier: issues.identifier })
  const personal = `a-box-${id8()}`
  const bBox = `b-box-${id8()}`
  await db.insert(devices).values([
    { userId: u.id, deviceId: personal, label: `personal`, kind: `server`, sharedTeamIds: [] },
    { userId: u.id, deviceId: bBox, label: `${MARK} box`, kind: `server`, sharedTeamIds: [teamB] },
  ])
  const [agentA] = await db.insert(agents).values({ teamId: teamA, name: `Alpha agent`, deviceId: personal, ownerId: u.id }).returning()
  const caller = (await db.transaction((tx) => conversationInTx(tx, agentA!, u.id))).session
  const threadA = await sendSession({ userId: u.id, input: { teamId: teamA, deviceId: personal, text: `Alpha thread` } as never })

  // Team B: everything a team has, each named with the marker.
  const repoB = await ensureTestRepository(teamB, { fullName: `iso-org/${MARK}-${id8()}` })
  const boardB = await createTestBoard({ teamId: teamB, prefix: `ISB`, name: `${MARK} board`, repositoryId: repoB })
  const [issueB, issueB2] = await db
    .insert(issues)
    .values([
      { boardId: boardB.id, teamId: teamB, title: `${MARK} issue`, description: `${MARK} body`, creatorId: u.id },
      { boardId: boardB.id, teamId: teamB, title: `${MARK} issue two`, creatorId: u.id },
    ])
    .returning({ id: issues.id, identifier: issues.identifier })
  const [commentB] = await db
    .insert(comments)
    .values({ issueId: issueB!.id, teamId: teamB, boardId: boardB.id, authorId: u.id, body: `${MARK} comment` })
    .returning({ id: comments.id })
  const [labelB] = await db.insert(labels).values({ teamId: teamB, name: `${MARK} label` }).returning({ id: labels.id })
  await db.insert(issueLabels).values({ issueId: issueB!.id, labelId: labelB!.id, teamId: teamB, boardId: boardB.id })
  await db.insert(issueRelations).values({ issueId: issueB!.id, relatedIssueId: issueB2!.id, type: `related`, teamId: teamB, boardId: boardB.id })
  const [statusB] = await db
    .insert(issueStatuses)
    .values({ teamId: teamB, category: `started`, name: `${MARK} status`, color: `#123456`, sortOrder: 99 })
    .returning({ id: issueStatuses.id })
  const [promptB] = await db.insert(prompts).values({ teamId: teamB, name: `${MARK} prompt`, body: `${MARK} prompt body` }).returning({ id: prompts.id })
  const [agentB] = await db.insert(agents).values({ teamId: teamB, name: `${MARK} agent`, deviceId: bBox, ownerId: u.id }).returning({ id: agents.id })
  const [eventB] = await db
    .insert(events)
    .values({ teamId: teamB, name: `teamb.secret`, payload: { note: MARK }, emittedBy: `user`, userId: u.id })
    .returning({ id: events.id })
  const threadB = await sendSession({ userId: u.id, input: { teamId: teamB, deviceId: bBox, text: `${MARK} thread` } as never })
  // A waiting question card in B (what escalate lifts).
  const questionB = await db.transaction((tx) =>
    insertMessage(tx, threadB.sessionId, {
      authorKind: `agent`,
      body: `${MARK} question`,
      card: { kind: `question`, question: `${MARK}?` },
    })
  )
  const appSlug = `teamb-secret-${id8()}`
  const [appB] = await db
    .insert(apps)
    .values({ teamId: teamB, slug: appSlug, name: `${MARK} app`, description: `${MARK} app`, createdBy: u.id })
    .returning({ id: apps.id })
  const [recordB] = await db
    .insert(appRecords)
    .values({ appId: appB!.id, teamId: teamB, kind: `note`, data: { text: MARK }, createdBy: u.id })
    .returning({ id: appRecords.id })
  const [attachmentB] = await db
    .insert(attachments)
    .values({
      teamId: teamB,
      issueId: issueB!.id,
      boardId: boardB.id,
      uploaderId: u.id,
      filename: `${MARK}.txt`,
      contentType: `text/plain`,
      sizeBytes: 5,
      storageKey: `iso/${id8()}`,
      url: `/api/attachments/x`,
    })
    .returning({ id: attachments.id })
  const [mcpB] = await db
    .insert(mcpServers)
    .values({ teamId: teamB, name: `teamb-secret`, url: `https://teamb-secret.test/mcp` })
    .returning({ id: mcpServers.id })
  const [prB] = await db
    .insert(pullRequests)
    .values({
      teamId: teamB,
      repositoryId: repoB,
      number: 7,
      url: `https://github.com/iso-org/${MARK}-${id8()}/pull/7`,
      title: `${MARK} pr`,
      head: `teamb-secret`,
      base: `main`,
    })
    .returning({ id: pullRequests.id })
  const [inviteB] = await db
    .insert(teamInvites)
    .values({ teamId: teamB, invitedById: u.id, token: `iso-${crypto.randomUUID()}`, email: `teamb-secret@iso.test`, expiresAt: new Date(Date.now() + 86_400_000) })
    .returning({ id: teamInvites.id })
  const [notificationB] = await db
    .insert(notifications)
    .values({ userId: u.id, teamId: teamB, issueId: issueB!.id, boardId: boardB.id, type: `issue_comment`, title: `${MARK} notification` })
    .returning({ id: notifications.id })

  const b = {
    teamId: teamB,
    boardId: boardB.id,
    repositoryId: repoB,
    issueId: issueB!.id,
    identifier: issueB!.identifier as string,
    issue2Id: issueB2!.id,
    commentId: commentB!.id,
    labelId: labelB!.id,
    statusId: statusB!.id,
    promptId: promptB!.id,
    agentId: agentB!.id,
    eventId: eventB!.id,
    sessionId: threadB.sessionId,
    messageId: questionB,
    appId: appB!.id,
    appSlug,
    recordId: recordB!.id,
    attachmentId: attachmentB!.id,
    mcpServerId: mcpB!.id,
    pullRequestId: prB!.id,
    inviteId: inviteB!.id,
    notificationId: notificationB!.id,
    mateId: mate.id,
    deviceId: bBox,
  }
  return {
    u,
    teamA,
    boardA: boardA.id,
    issueA: issueA!,
    personal,
    agentA: agentA!,
    callerId: caller.id,
    threadA: threadA.sessionId,
    b,
  }
}

/** The 16 as U's agent-tree caller sees them (header + confined access). */
async function callerTools(w: World): Promise<(name: string) => Call> {
  const request = new Request(`https://x.test/api/mcp`, { headers: { "x-exp-session-id": w.callerId } })
  const access = await confineMcpAccessToActingSession(FULL_ACCESS, request, w.u.id, w.callerId)
  expect(access.full).toBe(false)
  const handlers = new Map<string, (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>>()
  const server = {
    registerTool: (name: string, _def: unknown, handler: (args: Record<string, unknown>, extra: unknown) => Promise<ToolResult>) => {
      handlers.set(name, handler)
    },
  }
  registerNounTools(server as never, w.u, request, access, w.callerId)
  return (name) => async (args) => {
    const handler = handlers.get(`exponential_${name}`)
    if (!handler) throw new Error(`no tool ${name}`)
    try {
      return await handler(args, {})
    } catch (e) {
      // A throw is a refusal too (the SDK turns it into an error result).
      return { isError: true, content: [{ type: `text`, text: e instanceof Error ? e.message : String(e) }] }
    }
  }
}

/** Every row of B, hashed per table: every table with a `team_id`, B's team
 *  row and the b-box device row. */
async function snapshotB(w: World): Promise<Record<string, string>> {
  const tables = await db.execute<{ table_name: string }>(sql`
    SELECT c.table_name FROM information_schema.columns c
    JOIN information_schema.tables t ON t.table_name = c.table_name AND t.table_schema = c.table_schema
    WHERE c.table_schema = 'public' AND c.column_name = 'team_id' AND t.table_type = 'BASE TABLE'
    ORDER BY c.table_name
  `)
  const out: Record<string, string> = {}
  const hash = async (label: string, query: ReturnType<typeof sql>) => {
    const rows = await db.execute<{ h: string | null }>(query)
    out[label] = rows.rows?.[0]?.h ?? (rows as unknown as Array<{ h: string | null }>)[0]?.h ?? ``
  }
  const names = (tables.rows ?? (tables as unknown as Array<{ table_name: string }>)).map((row) => row.table_name)
  for (const name of names) {
    await hash(
      name,
      sql`SELECT md5(coalesce(string_agg(to_jsonb(t)::text, ',' ORDER BY to_jsonb(t)::text), '')) AS h FROM ${sql.identifier(name)} t WHERE team_id = ${w.b.teamId}`
    )
  }
  await hash(`teams`, sql`SELECT md5(to_jsonb(t)::text) AS h FROM teams t WHERE id = ${w.b.teamId}`)
  await hash(`devices`, sql`SELECT md5(coalesce(string_agg(to_jsonb(t)::text, ','), '')) AS h FROM devices t WHERE device_id = ${w.b.deviceId}`)
  return out
}

/** B's ids, the strings a read must never carry. */
const secretIds = (w: World) => [w.b.teamId, w.b.boardId, w.b.repositoryId, w.b.issueId, w.b.identifier, w.b.issue2Id, w.b.commentId, w.b.labelId, w.b.statusId, w.b.promptId, w.b.agentId, w.b.eventId, w.b.sessionId, w.b.messageId, w.b.appId, w.b.appSlug, w.b.recordId, w.b.attachmentId, w.b.mcpServerId, w.b.pullRequestId, w.b.inviteId, w.b.notificationId, w.b.mateId, w.b.deviceId]

type Kind = `read` | `write`
interface Case {
  kind: Kind
  /** `noun.op` (or a verb's name) the case covers. */
  covers: string
  args: (w: World) => Record<string, unknown>
}

const r = (covers: string, args: Case[`args`]): Case => ({ kind: `read`, covers, args })
const wr = (covers: string, args: Case[`args`]): Case => ({ kind: `write`, covers, args })

/** The matrix: every op with team B's ids. */
const CASES: Record<string, Case> = {
  // issues
  [`issues.list teamId`]: r(`issues.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`issues.list boardId`]: r(`issues.list`, (w) => ({ op: `list`, boardId: w.b.boardId })),
  [`issues.list boardIds`]: r(`issues.list`, (w) => ({ op: `list`, boardIds: [w.b.boardId] })),
  [`issues.list unfiltered`]: r(`issues.list`, () => ({ op: `list`, search: `TEAMB` })),
  [`issues.get identifier`]: r(`issues.get`, (w) => ({ op: `get`, id: w.b.identifier })),
  [`issues.get uuid`]: r(`issues.get`, (w) => ({ op: `get`, id: w.b.issueId })),
  [`issues.show teamId`]: r(`issues.show`, (w) => ({ op: `show`, teamId: w.b.teamId })),
  [`issues.show boardId`]: r(`issues.show`, (w) => ({ op: `show`, boardId: w.b.boardId })),
  [`issues.show unfiltered`]: r(`issues.show`, () => ({ op: `show` })),
  [`issues.create`]: wr(`issues.create`, (w) => ({ op: `create`, boardId: w.b.boardId, title: `leak` })),
  [`issues.create parent B in A`]: wr(`issues.create`, (w) => ({ op: `create`, boardId: w.boardA, title: `leak`, parentId: w.b.issueId })),
  [`issues.update`]: wr(`issues.update`, (w) => ({ op: `update`, id: w.b.identifier, title: `hijacked` })),
  [`issues.delete`]: wr(`issues.delete`, (w) => ({ op: `delete`, id: w.b.issueId })),
  [`issues.activity`]: r(`issues.activity`, (w) => ({ op: `activity`, issueId: w.b.issueId })),
  [`issues.update_status`]: wr(`issues.update_status`, (w) => ({ op: `update_status`, id: w.b.issueId, status: `done` })),
  [`issues.update_status B status on A`]: wr(`issues.update_status`, (w) => ({ op: `update_status`, id: w.issueA.id, statusId: w.b.statusId })),
  [`issues.subscribe`]: wr(`issues.subscribe`, (w) => ({ op: `subscribe`, issueId: w.b.issueId })),
  [`issues.unsubscribe`]: wr(`issues.unsubscribe`, (w) => ({ op: `unsubscribe`, issueId: w.b.issueId })),
  [`issues.unlink`]: wr(`issues.unlink`, (w) => ({ op: `unlink`, id: w.b.issueId, sessionId: w.b.sessionId })),
  [`issues.labels_add`]: wr(`issues.labels_add`, (w) => ({ op: `labels_add`, issueId: w.b.issueId, labelId: w.b.labelId })),
  [`issues.labels_add B label on A`]: wr(`issues.labels_add`, (w) => ({ op: `labels_add`, issueId: w.issueA.id, labelId: w.b.labelId })),
  [`issues.labels_remove`]: wr(`issues.labels_remove`, (w) => ({ op: `labels_remove`, issueId: w.b.issueId, labelId: w.b.labelId })),
  [`issues.relations_add`]: wr(`issues.relations_add`, (w) => ({ op: `relations_add`, issueId: w.issueA.id, relatedIssueId: w.b.issueId, type: `related` })),
  [`issues.relations_remove`]: wr(`issues.relations_remove`, (w) => ({ op: `relations_remove`, issueId: w.b.issueId, relatedIssueId: w.b.issue2Id, type: `related` })),
  // comments
  [`comments.list`]: r(`comments.list`, (w) => ({ op: `list`, issueId: w.b.identifier })),
  [`comments.create`]: wr(`comments.create`, (w) => ({ op: `create`, issueId: w.b.issueId, body: `leak` })),
  [`comments.update`]: wr(`comments.update`, (w) => ({ op: `update`, id: w.b.commentId, body: `hijacked` })),
  [`comments.delete`]: wr(`comments.delete`, (w) => ({ op: `delete`, id: w.b.commentId })),
  // attachments
  [`attachments.list`]: r(`attachments.list`, (w) => ({ op: `list`, issueId: w.b.issueId })),
  [`attachments.get`]: r(`attachments.get`, (w) => ({ op: `get`, id: w.b.attachmentId })),
  [`attachments.upload`]: wr(`attachments.upload`, (w) => ({ op: `upload`, issueId: w.b.issueId, filename: `x.txt`, contentType: `text/plain`, dataBase64: `aGVsbG8=` })),
  [`attachments.delete`]: wr(`attachments.delete`, (w) => ({ op: `delete`, id: w.b.attachmentId })),
  // boards
  [`boards.list teamId`]: r(`boards.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`boards.list unfiltered`]: r(`boards.list`, () => ({ op: `list` })),
  [`boards.get`]: r(`boards.get`, (w) => ({ op: `get`, id: w.b.boardId })),
  [`boards.create`]: wr(`boards.create`, (w) => ({ op: `create`, teamId: w.b.teamId, name: `Leak`, prefix: `LEAK` })),
  [`boards.update`]: wr(`boards.update`, (w) => ({ op: `update`, id: w.b.boardId, name: `hijacked` })),
  [`boards.delete`]: wr(`boards.delete`, (w) => ({ op: `delete`, id: w.b.boardId })),
  [`boards.set_repository`]: wr(`boards.set_repository`, (w) => ({ op: `set_repository`, id: w.b.boardId, repositoryId: w.b.repositoryId })),
  // teams
  [`teams.list`]: r(`teams.list`, () => ({ op: `list` })),
  [`teams.get`]: r(`teams.get`, (w) => ({ op: `get`, id: w.b.teamId })),
  [`teams.create`]: wr(`teams.create`, () => ({ op: `create`, name: `Leak team` })),
  [`teams.update`]: wr(`teams.update`, (w) => ({ op: `update`, id: w.b.teamId, name: `hijacked` })),
  // people
  [`people.list`]: r(`people.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`people.add`]: wr(`people.add`, (w) => ({ op: `add`, teamId: w.b.teamId, email: `leak@iso.test` })),
  [`people.invites_list`]: r(`people.invites_list`, (w) => ({ op: `invites_list`, teamId: w.b.teamId })),
  [`people.invites_create`]: wr(`people.invites_create`, (w) => ({ op: `invites_create`, teamId: w.b.teamId, email: `leak@iso.test` })),
  [`people.invites_revoke`]: wr(`people.invites_revoke`, (w) => ({ op: `invites_revoke`, id: w.b.inviteId })),
  // team_config
  [`team_config.labels_list`]: r(`team_config.labels_list`, (w) => ({ op: `labels_list`, teamId: w.b.teamId })),
  [`team_config.labels_get`]: r(`team_config.labels_get`, (w) => ({ op: `labels_get`, id: w.b.labelId })),
  [`team_config.labels_create`]: wr(`team_config.labels_create`, (w) => ({ op: `labels_create`, teamId: w.b.teamId, name: `leak` })),
  [`team_config.labels_update`]: wr(`team_config.labels_update`, (w) => ({ op: `labels_update`, id: w.b.labelId, name: `hijacked` })),
  [`team_config.labels_delete`]: wr(`team_config.labels_delete`, (w) => ({ op: `labels_delete`, id: w.b.labelId })),
  [`team_config.statuses_list`]: r(`team_config.statuses_list`, (w) => ({ op: `statuses_list`, teamId: w.b.teamId })),
  [`team_config.statuses_create`]: wr(`team_config.statuses_create`, (w) => ({ op: `statuses_create`, teamId: w.b.teamId, category: `started`, name: `leak`, color: `#000000` })),
  [`team_config.statuses_update`]: wr(`team_config.statuses_update`, (w) => ({ op: `statuses_update`, id: w.b.statusId, name: `hijacked` })),
  [`team_config.statuses_delete`]: wr(`team_config.statuses_delete`, (w) => ({ op: `statuses_delete`, id: w.b.statusId })),
  [`team_config.prompts_list`]: r(`team_config.prompts_list`, (w) => ({ op: `prompts_list`, teamId: w.b.teamId })),
  [`team_config.prompts_get`]: r(`team_config.prompts_get`, (w) => ({ op: `prompts_get`, id: w.b.promptId })),
  [`team_config.prompts_create`]: wr(`team_config.prompts_create`, (w) => ({ op: `prompts_create`, teamId: w.b.teamId, name: `leak`, body: `leak` })),
  [`team_config.prompts_update`]: wr(`team_config.prompts_update`, (w) => ({ op: `prompts_update`, id: w.b.promptId, body: `hijacked` })),
  [`team_config.prompts_delete`]: wr(`team_config.prompts_delete`, (w) => ({ op: `prompts_delete`, id: w.b.promptId })),
  [`team_config.agents_list`]: r(`team_config.agents_list`, (w) => ({ op: `agents_list`, teamId: w.b.teamId })),
  [`team_config.mcp_servers_list`]: r(`team_config.mcp_servers_list`, (w) => ({ op: `mcp_servers_list`, teamId: w.b.teamId })),
  [`team_config.mcp_servers_add`]: wr(`team_config.mcp_servers_add`, (w) => ({ op: `mcp_servers_add`, teamId: w.b.teamId, url: `https://leak.test/mcp` })),
  [`team_config.mcp_servers_remove`]: wr(`team_config.mcp_servers_remove`, (w) => ({ op: `mcp_servers_remove`, id: w.b.mcpServerId })),
  [`team_config.repositories_list`]: r(`team_config.repositories_list`, (w) => ({ op: `repositories_list`, teamId: w.b.teamId })),
  [`team_config.repositories_add`]: wr(`team_config.repositories_add`, (w) => ({ op: `repositories_add`, teamId: w.b.teamId, fullName: `leak/leak` })),
  // apps
  [`apps.list teamId`]: r(`apps.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`apps.list unfiltered`]: r(`apps.list`, () => ({ op: `list` })),
  [`apps.get`]: r(`apps.get`, (w) => ({ op: `get`, slug: w.b.appSlug, teamId: w.b.teamId })),
  [`apps.upsert`]: wr(`apps.upsert`, (w) => ({ op: `upsert`, slug: w.b.appSlug, teamId: w.b.teamId, package: { root: { id: `r`, component: `Text`, props: { text: `x` } } } })),
  [`apps.delete`]: wr(`apps.delete`, (w) => ({ op: `delete`, slug: w.b.appSlug, teamId: w.b.teamId })),
  [`apps.catalog`]: r(`apps.catalog`, () => ({ op: `catalog` })),
  [`apps.records_list`]: r(`apps.records_list`, (w) => ({ op: `records_list`, slug: w.b.appSlug, teamId: w.b.teamId })),
  [`apps.records_upsert`]: wr(`apps.records_upsert`, (w) => ({ op: `records_upsert`, slug: w.b.appSlug, teamId: w.b.teamId, kind: `note`, data: { text: `leak` } })),
  [`apps.records_delete`]: wr(`apps.records_delete`, (w) => ({ op: `records_delete`, slug: w.b.appSlug, teamId: w.b.teamId, id: w.b.recordId })),
  // sessions_send (the verb)
  [`sessions_send teamId`]: wr(`sessions_send`, (w) => ({ teamId: w.b.teamId, deviceId: w.personal, text: `leak` })),
  [`sessions_send B session`]: wr(`sessions_send`, (w) => ({ sessionId: w.b.sessionId, text: `leak` })),
  [`sessions_send B session stop`]: wr(`sessions_send`, (w) => ({ sessionId: w.b.sessionId, stop: true })),
  [`sessions_send B agent`]: wr(`sessions_send`, (w) => ({ agentId: w.b.agentId, text: `leak` })),
  [`sessions_send B parent`]: wr(`sessions_send`, (w) => ({ parentId: w.b.sessionId, text: `leak` })),
  [`sessions_send B app`]: wr(`sessions_send`, (w) => ({ appId: w.b.appId, text: `leak` })),
  [`sessions_send b-box`]: wr(`sessions_send`, (w) => ({ deviceId: w.b.deviceId, text: `leak` })),
  [`sessions_send B prompt`]: wr(`sessions_send`, (w) => ({ deviceId: w.personal, promptId: w.b.promptId })),
  [`sessions_send B issue`]: wr(`sessions_send`, (w) => ({ deviceId: w.personal, issueIds: [w.b.identifier], text: `leak` })),
  [`sessions_send B repository`]: wr(`sessions_send`, (w) => ({ deviceId: w.personal, repositoryId: w.b.repositoryId, text: `leak` })),
  [`sessions_send B mcp server`]: wr(`sessions_send`, (w) => ({ deviceId: w.personal, mcpServerIds: [w.b.mcpServerId], text: `leak` })),
  // Answers (its own session) but watches nothing outside its team: below.
  [`sessions_send wakeOn B`]: r(`sessions_send`, (w) => ({ sessionId: w.callerId, wakeOn: [w.b.sessionId] })),
  [`sessions_send move A thread to b-box`]: wr(`sessions_send`, (w) => ({ sessionId: w.threadA, deviceId: w.b.deviceId })),
  // sessions
  [`sessions.get B`]: r(`sessions.get`, (w) => ({ op: `get`, id: w.b.sessionId, messages: 5 })),
  [`sessions.get own`]: r(`sessions.get`, () => ({ op: `get` })),
  [`sessions.list teamId`]: r(`sessions.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`sessions.list B agent`]: r(`sessions.list`, (w) => ({ op: `list`, agentId: w.b.agentId })),
  [`sessions.list unfiltered`]: r(`sessions.list`, () => ({ op: `list` })),
  [`sessions.link B issue to own`]: wr(`sessions.link`, (w) => ({ op: `link`, issueIds: [w.b.identifier] })),
  [`sessions.link to B session`]: wr(`sessions.link`, (w) => ({ op: `link`, issueIds: [w.issueA.identifier], sessionId: w.b.sessionId })),
  [`sessions.unlink`]: wr(`sessions.unlink`, (w) => ({ op: `unlink`, issueId: w.b.issueId, sessionId: w.b.sessionId })),
  [`sessions.configure`]: wr(`sessions.configure`, (w) => ({ op: `configure`, sessionId: w.b.sessionId, model: `leak` })),
  [`sessions.escalate`]: wr(`sessions.escalate`, (w) => ({ op: `escalate`, messageId: w.b.messageId })),
  [`sessions.end`]: wr(`sessions.end`, (w) => ({ op: `end`, id: w.b.sessionId, summary: `hijacked` })),
  // session_self: the caller's OWN session only; it must still answer nothing of B.
  [`session_self.suggest`]: r(`session_self.suggest`, () => ({ op: `suggest`, replies: [`ok`] })),
  [`session_self.compact`]: r(`session_self.compact`, () => ({ op: `compact`, reason: `test` })),
  [`session_self.report_bug`]: r(`session_self.report_bug`, () => ({ op: `report_bug`, title: `x`, description: `x` })),
  // show (the verb): the caller's own card.
  [`show`]: r(`show`, () => ({ title: `Hello`, story: false, surface: { root: { id: `t`, component: `Text`, props: { text: `hi` } } } })),
  // pr
  [`pr.open`]: wr(`pr.open`, (w) => ({ op: `open`, title: `leak`, repositoryId: w.b.repositoryId, issueIds: [w.b.identifier] })),
  [`pr.merge`]: wr(`pr.merge`, (w) => ({ op: `merge`, pullRequestId: w.b.pullRequestId })),
  [`pr.close`]: wr(`pr.close`, (w) => ({ op: `close`, pullRequestId: w.b.pullRequestId })),
  [`pr.retarget`]: wr(`pr.retarget`, (w) => ({ op: `retarget`, pullRequestId: w.b.pullRequestId, base: `dev` })),
  [`pr.update`]: wr(`pr.update`, (w) => ({ op: `update`, pullRequestId: w.b.pullRequestId, title: `hijacked` })),
  [`pr.update by repo`]: wr(`pr.update`, (w) => ({ op: `update`, repositoryId: w.b.repositoryId, prNumber: 7, title: `hijacked` })),
  [`pr.unlink`]: wr(`pr.unlink`, (w) => ({ op: `unlink`, issueId: w.b.issueId, pullRequestId: w.b.pullRequestId })),
  [`pr.files`]: r(`pr.files`, (w) => ({ op: `files`, pullRequestId: w.b.pullRequestId })),
  // devices
  [`devices.list unfiltered`]: r(`devices.list`, () => ({ op: `list` })),
  [`devices.list teamId`]: r(`devices.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`devices.account_login`]: wr(`devices.account_login`, (w) => ({ op: `account_login`, deviceId: w.b.deviceId, agent: `claude` })),
  // events
  [`events.emit`]: wr(`events.emit`, (w) => ({ op: `emit`, name: `leak`, teamId: w.b.teamId })),
  [`events.list teamId`]: r(`events.list`, (w) => ({ op: `list`, teamId: w.b.teamId })),
  [`events.list unfiltered`]: r(`events.list`, () => ({ op: `list` })),
  // notifications: the caller's own, but one about team B is B's.
  [`notifications.list`]: r(`notifications.list`, () => ({ op: `list` })),
  [`notifications.mark_read B`]: wr(`notifications.mark_read`, (w) => ({ op: `mark_read`, id: w.b.notificationId })),
  [`notifications.mark_read all`]: r(`notifications.mark_read`, () => ({ op: `mark_read`, all: true })),
}

/** Ops whose case cannot assert the B rule as a refusal, with why. */
const EXEMPT: Record<string, string> = {}

const surface = () => {
  const all: string[] = []
  for (const spec of NOUN_TOOLS) {
    if (spec.verb) all.push(spec.name)
    for (const op of Object.keys(spec.ops ?? {})) all.push(`${spec.name}.${op}`)
  }
  return all
}

afterAll(async () => {
  if (!ready.ok) return
  if (made.teams.length) {
    await db.execute(sql`DELETE FROM teams WHERE id IN ${sql.raw(`('${made.teams.join(`','`)}')`)}`)
  }
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe(`team isolation matrix covers the whole MCP surface (#EXP-1440)`, () => {
  it(`every noun op and both verbs have a case or an exemption`, () => {
    const covered = new Set(Object.values(CASES).map((c) => c.covers))
    const missing = surface().filter((row) => !covered.has(row) && !(row in EXEMPT))
    expect(missing).toEqual([])
    expect(surface()).toEqual(expect.arrayContaining([`sessions_send`, `show`]))
    // No stale case for an op that no longer exists.
    expect([...covered].filter((row) => !surface().includes(row))).toEqual([])
  })
})

describe.skipIf(!ready.ok)(`an agent-tree caller of team A never reaches team B (#EXP-1440)`, () => {
  let w: World
  let tools: (name: string) => Call
  beforeAll(async () => {
    vi.stubGlobal(`fetch`, async () => {
      throw new Error(`network disabled in this test`)
    })
    w = await world()
    tools = await callerTools(w)
  }, 60_000)
  afterAll(() => {
    vi.unstubAllGlobals()
  })

  it.each(Object.entries(CASES))(`%s`, async (_name, testCase) => {
    const [noun] = testCase.covers.split(`.`)
    const before = await snapshotB(w)
    const result = await tools(noun!)(testCase.args(w))
    const text = result.content.map((part) => part.text ?? ``).join(`\n`)
    expect(text, `the marker leaked`).not.toContain(MARK)
    if (testCase.kind === `write`) {
      expect(result.isError, `a write into B succeeded: ${text}`).toBe(true)
    } else if (!result.isError) {
      for (const id of secretIds(w)) expect(text, `an id of B leaked`).not.toContain(id)
    }
    expect(await snapshotB(w), `a row of B changed`).toEqual(before)
  })

  it(`devices list (no teamId) names neither b-box nor team B; the personal machine stays`, async () => {
    const result = await tools(`devices`)({ op: `list` })
    expect(result.isError, result.content[0]?.text).toBeFalsy()
    const text = result.content[0]!.text!
    expect(text).toContain(w.personal)
    expect(text).not.toContain(w.b.deviceId)
    expect(text).not.toContain(w.b.teamId)
  })

  it(`sessions_send refuses a new thread on b-box and a move of an A thread to it, with the device message`, async () => {
    const start = await tools(`sessions_send`)({ text: `On b-box`, deviceId: w.b.deviceId })
    expect(start.isError).toBe(true)
    expect(start.content[0]!.text).toContain(DEVICE_OTHER_TEAM_MESSAGE)
    const move = await tools(`sessions_send`)({ sessionId: w.threadA, deviceId: w.b.deviceId })
    expect(move.isError).toBe(true)
    expect(move.content[0]!.text).toContain(DEVICE_OTHER_TEAM_MESSAGE)
    const [thread] = await db.select({ deviceId: sessions.deviceId }).from(sessions).where(eq(sessions.id, w.threadA))
    expect(thread!.deviceId).toBe(w.personal)
  })

  it(`wakeOn never watches a session of B`, async () => {
    const result = await tools(`sessions_send`)({ sessionId: w.callerId, wakeOn: [w.b.sessionId] })
    expect(result.isError, result.content[0]?.text).toBeFalsy()
    const [caller] = await db.select({ wakeOn: sessions.wakeOn }).from(sessions).where(eq(sessions.id, w.callerId))
    expect(caller!.wakeOn ?? []).not.toContain(w.b.sessionId)
  })

  it(`the personal machine still starts a thread in A`, async () => {
    const started = await tools(`sessions_send`)({ text: `On my box`, deviceId: w.personal })
    expect(started.isError, started.content[0]?.text).toBeFalsy()
    const { sessionId } = JSON.parse(started.content[0]!.text!) as { sessionId: string }
    const [row] = await db
      .select({ teamId: sessions.teamId, deviceId: sessions.deviceId })
      .from(sessions)
      .where(eq(sessions.id, sessionId))
    expect(row).toMatchObject({ teamId: w.teamA, deviceId: w.personal })
  })
})

describe.skipIf(!ready.ok)(`a person's own machine shared only with another team (#EXP-1440)`, () => {
  it(`resolveSessionDevice refuses it for A and serves it shared for B`, async () => {
    const u = await user()
    const teamA = await team(`Alpha`, u.id)
    const teamB = await team(`Beta`, u.id)
    const bBox = `b-box-${id8()}`
    await db.insert(devices).values({ userId: u.id, deviceId: bBox, label: `b`, kind: `server`, sharedTeamIds: [teamB] })
    const refused = resolveSessionDevice(u.id, teamA, bBox)
    await expect(refused).rejects.toMatchObject({ code: `FORBIDDEN`, message: DEVICE_OTHER_TEAM_MESSAGE })
    await expect(resolveSessionDevice(u.id, teamB, bBox)).resolves.toMatchObject({ shared: true, ownerId: u.id })
    // Without an agent tree, the move of a person's A thread is refused too.
    const personal = `a-box-${id8()}`
    await db.insert(devices).values({ userId: u.id, deviceId: personal, label: `a`, kind: `server` })
    const thread = await sendSession({ userId: u.id, input: { teamId: teamA, deviceId: personal, text: `Mine` } as never })
    await expect(
      sendSession({ userId: u.id, input: { sessionId: thread.sessionId, deviceId: bBox } as never })
    ).rejects.toMatchObject({ code: `FORBIDDEN`, message: DEVICE_OTHER_TEAM_MESSAGE })
  })
})
