import fixture from "../fixtures/local-parity.json" with { type: "json" }

// #EDE-55 (fixtures/local-parity.json): the rules a LOCAL DESKTOP replays
// without a server. TS reference; elocal (Rust, `elocal::parity`) replays
// every case over its SQLite store. Rows are the server's tables in
// snake_case (the synced-shape layout every client holds).

export const LOCAL_PROCEDURES: readonly string[] = fixture.procedures.local
export const LOCAL_EMAIL_DOMAIN = fixture.identity.emailDomain
export const LOCAL_SEED = fixture.seed
export const LOCAL_ANCHOR_BY_CATEGORY: Readonly<Record<string, string>> = fixture.issues.anchorByCategory

export type Row = Record<string, unknown>
export type Written = { table: string; row: Row }

export function isLocalProcedure(procedure: string): boolean {
  return LOCAL_PROCEDURES.includes(procedure)
}

/** The refusal a Local desktop answers for a procedure it does not serve. */
export function localRefusal(procedure: string): string {
  return fixture.procedures.refusal.replace(`{procedure}`, procedure)
}

/** The identity's email from the machine login. */
export function localEmail(login: string): string {
  const local = login
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, `-`)
    .replace(/^-+|-+$/g, ``)
  return `${local || `local`}@${LOCAL_EMAIL_DOMAIN}`
}

export interface LocalIdentity {
  userId: string
  email: string
  name: string
}

/** A fresh desktop's rows, ids taken from `nextId` in order. */
export function seedRows(desktop: { id: string; name: string }, identity: LocalIdentity, now: string, nextId: () => string): Written[] {
  const out: Written[] = [
    {
      table: `teams`,
      row: { id: desktop.id, name: desktop.name, slug: `${fixture.seed.slugPrefix}${desktop.id.slice(0, 8)}`, icon_url: null, pr_opened_automation: true, pr_merged_automation: true, end_sessions_on_merge: true, yolo_mode: false, merge_mode: `auto`, estimation_type: `none`, time_zone: null, created_at: now, updated_at: now },
    },
    { table: `users`, row: { id: identity.userId, name: identity.name, email: identity.email, image: null, created_at: now, updated_at: now } },
    { table: `team_members`, row: { id: nextId(), team_id: desktop.id, user_id: identity.userId, role: `owner`, created_at: now, updated_at: now } },
  ]
  for (const s of fixture.seed.statuses) {
    out.push({ table: `issue_statuses`, row: { id: nextId(), team_id: desktop.id, category: s.category, name: s.name, color: s.color, sort_order: s.sortOrder, builtin_key: s.key, created_at: now, updated_at: now } })
  }
  const board = fixture.seed.board
  out.push({ table: `boards`, row: { id: nextId(), team_id: desktop.id, name: board.name, slug: board.slug, prefix: board.prefix, color: board.color, icon: null, repository_id: null, default_branch: null, sort_order: 0, created_at: now, updated_at: now } })
  return out
}

export function nextIssueNumber(numbers: readonly number[]): number {
  return numbers.reduce((max, n) => (n > max ? n : max), 0) + 1
}

export function issueIdentifier(prefix: string, number: number): string {
  return `${prefix}-${number}`
}

export function anchorForCategory(category: string): string {
  return LOCAL_ANCHOR_BY_CATEGORY[category] ?? `backlog`
}

export interface StatusRow {
  id: string
  category: string
  builtin_key: string | null
}

/** The status row and anchor a write names: by id, by anchor (the builtin
 *  row with that key), else the builtin backlog. */
export function resolveStatus(statuses: readonly StatusRow[], statusId: string | null | undefined, anchor: string | null | undefined): { status: string; status_id: string | null } {
  if (statusId) {
    const row = statuses.find((s) => s.id === statusId)
    if (row) return { status: row.builtin_key ?? anchorForCategory(row.category), status_id: row.id }
  }
  const key = anchor ?? `backlog`
  const builtin = statuses.find((s) => s.builtin_key === key)
  return { status: key, status_id: builtin?.id ?? null }
}

