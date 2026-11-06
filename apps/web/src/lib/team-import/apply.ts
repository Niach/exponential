// #EXP-1713: the commit of a team import (fixtures/team-import.json): every
// staged row applied in ONE transaction, in `tables` order, with the
// preserve-timestamps guard on (the rows keep their local stamps). Ids are
// kept; a primary key that already exists is remapped to a fresh uuid and
// recorded in `import_id_map`, and every reference (a foreign key into an
// imported table, an array of ids) follows. User columns take the identity
// rule (the local identity = the caller, anyone else = null, the caller
// where the column is required), mentions of the local identity are
// rewritten, statuses map onto the team's builtins, sessions arrive ended
// and private, attachments are the rows the signed upload already landed.
// The pure half (`planRow`, `convertRow`) has no database and is unit
// tested; `applyImport` runs it against a transaction.
import { randomUUID } from "crypto"
import { and, eq, inArray, sql } from "drizzle-orm"
import type { PgColumn } from "drizzle-orm/pg-core"
import { TRPCError } from "@trpc/server"
import type { db } from "@/db/connection"
import { apps, attachments, conversations, importIdMap, issueStatuses, type App } from "@/db/schema"
import { importRecords } from "@/lib/apps/records-service"
import { ISSUE_STATUS_STARTED_MAX } from "@/lib/domain"
import {
  mapUserId,
  matchStatuses,
  remapRow,
  rewriteLocalMentions,
  sessionArrival,
  TEAM_IMPORT_TABLES,
  TEAM_IMPORT_TEXT_COLUMNS,
  type StatusLike,
} from "@exp/domain-contract/team-import"
import { ARRAY_REFERENCES, DROPPED_COLUMNS, IMPORT_TABLES, tableMeta, type TableMeta } from "@/lib/team-import/tables"

export type Tx = Parameters<Parameters<typeof db.transaction>[0]>[0]

export interface StagedRow {
  tableName: string
  localId: string
  row: Record<string, unknown>
}

export interface ImportIdentity {
  /** the local identity's user id (the manifest) */
  localUserId: string
  localEmail: string
  /** the caller: the team's owner once committed */
  callerId: string
  callerEmail: string
}

/** The id map of an import: `table/localId` → server id. */
export class IdMap {
  private readonly map = new Map<string, string>()
  readonly entries: { tableName: string; localId: string; serverId: string }[] = []
  set(tableName: string, localId: string, serverId: string): void {
    this.map.set(`${tableName}/${localId}`, serverId)
    this.entries.push({ tableName, localId, serverId })
  }
  get(tableName: string, localId: string): string {
    return this.map.get(`${tableName}/${localId}`) ?? localId
  }
  lookup = (tableName: string, localId: string): string => this.get(tableName, localId)
}

const BATCH = 200

function bad(message: string): TRPCError {
  return new TRPCError({ code: `BAD_REQUEST`, message })
}

/** One row as the server stores it: the team's, dropped columns gone,
 *  references remapped, the identity rule, mentions rewritten. Returns the
 *  snake_case row (self references nulled, kept aside) and the remap count. */
export function planRow(
  meta: TableMeta,
  row: Record<string, unknown>,
  teamId: string,
  identity: ImportIdentity,
  ids: IdMap
): { row: Record<string, unknown>; deferred: Record<string, string>; remapped: number } {
  const known = new Set(meta.columns.map((c) => c.name))
  const dropped = new Set(DROPPED_COLUMNS[meta.name] ?? [])
  const out: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(row)) {
    if (known.has(key) && !dropped.has(key)) out[key] = value
  }
  for (const column of meta.teamColumns) out[column] = teamId
  const references = { ...meta.references, ...(ARRAY_REFERENCES[meta.name] ?? {}) }
  const deferred: Record<string, string> = {}
  for (const column of meta.selfReferences) {
    const value = out[column]
    if (typeof value === `string`) deferred[column] = ids.get(meta.name, value)
    out[column] = null
  }
  const userArrays = new Set(Object.entries(references).filter(([, t]) => t === `users`).map(([c]) => c))
  const remap = Object.fromEntries(Object.entries(references).filter(([c]) => !userArrays.has(c)))
  const remapped = remapRow(out, remap, ids.lookup)
  const result = remapped.row
  for (const column of [...meta.userColumns, ...userArrays]) {
    if (!(column in result) && !meta.columns.find((c) => c.name === column)?.notNull) continue
    const required = meta.columns.find((c) => c.name === column)?.notNull ?? false
    result[column] = mapUserId(result[column] ?? null, identity.localUserId, identity.callerId, required)
  }
  for (const column of TEAM_IMPORT_TEXT_COLUMNS[meta.name] ?? []) {
    const value = result[column]
    if (typeof value === `string`) result[column] = rewriteLocalMentions(value, identity.localEmail, identity.callerEmail)
  }
  return { row: result, deferred, remapped: remapped.remapped }
}