export interface IssueContext {
  teamId: string
  boardPrefix: string
  numbers: readonly number[]
  statuses: readonly StatusRow[]
  identity: LocalIdentity
  id: string
  now: string
}

/** `issues.create` as the server would store it. */
export function createIssueRow(input: Row, ctx: IssueContext): Row {
  const number = nextIssueNumber(ctx.numbers)
  const status = resolveStatus(ctx.statuses, input.statusId as string | undefined, input.status as string | undefined)
  const agent = typeof input.assigneeAgentId === `string` ? input.assigneeAgentId : null
  const assignee = agent ? null : typeof input.assigneeId === `string` ? input.assigneeId : ctx.identity.userId
  return {
    id: ctx.id,
    board_id: input.boardId,
    team_id: ctx.teamId,
    number,
    identifier: issueIdentifier(ctx.boardPrefix, number),
    title: input.title,
    description: typeof input.description === `string` ? input.description : null,
    status: status.status,
    status_id: status.status_id,
    priority: typeof input.priority === `string` ? input.priority : `none`,
    assignee_id: assignee,
    assignee_agent_id: agent,
    creator_id: ctx.identity.userId,
    creator_agent_id: null,
    source: `user`,
    due_date: typeof input.dueDate === `string` ? input.dueDate : null,
    estimate: typeof input.estimate === `number` ? input.estimate : null,
    sort_order: number,
    completed_at: null,
    duplicate_of_id: null,
    draft_id: typeof input.draftId === `string` ? input.draftId : null,
    via_app_id: null,
    created_at: ctx.now,
    updated_at: ctx.now,
  }
}

const ISSUE_PATCH_COLUMNS: Readonly<Record<string, string>> = {
  title: `title`,
  priority: `priority`,
  assigneeId: `assignee_id`,
  assigneeAgentId: `assignee_agent_id`,
  description: `description`,
  dueDate: `due_date`,
  duplicateOfId: `duplicate_of_id`,
  estimate: `estimate`,
}

/** `issues.update`: a key present = set, null = cleared; a status write
 *  dual-writes the anchor and keeps `completed_at` honest. */
export function applyIssuePatch(row: Row, patch: Row, statuses: readonly StatusRow[], now: string): Row {
  const out: Row = { ...row }
  for (const [key, column] of Object.entries(ISSUE_PATCH_COLUMNS)) {
    if (key in patch) out[column] = patch[key] ?? null
  }
  if (`statusId` in patch || `status` in patch) {
    const resolved = resolveStatus(statuses, patch.statusId as string | undefined, patch.status as string | undefined)
    out.status = resolved.status
    out.status_id = resolved.status_id
    const completed = resolved.status === `done`
    const was = row.status === `done`
    if (completed && !was) out.completed_at = now
    if (!completed) out.completed_at = null
  }
  out.updated_at = now
  return out
}

export const LOCAL_DRAFT_CONSUMED: string = fixture.drafts.consumedRefusal

/** `issueDrafts.upsert` refused: an issue was already filed from the draft. */
export function draftConsumedRefusal(identifier: string): string {
  return LOCAL_DRAFT_CONSUMED.replace(`{identifier}`, identifier)
}

/** `issueDrafts.upsert`: the person's draft row, keyed by the client's id
 *  (a re-save keeps `created_at`; an agent assignee clears the person). */
export function draftRow(input: Row, ctx: { identity: LocalIdentity; existing: Row | null; now: string }): Row {
  const str = (key: string) => (typeof input[key] === `string` ? (input[key] as string) : null)
  const agent = str(`assigneeAgentId`)
  return {
    id: input.id,
    user_id: ctx.identity.userId,
    team_id: input.teamId,
    board_id: input.boardId,
    title: str(`title`) ?? ``,
    description: str(`description`) ?? ``,
    status_id: str(`statusId`),
    priority: str(`priority`) ?? `none`,
    assignee_id: agent ? null : str(`assigneeId`),
    assignee_agent_id: agent,
    label_ids: Array.isArray(input.labelIds) ? input.labelIds : [],
    due_date: str(`dueDate`),
    session_id: str(`sessionId`),
    created_at: (ctx.existing?.created_at as string | undefined) ?? ctx.now,
    updated_at: ctx.now,
  }
}