/** The drizzle values of a planned row: keys by column key, timestamps as
 *  Dates; a required column without a value fails naming it. */
export function convertRow(meta: TableMeta, row: Record<string, unknown>, localId: string): Record<string, unknown> {
  const values: Record<string, unknown> = {}
  for (const column of meta.columns) {
    const value = row[column.name]
    if (value === undefined) {
      if (column.notNull && !column.hasDefault) throw bad(`${meta.name} ${localId}: ${column.name} is required`)
      continue
    }
    if (value === null) {
      if (column.notNull && !column.hasDefault) throw bad(`${meta.name} ${localId}: ${column.name} is required`)
      if (column.notNull) continue
      values[column.key] = null
      continue
    }
    if (column.dataType === `date`) {
      const date = typeof value === `string` || typeof value === `number` ? new Date(value) : value
      if (!(date instanceof Date) || Number.isNaN(date.getTime())) throw bad(`${meta.name} ${localId}: ${column.name} is not a timestamp`)
      values[column.key] = date
      continue
    }
    values[column.key] = value
  }
  return values
}

async function existingIds(tx: Tx, meta: TableMeta, ids: string[]): Promise<Set<string>> {
  const found = new Set<string>()
  const column = (meta.table as unknown as Record<string, unknown>).id as PgColumn
  for (let i = 0; i < ids.length; i += 500) {
    const slice = ids.slice(i, i + 500)
    const rows = (await tx.select({ id: column }).from(meta.table).where(inArray(column, slice))) as { id: string }[]
    for (const r of rows) found.add(r.id)
  }
  return found
}

/** Collisions of this table's ids with rows already on the server: each
 *  gets a fresh uuid in the map. */
export async function remapCollisions(tx: Tx, meta: TableMeta, rows: StagedRow[], ids: IdMap): Promise<number> {
  if (!meta.hasId || rows.length === 0) return 0
  const wanted = rows.map((r) => r.localId).filter((id) => ids.get(meta.name, id) === id)
  const taken = await existingIds(tx, meta, wanted)
  for (const id of taken) ids.set(meta.name, id, randomUUID())
  return taken.size
}

async function insertRows(tx: Tx, meta: TableMeta, values: Record<string, unknown>[]): Promise<void> {
  for (let i = 0; i < values.length; i += BATCH) {
    await tx.insert(meta.table).values(values.slice(i, i + BATCH) as never)
  }
}

async function writeDeferred(tx: Tx, meta: TableMeta, deferred: { id: string; columns: Record<string, string> }[]): Promise<void> {
  const column = (meta.table as unknown as Record<string, unknown>).id as PgColumn
  for (const entry of deferred) {
    const set: Record<string, unknown> = {}
    for (const [name, value] of Object.entries(entry.columns)) {
      const key = meta.columns.find((c) => c.name === name)?.key ?? name
      set[key] = value
    }
    await tx.update(meta.table).set(set).where(eq(column, entry.id))
  }
}

export interface ApplyContext {
  tx: Tx
  teamId: string
  identity: ImportIdentity
  deviceId: string | null
  now: Date
  ids: IdMap
  /** server issue id → its board id (comments, labels, attachments carry it) */
  issueBoards: Map<string, string>
  remapped: number
}

/** A generic table: collisions, plan, insert, then the self references. */
async function applyGeneric(ctx: ApplyContext, meta: TableMeta, rows: StagedRow[], prepare?: (row: Record<string, unknown>, localId: string) => Record<string, unknown>): Promise<void> {
  ctx.remapped += await remapCollisions(ctx.tx, meta, rows, ctx.ids)
  const values: Record<string, unknown>[] = []
  const deferred: { id: string; columns: Record<string, string> }[] = []
  for (const staged of rows) {
    const planned = planRow(meta, staged.row, ctx.teamId, ctx.identity, ctx.ids)
    ctx.remapped += planned.remapped
    let row = planned.row
    if (meta.hasId) row.id = ctx.ids.get(meta.name, staged.localId)
    if (prepare) row = prepare(row, staged.localId)
    values.push(convertRow(meta, row, staged.localId))
    if (Object.keys(planned.deferred).length > 0 && typeof row.id === `string`) deferred.push({ id: row.id, columns: planned.deferred })
  }
  await insertRows(ctx.tx, meta, values)
  await writeDeferred(ctx.tx, meta, deferred)
}

function boardOfIssue(ctx: ApplyContext, row: Record<string, unknown>, what: string, localId: string): string {
  const issueId = row.issue_id
  const board = typeof issueId === `string` ? ctx.issueBoards.get(issueId) : undefined
  if (!board) throw bad(`${what} ${localId}: its issue is not part of the import`)
  return board
}

async function applyStatuses(ctx: ApplyContext, meta: TableMeta, rows: StagedRow[]): Promise<void> {
  const team = (await ctx.tx
    .select({ id: issueStatuses.id, category: issueStatuses.category, name: issueStatuses.name, sortOrder: issueStatuses.sortOrder, builtinKey: issueStatuses.builtinKey })
    .from(issueStatuses)
    .where(eq(issueStatuses.teamId, ctx.teamId))) as StatusLike[]
  const local: StatusLike[] = rows.map((r) => ({
    id: r.localId,
    category: String(r.row.category ?? ``),
    name: String(r.row.name ?? ``),
    sortOrder: typeof r.row.sort_order === `number` ? r.row.sort_order : 0,
    builtinKey: typeof r.row.builtin_key === `string` ? r.row.builtin_key : null,
  }))
  const match = matchStatuses(local, team)
  for (const [localId, serverId] of Object.entries(match.map)) ctx.ids.set(meta.name, localId, serverId)
  const create = rows.filter((r) => match.create.includes(r.localId))
  const started = team.filter((t) => t.category === `started`).length + create.filter((r) => r.row.category === `started`).length
  if (started > ISSUE_STATUS_STARTED_MAX) throw bad(`A team can have at most ${ISSUE_STATUS_STARTED_MAX} started statuses`)
  await applyGeneric(ctx, meta, create, (row) => ({ ...row, builtin_key: null }))
}

async function applySessions(ctx: ApplyContext, meta: TableMeta, rows: StagedRow[]): Promise<void> {
  // Roots before their children: `root_id` is a required self reference.
  const byDepth = [...rows].sort((a, b) => Number(a.row.depth ?? 0) - Number(b.row.depth ?? 0))
  ctx.remapped += await remapCollisions(ctx.tx, meta, byDepth, ctx.ids)
  let depth = -1
  let batch: StagedRow[] = []
  const flush = async () => {
    if (batch.length === 0) return
    const chunk = batch
    batch = []
    await applyGeneric(ctx, meta, chunk, (row) => sessionArrival({ ...row, background: false }, ctx.now.toISOString()))
  }
  for (const staged of byDepth) {
    const d = Number(staged.row.depth ?? 0)
    if (d !== depth) {
      await flush()
      depth = d
    }
    batch.push(staged)
  }
  await flush()
}

async function applyConversations(ctx: ApplyContext, meta: TableMeta, rows: StagedRow[]): Promise<void> {
  // A session's conversation is the one the server made on the session's
  // insert: the local id maps to it. The others (dm, channel) are inserted.
  const ofSessions = rows.filter((r) => r.row.kind === `session` && typeof r.row.session_id === `string`)
  const sessionIds = ofSessions.map((r) => ctx.ids.get(`sessions`, r.row.session_id as string))
  const made = new Map<string, string>()
  for (let i = 0; i < sessionIds.length; i += 500) {
    const slice = sessionIds.slice(i, i + 500)
    if (slice.length === 0) continue
    const found = await ctx.tx.select({ id: conversations.id, sessionId: conversations.sessionId }).from(conversations).where(inArray(conversations.sessionId, slice))
    for (const c of found) if (c.sessionId) made.set(c.sessionId, c.id)
  }
  for (const staged of ofSessions) {
    const serverSession = ctx.ids.get(`sessions`, staged.row.session_id as string)
    const conversation = made.get(serverSession)
    if (!conversation) throw bad(`conversations ${staged.localId}: its session is not part of the import`)
    if (conversation !== staged.localId) ctx.ids.set(meta.name, staged.localId, conversation)
  }
  await applyGeneric(ctx, meta, rows.filter((r) => !ofSessions.includes(r)))
}