/** #EXP-1800: `composerDrafts.upsert` refused: an empty text is a delete. */
export const LOCAL_COMPOSER_DRAFT_EMPTY: string = fixture.composerDrafts.emptyRefusal

/** #EXP-1800: `composerDrafts.upsert`: the identity's one row per `key`
 *  (composer-draft.json); a new key takes `id`, a re-save keeps the row's id
 *  and `created_at` and replaces team and text. */
export function composerDraftRow(input: Row, ctx: { identity: LocalIdentity; existing: Row | null; id: string; now: string }): Row {
  return {
    id: (ctx.existing?.id as string | undefined) ?? ctx.id,
    user_id: ctx.identity.userId,
    team_id: input.teamId ?? null,
    key: typeof input.key === `string` ? input.key : ``,
    text: typeof input.text === `string` ? input.text : ``,
    created_at: (ctx.existing?.created_at as string | undefined) ?? ctx.now,
    updated_at: ctx.now,
  }
}

/** `comments.create`: the identity's, one level deep. */
export function createCommentRow(input: Row, ctx: { teamId: string; identity: LocalIdentity; existing: readonly { id: string; parent_id: string | null }[]; id: string; now: string }): Row {
  let parent = typeof input.parentId === `string` ? input.parentId : null
  const parentRow = parent ? ctx.existing.find((c) => c.id === parent) : undefined
  if (parentRow?.parent_id) parent = parentRow.parent_id
  return {
    id: ctx.id,
    issue_id: input.issueId,
    team_id: ctx.teamId,
    author_id: ctx.identity.userId,
    parent_id: parent,
    source: `user`,
    audience: `team`,
    author_agent_id: null,
    via_app_id: null,
    body: input.body,
    edited_at: null,
    created_at: ctx.now,
    updated_at: ctx.now,
  }
}

/** The title of a new root: the name, else the text's first line cut at 80. */
export function sessionTitle(name: string | null | undefined, text: string | null | undefined): string | null {
  if (typeof name === `string` && name.trim()) return name.trim()
  const line = (text ?? ``).split(`\n`).map((l) => l.trim()).find((l) => l.length > 0) ?? ``
  return line ? line.slice(0, 80) : null
}

export interface SendContext {
  teamId: string
  identity: LocalIdentity
  deviceId: string
  /** the target session and its conversation when `sessionId` names one */
  existing?: { session: Row; conversation: Row } | null
  /** #EDE-65 lane L: the rows of the send's `attachmentIds` (id, filename, content_type) */
  attachments?: Row[]
  nextId: () => string
  now: string
}

const INLINE_IMAGE_TYPES = [`image/png`, `image/jpeg`, `image/webp`, `image/gif`, `image/avif`]

/** The server's `attachmentLine`: an embed for an inline image, else a link. */
export function attachmentLine(row: Row): string {
  const url = `/api/attachments/${row.id as string}`
  const name = String(row.filename ?? ``).replace(/[[\]]/g, ``)
  return INLINE_IMAGE_TYPES.includes(String(row.content_type)) ? `![${name}](${url})` : `[${name}](${url})`
}

/** A sent body with the line of every attachment it does not name yet (the server's `claimMessageAttachments`). */
export function bodyWithAttachments(text: string, ids: readonly string[], attachments: readonly Row[]): string {
  const lines = ids
    .filter((id) => !text.includes(`/api/attachments/${id}`))
    .map((id) => attachments.find((row) => row.id === id))
    .filter((row): row is Row => !!row)
    .map(attachmentLine)
  if (lines.length === 0) return text
  return [text.trimEnd(), ...lines].filter(Boolean).join(`\n\n`)
}