async function applyAttachments(ctx: ApplyContext, meta: TableMeta, rows: StagedRow[]): Promise<number> {
  // The signed upload landed the rows (under the import's own id map, made
  // at chunk time); the commit binds them to what they belong to and gives
  // them their local stamps. A row whose bytes never landed fails here.
  const serverIds = rows.map((r) => ctx.ids.get(meta.name, r.localId))
  const landed = new Map<string, number>()
  for (let i = 0; i < serverIds.length; i += 500) {
    const slice = serverIds.slice(i, i + 500)
    if (slice.length === 0) continue
    const found = await ctx.tx
      .select({ id: attachments.id, sizeBytes: attachments.sizeBytes })
      .from(attachments)
      .where(and(inArray(attachments.id, slice), eq(attachments.teamId, ctx.teamId)))
    for (const a of found) landed.set(a.id, Number(a.sizeBytes))
  }
  const missing = rows.filter((r) => !landed.has(ctx.ids.get(meta.name, r.localId))).map((r) => r.localId)
  if (missing.length > 0) {
    const shown = missing.slice(0, 5).join(`, `)
    throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `${missing.length} attachment(s) never landed: ${shown}${missing.length > 5 ? `, …` : ``}` })
  }
  let bytes = 0
  for (const staged of rows) {
    const planned = planRow(meta, staged.row, ctx.teamId, ctx.identity, ctx.ids)
    ctx.remapped += planned.remapped
    const row = planned.row
    const serverId = ctx.ids.get(meta.name, staged.localId)
    bytes += landed.get(serverId) ?? 0
    const set: Record<string, unknown> = {
      issueId: typeof row.issue_id === `string` ? row.issue_id : null,
      boardId: typeof row.issue_id === `string` ? boardOfIssue(ctx, row, meta.name, staged.localId) : null,
      commentId: typeof row.comment_id === `string` ? row.comment_id : null,
      messageId: typeof row.message_id === `string` ? row.message_id : null,
      appId: typeof row.app_id === `string` ? row.app_id : null,
      asFile: row.as_file === true,
    }
    if (typeof row.created_at === `string`) set.createdAt = new Date(row.created_at)
    if (typeof row.updated_at === `string`) set.updatedAt = new Date(row.updated_at)
    await ctx.tx.update(attachments).set(set).where(eq(attachments.id, serverId))
  }
  return bytes
}

/** App records go through the one records service (ids and stamps kept,
 *  a taken id remapped by it, the quota and the events its own). */
async function applyRecords(ctx: ApplyContext, rows: StagedRow[]): Promise<void> {
  const byApp = new Map<string, StagedRow[]>()
  for (const staged of rows) {
    const app = staged.row.app_id
    if (typeof app !== `string`) throw bad(`app_records ${staged.localId}: app_id is required`)
    const serverApp = ctx.ids.get(`apps`, app)
    byApp.set(serverApp, [...(byApp.get(serverApp) ?? []), staged])
  }
  for (const [appId, list] of byApp) {
    const [app] = await ctx.tx
      .select({ id: apps.id, teamId: apps.teamId, package: apps.package, draftPackage: apps.draftPackage, publishedVersion: apps.publishedVersion })
      .from(apps)
      .where(and(eq(apps.id, appId), eq(apps.teamId, ctx.teamId)))
      .limit(1)
    if (!app) throw bad(`app_records ${list[0]!.localId}: its app is not part of the import`)
    const records = list.map((staged) => {
      const row = staged.row
      const author = mapUserId(typeof row.author_id === `string` ? row.author_id : typeof row.created_by === `string` ? row.created_by : null, ctx.identity.localUserId, ctx.identity.callerId, true)
      if (typeof row.kind !== `string` || row.data === null || typeof row.data !== `object`) throw bad(`app_records ${staged.localId}: kind and data are required`)
      const stamp = (v: unknown) => (typeof v === `string` && !Number.isNaN(new Date(v).getTime()) ? new Date(v) : undefined)
      return { id: staged.localId, kind: row.kind, data: row.data as Record<string, unknown>, author: author as string, createdAt: stamp(row.created_at), updatedAt: stamp(row.updated_at) }
    })
    const remapped = await importRecords(ctx.tx, app as Pick<App, `id` | `teamId` | `package`> & Partial<Pick<App, `publishedVersion`>>, ctx.identity.callerId, `owner`, records)
    for (const [localId, serverId] of Object.entries(remapped)) {
      ctx.ids.set(`app_records`, localId, serverId)
      ctx.remapped += 1
    }
  }
}