/** `sessions.send`: the rows written and the answer. */
export function sendRows(input: Row, ctx: SendContext): { rows: Written[]; output: { sessionId: string; messageId: string } } {
  const rows: Written[] = []
  const str = (v: unknown) => (typeof v === `string` ? v : null)
  const bool = (v: unknown) => v === true
  let sessionId: string
  let conversationId: string
  if (ctx.existing) {
    sessionId = ctx.existing.session.id as string
    conversationId = ctx.existing.conversation.id as string
  } else {
    sessionId = ctx.nextId()
    conversationId = ctx.nextId()
    rows.push({
      table: `sessions`,
      row: {
        id: sessionId,
        team_id: ctx.teamId,
        parent_id: null,
        root_id: sessionId,
        depth: 0,
        agent_id: str(input.agentId),
        app_id: str(input.appId),
        user_id: ctx.identity.userId,
        device_id: ctx.deviceId,
        cli: str(input.cli),
        account: str(input.account),
        model: str(input.model),
        effort: str(input.effort),
        plan_mode: bool(input.planMode),
        computer_use: bool(input.computerUse),
        ultracode: bool(input.ultracode),
        subagent_model: str(input.subagentModel),
        repository_id: null,
        branch: null,
        status: `queued`,
        busy: false,
        needs_input: false,
        blocked: null,
        started_at: null,
        last_turn_at: null,
        ended_at: null,
        agent_instructions_at: null,
        started_by_agent_id: null,
        shared: false,
        title: sessionTitle(str(input.name), str(input.text)),
        seal_public_key: null,
        seal_epoch: null,
        seal_rotate_epoch: null,
        sealed_title: null,
        guide: null,
        via_app_id: null,
        created_at: ctx.now,
        updated_at: ctx.now,
      },
    })
    rows.push({ table: `conversations`, row: { id: conversationId, team_id: ctx.teamId, kind: `session`, session_id: sessionId, title: null, digest: null, created_at: ctx.now, updated_at: ctx.now } })
  }
  const messageId = ctx.nextId()
  const attachmentIds: string[] = Array.isArray(input.attachmentIds) ? input.attachmentIds : []
  rows.push({
    table: `messages`,
    row: {
      id: messageId,
      conversation_id: conversationId,
      root_id: sessionId,
      team_id: ctx.teamId,
      author_kind: `user`,
      author_id: ctx.identity.userId,
      author_session_id: null,
      body: bodyWithAttachments(str(input.text) ?? ``, attachmentIds, ctx.attachments ?? []),
      card: null,
      attachment_ids: attachmentIds,
      issue_brief: null,
      answered_at: null,
      delivered_at: null,
      consumed_at: null,
      created_at: ctx.now,
      updated_at: ctx.now,
    },
  })
  if (!ctx.existing && Array.isArray(input.issueIds)) {
    input.issueIds.forEach((issueId, i) => {
      rows.push({ table: `session_issues`, row: { id: ctx.nextId(), session_id: sessionId, issue_id: issueId, team_id: ctx.teamId, source: `user`, added_by: ctx.identity.userId, added_by_session_id: null, sort_order: i, created_at: ctx.now, updated_at: ctx.now } })
    })
  }
  return { rows, output: { sessionId, messageId } }
}

/** `agents.ensureAssistant`: the identity's one assistant row. */
export function assistantRow(ctx: { teamId: string; identity: LocalIdentity; deviceId: string; id: string; now: string }): Row {
  return { id: ctx.id, team_id: ctx.teamId, name: `Assistant`, icon: null, color: null, triggers: [], device_id: ctx.deviceId, cli: null, account: null, model: null, skill_ids: [], owner_id: ctx.identity.userId, sort_order: 0, role: `assistant`, instructions_updated_at: null, created_at: ctx.now, updated_at: ctx.now }
}