export interface ApplyResult {
  remapped: number
  attachmentBytes: number
  counts: Record<string, number>
}

/** Applies every staged row (grouped by table, in `tables` order) inside
 *  `tx`, which already has the preserve-timestamps guard on. */
export async function applyImport(
  tx: Tx,
  args: { importId: string; teamId: string; identity: ImportIdentity; deviceId: string | null; rows: StagedRow[]; ids: IdMap; now?: Date }
): Promise<ApplyResult> {
  const ctx: ApplyContext = {
    tx,
    teamId: args.teamId,
    identity: args.identity,
    deviceId: args.deviceId,
    now: args.now ?? new Date(),
    ids: args.ids,
    issueBoards: new Map(),
    remapped: 0,
  }
  const byTable = new Map<string, StagedRow[]>()
  for (const row of args.rows) {
    const list = byTable.get(row.tableName) ?? []
    list.push(row)
    byTable.set(row.tableName, list)
  }
  const counts: Record<string, number> = {}
  let attachmentBytes = 0
  for (const name of TEAM_IMPORT_TABLES) {
    const rows = byTable.get(name) ?? []
    counts[name] = rows.length
    if (rows.length === 0) continue
    if (!(name in IMPORT_TABLES)) {
      if (name === `app_records`) await applyRecords(ctx, rows)
      else throw bad(`not an import table: ${name}`)
      continue
    }
    const meta = tableMeta(name)
    switch (meta.name) {
      case `issue_statuses`:
        await applyStatuses(ctx, meta, rows)
        break
      case `issues`:
        await applyGeneric(ctx, meta, rows, (row, localId) => {
          const board = row.board_id
          if (typeof board !== `string`) throw bad(`issues ${localId}: board_id is required`)
          ctx.issueBoards.set(row.id as string, board)
          const status = row.status_id
          if (typeof status === `string` && ctx.ids.get(`issue_statuses`, status) === status && !args.rows.some((r) => r.tableName === `issue_statuses` && r.localId === status)) {
            // A status the local file never listed: let the trigger derive it from the anchor.
            return { ...row, status_id: null }
          }
          return row
        })
        break
      case `issue_labels`:
      case `issue_relations`:
      case `comments`:
        await applyGeneric(ctx, meta, rows, (row, localId) => ({ ...row, board_id: boardOfIssue(ctx, row, meta.name, localId) }))
        break
      case `agents`:
        await applyGeneric(ctx, meta, rows, (row) => ({ ...row, device_id: ctx.deviceId ?? row.device_id }))
        break
      case `sessions`:
        await applySessions(ctx, meta, rows)
        break
      case `conversations`:
        await applyConversations(ctx, meta, rows)
        break
      case `attachments`:
        attachmentBytes = await applyAttachments(ctx, meta, rows)
        break
      default:
        await applyGeneric(ctx, meta, rows)
    }
  }
  // Only one agent of a team may be the owner's assistant: the first stays.
  await tx.execute(sql`
    UPDATE agents SET role = 'agent'
    WHERE team_id = ${args.teamId} AND role = 'assistant'
      AND id <> (SELECT id FROM agents WHERE team_id = ${args.teamId} AND role = 'assistant' ORDER BY created_at, id LIMIT 1)
  `)
  const fresh = ctx.ids.entries.filter((e) => e.serverId !== e.localId)
  for (let i = 0; i < fresh.length; i += BATCH) {
    await tx
      .insert(importIdMap)
      .values(fresh.slice(i, i + BATCH).map((e) => ({ importId: args.importId, tableName: e.tableName, localId: e.localId, serverId: e.serverId })))
      .onConflictDoNothing()
  }
  return { remapped: ctx.remapped, attachmentBytes, counts }
}
