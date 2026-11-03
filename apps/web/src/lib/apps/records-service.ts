// #VAPP-193: the ONE app-records service. Every server-side read and write of
// `app_records` runs through here: tRPC `apps.records.*` (and with it the MCP
// apps noun's `records_*` ops), the apps broker (`/api/mcp/apps`) and the
// `.app.json` import. Export is client-side (the synced shape); the watchdog
// purge (`lib/app-records-purge.ts`) and the plan quota count
// (`lib/billing.ts`) are admin reads that keep the owner role.
//
// Checks stay here, in order: the app, membership, the kind's schema, the
// record cap, ownership (#VAPP-187), on-behalf (#VAPP-188), the team quota.
// Everything that reads ANOTHER table runs first; then `asEapps` switches the
// transaction to the NOLOGIN role `eapps` (grants on `app_records` only) with
// its RLS scope (team + apps) and a 2 s statement timeout, the Postgres
// backstop behind these checks (docs/spec/apps.md §3.4, §12.2). The
// structured query maps onto a parameterised Drizzle query over a fixed
// column allowlist: app text never becomes SQL.
// #VAPP-215 (R4): every write appends its `app_record_events` row in the
// same transaction under the same role (`logEvents`); history, restore and
// undo read those rows back (app-records.json `history`).
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, inArray, isNull, notInArray, or, sql, type SQL } from "drizzle-orm"
import { validateRecord, type RecordSchema } from "@exponential_app/eui"
import { RECORD_POLICY_COPY, recordPolicy, recordRefusal, type AppAccessLevel, type RecordOp, type RecordViewer } from "@exponential_app/eui/host"
import { kindComputed, parseRecordsJson, recordsValue, withComputed, type RecordRow, type RecordsQueryIssue, type RecordsValue } from "@exponential_app/eui/host/query"
import { APP_RECORD_MAX_BYTES } from "@exp/domain-contract/apps"
import { APP_RECORD_HISTORY, APP_RECORDS_ON_BEHALF, eventState, inverseWrite, recordAuthor, writeAsVerdict, type RecordEventOp, type RecordEventVia } from "@exp/domain-contract/app-records"
import { db as defaultDb } from "@/db/connection"
import { appAccess, appRecordEvents, appRecords, apps, teamMembers, users, type App } from "@/db/schema"
import { assertTeamMember } from "@/lib/auth/membership"
import { assertCanCreateAppRecord } from "@/lib/billing"
import { packageBytes, recordKindSchema } from "@/lib/apps/package"
import { generateTxId } from "@/lib/trpc"
import { PLAN_LIMITS } from "@/lib/plan-limits"
import type { Db, Tx } from "@/lib/sessions/tx"

/** The role a records statement runs as (custom SQL: `eapps` + its policy). */
export const RECORDS_ROLE = `eapps`
/** A records statement's ceiling under the role. */
export const RECORDS_STATEMENT_TIMEOUT = `2s`

/** What a records transaction may touch: one team, these apps; `write` =
 *  it may insert or update (eApps R3: only a member with write access to
 *  the app; the policy `eapps_write` refuses anything else). */
export interface RecordsScope {
  teamId: string
  appIds: readonly string[]
  write?: boolean
}

/** One app by id or by (team, slug). */
export interface AppRef {
  id?: string
  teamId?: string
  slug?: string
}

type Executor = typeof defaultDb

/** Switches `tx` to role `eapps` scoped to `scope` (SET LOCAL: it ends with
 *  the transaction), then runs `fn`. Read every other table BEFORE this:
 *  under the role they answer permission denied. */
export async function asEapps<T>(tx: Tx, scope: RecordsScope, fn: (tx: Tx) => Promise<T>): Promise<T> {
  await tx.execute(
    sql`SELECT set_config('eapps.team', ${scope.teamId}, true), set_config('eapps.apps', ${scope.appIds.join(`,`)}, true), set_config('eapps.write', ${scope.write ? `on` : `off`}, true), set_config('statement_timeout', ${RECORDS_STATEMENT_TIMEOUT}, true)`
  )
  await tx.execute(sql.raw(`SET LOCAL ROLE ${RECORDS_ROLE}`))
  return fn(tx)
}

/** A transaction of its own, entirely under the role. */
export function inRecordsScope<T>(scope: RecordsScope, fn: (tx: Tx) => Promise<T>, executor: Executor = defaultDb): Promise<T> {
  return executor.transaction((tx) => asEapps(tx, scope, fn))
}

/** The app a records call names (live only). */
export async function loadApp(tx: Db, ref: AppRef): Promise<App> {
  const where = ref.id ? eq(apps.id, ref.id) : and(eq(apps.teamId, ref.teamId!), eq(apps.slug, ref.slug!))
  const [app] = await tx.select().from(apps).where(and(where, isNull(apps.deletedAt))).limit(1)
  if (!app) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  return app
}

/** VAPP-188: the team's member ids (an on-behalf author must be one). */
export async function teamMemberIds(tx: Pick<Tx, `select`>, teamId: string): Promise<string[]> {
  const rows = await tx.select({ userId: teamMembers.userId }).from(teamMembers).where(eq(teamMembers.teamId, teamId))
  return rows.map((r) => r.userId)
}

/** VAPP-188: may the writer name `author` (app-records.json `onBehalf`)?
 *  Another member = owners only (FORBIDDEN), and a member (BAD_REQUEST).
 *  `members` = the team's ids, read before the role switch (owners only). */
function assertCanWriteAs(userId: string, role: string, author: string | undefined, members: readonly string[]) {
  if (author === undefined || author === userId) return
  const verdict = writeAsVerdict(userId, role, author, role === `owner` ? members : [])
  if (verdict === `forbidden`) throw new TRPCError({ code: `FORBIDDEN`, message: APP_RECORDS_ON_BEHALF.refusal })
  if (verdict === `notMember`) throw new TRPCError({ code: `BAD_REQUEST`, message: APP_RECORDS_ON_BEHALF.notMember })
}

/** eApps R3: the member's access level for the app (no row = write). Read
 *  BEFORE the role switch. */
export async function appAccessLevel(executor: Pick<Tx, `select`>, appId: string, userId: string): Promise<AppAccessLevel> {
  const [row] = await executor.select({ level: appAccess.level }).from(appAccess).where(and(eq(appAccess.appId, appId), eq(appAccess.userId, userId))).limit(1)
  return (row?.level as AppAccessLevel | undefined) ?? `write`
}

/** eApps R3: who is asking (membership + access); access `none` hides the
 *  app (NOT_FOUND, as if it did not exist). */
async function viewerOf(executor: Pick<Tx, `select`>, app: App, userId: string): Promise<RecordViewer & { id: string; role: string }> {
  const member = await assertTeamMember(userId, app.teamId)
  const access = await appAccessLevel(executor, app.id, userId)
  if (access === `none`) throw new TRPCError({ code: `NOT_FOUND`, message: `App not found` })
  return { id: userId, role: member.role, access }
}

/** eApps R3: a write needs write access (before any locked read: under
 *  `eapps.write = off` a FOR UPDATE read sees no row). */
function assertCanWrite(viewer: RecordViewer) {
  if ((viewer.access ?? `write`) !== `write`) throw new TRPCError({ code: `FORBIDDEN`, message: RECORD_POLICY_COPY.readOnly })
}

type RecordsApp = Pick<App, `package`> & Partial<Pick<App, `draftPackage`>>
const recordsOf = (pkg: unknown): Record<string, unknown> => {
  const records = (pkg as { records?: unknown } | null)?.records
  return records && typeof records === `object` && !Array.isArray(records) ? (records as Record<string, unknown>) : {}
}

/** eApps V1 (#VAPP-210): the record kinds a write or read is checked
 *  against: the PUBLISHED package's, plus a draft's kinds the published one
 *  does not declare yet (an editor seeds them before publishing); a kind in
 *  both keeps the published definition (its policy). */
function effectiveRecords(app: RecordsApp): Record<string, unknown> {
  return { ...recordsOf(app.draftPackage), ...recordsOf(app.package) }
}

/** eApps V1: the schemas a record of `kind` may satisfy, the published one
 *  first, then the draft's (a draft that reshapes a kind accepts records of
 *  either shape until it is published). */
function kindSchemas(app: RecordsApp, kind: string): RecordSchema[] {
  return [recordKindSchema(app.package, kind), recordKindSchema(app.draftPackage, kind)].filter((schema): schema is RecordSchema => schema !== null)
}

/** A record's data against the first schema it satisfies (the published
 *  one's issues when none). */
function checkedAgainst(schemas: readonly RecordSchema[], data: Record<string, unknown>): Record<string, unknown> {
  const fits = schemas.find((schema) => validateRecord(schema, data).length === 0)
  return checkedRecordData(fits ?? schemas[0]!, data)
}

/** VAPP-214 (Q3): the rows with their kind's computed fields
 *  (`records.<kind>.computed`) written into `data`: eUI's evaluator, the
 *  same as every client host's records query; evaluated on read, never stored. */
function withComputedFields<R extends { id: string; kind?: string; data: unknown; createdBy: string | null; authorId: string | null; createdAt: Date; updatedAt: Date }>(app: RecordsApp, rows: R[], kind?: string): R[] {
  const records = effectiveRecords(app)
  let evaluated: readonly RecordRow[] = rows.map((row) => ({
    id: row.id,
    kind: row.kind ?? kind ?? ``,
    data: row.data && typeof row.data === `object` && !Array.isArray(row.data) ? (row.data as Record<string, unknown>) : {},
    createdBy: row.createdBy,
    authorId: row.authorId,
    createdAt: row.createdAt.toISOString(),
    updatedAt: row.updatedAt.toISOString(),
  }))
  const now = new Date().toISOString()
  for (const k of new Set(evaluated.map((r) => r.kind))) evaluated = withComputed(evaluated, k, kindComputed(records, k), now)
  return rows.map((row, i) => (evaluated[i]!.data === row.data ? row : { ...row, data: evaluated[i]!.data }))
}

/** A kind's effective policy (record-policy.json). */
const kindPolicy = (app: RecordsApp, kind: string) => recordPolicy(effectiveRecords(app)[kind])

/** eApps R3: refuse what the kind's policy and the member's access refuse
 *  (record-policy.json `copy` = the FORBIDDEN message). */
function assertRecordOp(app: RecordsApp, kind: string, op: RecordOp, viewer: RecordViewer, row?: { authorId: string | null; createdBy: string | null }) {
  const refusal = recordRefusal(kindPolicy(app, kind), op, viewer, row)
  if (refusal) throw new TRPCError({ code: `FORBIDDEN`, message: refusal })
}

/** A row the viewer may not read is not there for them (NOT_FOUND, no leak). */
function assertReadable(app: RecordsApp, viewer: RecordViewer, row: { kind: string; authorId: string | null; createdBy: string | null }) {
  if (recordRefusal(kindPolicy(app, row.kind), `read`, viewer, row)) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
}

/** eApps R3: the SQL twin of the read policy for one app (a parameterised
 *  condition, undefined = every row): a non-owner never reads an `owners`
 *  kind, and reads an `author` kind's rows only where they are the author. */
export function readPolicyWhere(app: RecordsApp, viewer: { id: string; role: string }): SQL | undefined {
  if (viewer.role === `owner`) return undefined
  const kinds = Object.keys(effectiveRecords(app))
  const owners = kinds.filter((k) => kindPolicy(app, k).read === `owners`)
  const author = kinds.filter((k) => kindPolicy(app, k).read === `author`)
  const mine = sql`coalesce(${appRecords.authorId}, ${appRecords.createdBy}) = ${viewer.id}`
  return and(owners.length ? notInArray(appRecords.kind, owners) : undefined, author.length ? or(notInArray(appRecords.kind, author), mine) : undefined)
}

/** The team's record quota, behind the team's lock, in `tx` and BEFORE the
 *  role switch (the count spans every app of the team). */
async function lockAndCheckQuota(tx: Tx, teamId: string, adding = 1) {
  await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext(${`app_records:${teamId}`}))`)
  await assertCanCreateAppRecord(teamId, tx, adding)
}

/** A record's data against its kind's schema and the per-record cap
 *  (BAD_REQUEST naming every issue, up to ten). */
function checkedRecordData(schema: RecordSchema, data: Record<string, unknown>): Record<string, unknown> {
  const issues = validateRecord(schema, data)
  if (issues.length > 0) {
    const listed = issues.slice(0, 10).map((issue) => `${issue.path || `/`}: ${issue.message}`)
    throw new TRPCError({ code: `BAD_REQUEST`, message: `record is invalid: ${listed.join(`; `)}` })
  }
  const bytes = packageBytes(data)
  if (bytes > APP_RECORD_MAX_BYTES) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `record is ${bytes} bytes; the cap is ${APP_RECORD_MAX_BYTES}` })
  }
  return data
}

/** The columns a records read returns (the allowlist; nothing else is read). */
const RECORD_COLUMNS = {
  id: appRecords.id,
  kind: appRecords.kind,
  data: appRecords.data,
  createdBy: appRecords.createdBy,
  authorId: appRecords.authorId,
  createdAt: appRecords.createdAt,
  updatedAt: appRecords.updatedAt,
}
const { kind: _kind, ...BROKER_COLUMNS } = RECORD_COLUMNS
const newestFirst = [desc(appRecords.createdAt), asc(appRecords.id)] as const

/** VAPP-215: where a write came from (app-records.json `history.vias`). */
export type RecordWriteVia = RecordEventVia

/** One history row to append (app-records.json `history`). */
interface EventInput {
  recordId: string
  kind: string
  op: RecordEventOp
  author: string | null
  patch: Record<string, unknown> | null
  before: Record<string, unknown> | null
}

/** VAPP-215: append the writes' events in `tx` (already under the role,
 *  `eapps.write = on`); their ids in order. */
async function logEvents(tx: Tx, app: Pick<App, `id` | `teamId`>, actor: string, via: RecordWriteVia, events: readonly EventInput[]): Promise<string[]> {
  if (events.length === 0) return []
  const rows = await tx
    .insert(appRecordEvents)
    .values(events.map((e) => ({ ...e, appId: app.id, teamId: app.teamId, actor, via })))
    .returning({ id: appRecordEvents.id })
  return rows.map((r) => r.id)
}

// ── tRPC `apps.records.*` (and the MCP records_* ops through it) ──────────

/** Any member lists an app's live records, newest first. */
export async function listRecords(
  userId: string,
  ref: AppRef,
  query: { kind?: string; limit: number; offset: number },
  executor: Executor = defaultDb
) {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  const rows = await inRecordsScope(
    { teamId: app.teamId, appIds: [app.id] },
    (tx) =>
      tx
        .select(RECORD_COLUMNS)
        .from(appRecords)
        .where(and(eq(appRecords.appId, app.id), isNull(appRecords.deletedAt), query.kind ? eq(appRecords.kind, query.kind) : undefined, readPolicyWhere(app, viewer)))
        .orderBy(...newestFirst)
        .limit(query.limit)
        .offset(query.offset),
    executor
  )
  return { records: withComputedFields(app, rows) }
}

/** Create a record, or (with `recordId`) patch that record's data (shallow).
 *  VAPP-188: `author` = an on-behalf write (owners only): the create's
 *  author, or an update's reassignment; created_by/updated_by stay the real
 *  writer. */
export async function upsertRecord(
  userId: string,
  ref: AppRef,
  input: { kind: string; data: Record<string, unknown>; recordId?: string; author?: string; via?: RecordWriteVia },
  executor: Executor = defaultDb
): Promise<{ txid: number; id: string; created: boolean; eventId: string }> {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  assertCanWrite(viewer)
  const member = { role: viewer.role }
  const schemas = kindSchemas(app, input.kind)
  if (schemas.length === 0) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The app declares no record kind "${input.kind}": add it to the package's records first` })
  }
  const checked = (data: Record<string, unknown>) => checkedAgainst(schemas, data)
  // VAPP-188: an on-behalf author must be a member (team_members: read now).
  const onBehalf = input.author !== undefined && input.author !== userId && member.role === `owner`
  const members = onBehalf ? await teamMemberIds(executor, app.teamId) : []
  const scope = { teamId: app.teamId, appIds: [app.id], write: viewer.access === `write` }
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    if (input.recordId) {
      const recordId = input.recordId
      return asEapps(tx, scope, async (tx) => {
        // Release train 2026-10-25: the patch merges onto the row as locked
        // in THIS transaction, so two concurrent patches queue here and
        // neither loses the other's fields.
        const [existing] = await tx
          .select({ id: appRecords.id, kind: appRecords.kind, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
          .from(appRecords)
          .where(and(eq(appRecords.id, recordId), eq(appRecords.appId, app.id), eq(appRecords.kind, input.kind), isNull(appRecords.deletedAt)))
          .limit(1)
          .for(`update`)
        if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
        assertReadable(app, viewer, existing)
        assertRecordOp(app, input.kind, `update`, viewer, existing)
        // VAPP-188: an owner may REASSIGN the row (`author` ≠ its author).
        const reassign = input.author !== undefined && input.author !== recordAuthor(existing)
        if (reassign) assertCanWriteAs(userId, member.role, input.author, members)
        const data = checked({ ...existing.data, ...input.data })
        await tx
          .update(appRecords)
          .set({ data, ...(reassign ? { authorId: input.author } : {}), updatedBy: userId, updatedAt: new Date() })
          .where(eq(appRecords.id, existing.id))
        const [eventId] = await logEvents(tx, app, userId, input.via ?? `app`, [
          { recordId: existing.id, kind: existing.kind, op: `update`, author: reassign ? input.author! : recordAuthor(existing), patch: input.data, before: existing.data },
        ])
        return { txid: txId, id: existing.id, created: false, eventId: eventId! }
      })
    }
    assertRecordOp(app, input.kind, `create`, viewer)
    const data = checked(input.data)
    // VAPP-188: an on-behalf create (owners only; the author a member).
    assertCanWriteAs(userId, member.role, input.author, members)
    // The team's quota, counted in THIS transaction behind the team's lock:
    // concurrent creates at the limit queue here and the second one sees the
    // first's row (no race past `appRecords`).
    await lockAndCheckQuota(tx, app.teamId)
    return asEapps(tx, scope, async (tx) => {
      const [row] = await tx
        .insert(appRecords)
        .values({ appId: app.id, teamId: app.teamId, kind: input.kind, data, createdBy: userId, authorId: input.author ?? userId, updatedBy: userId })
        .returning({ id: appRecords.id })
      const [eventId] = await logEvents(tx, app, userId, input.via ?? `app`, [
        { recordId: row!.id, kind: input.kind, op: `create`, author: input.author ?? userId, patch: data, before: null },
      ])
      return { txid: txId, id: row!.id, created: true, eventId: eventId! }
    })
  })
}

/** Soft-delete one record (its author or a team owner). */
export async function deleteRecord(
  userId: string,
  ref: AppRef,
  recordId: string,
  executor: Executor = defaultDb,
  via: RecordWriteVia = `app`
): Promise<{ txid: number; eventId: string }> {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  assertCanWrite(viewer)
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    return asEapps(tx, { teamId: app.teamId, appIds: [app.id], write: viewer.access === `write` }, async (tx) => {
      const [existing] = await tx
        .select({ id: appRecords.id, kind: appRecords.kind, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
        .from(appRecords)
        .where(and(eq(appRecords.id, recordId), eq(appRecords.appId, app.id), isNull(appRecords.deletedAt)))
        .limit(1)
        .for(`update`)
      if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
      assertReadable(app, viewer, existing)
      assertRecordOp(app, existing.kind, `delete`, viewer, existing)
      await tx
        .update(appRecords)
        .set({ deletedAt: sql`now()`, updatedBy: userId, updatedAt: new Date() })
        .where(eq(appRecords.id, existing.id))
      const [eventId] = await logEvents(tx, app, userId, via, [
        { recordId: existing.id, kind: existing.kind, op: `delete`, author: recordAuthor(existing), patch: null, before: existing.data },
      ])
      return { txid: txId, eventId: eventId! }
    })
  })
}

/** VAPP-194: one write of a package action's plan (`@exponential_app/eui`
 *  `ActionWrite`): a create, a patch (`id`) or a delete. */
export type RecordBatchWrite = { op: `upsert`; kind: string; data: Record<string, unknown>; id?: string } | { op: `delete`; id: string }

/** VAPP-194 (eApps E1): a package action's record writes as ONE Postgres
 *  transaction, all or nothing (docs/spec/apps.md §4.5): every write runs the
 *  same checks as `upsertRecord` / `deleteRecord` (the kind's schema, the
 *  record cap, ownership; the team quota for all the creates at once), in
 *  order; the first refusal (`write <i>: …`) rolls the whole batch back. The
 *  server stays the authority: the client's `require` steps guard nothing
 *  here. No on-behalf writes (an action writes as its member). */
export async function commitRecordBatch(
  userId: string,
  ref: AppRef,
  writes: readonly RecordBatchWrite[],
  executor: Executor = defaultDb
): Promise<{ txid: number; ids: string[]; eventIds: string[] }> {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  assertCanWrite(viewer)
  const at = (i: number, e: unknown): never => {
    if (e instanceof TRPCError) throw new TRPCError({ code: e.code, message: `write ${i}: ${e.message}` })
    throw e
  }
  const schemas = writes.map((w, i) => {
    if (w.op !== `upsert`) return null
    const schemas = kindSchemas(app, w.kind)
    if (schemas.length === 0) at(i, new TRPCError({ code: `BAD_REQUEST`, message: `The app declares no record kind "${w.kind}": add it to the package's records first` }))
    return schemas
  })
  const creates = writes.filter((w) => w.op === `upsert` && w.id === undefined).length
  const scope = { teamId: app.teamId, appIds: [app.id], write: viewer.access === `write` }
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    if (creates > 0) await lockAndCheckQuota(tx, app.teamId, creates)
    return asEapps(tx, scope, async (tx) => {
      const ids: string[] = []
      const events: EventInput[] = []
      for (const [i, w] of writes.entries()) {
        try {
          if (w.op === `delete`) {
            const [existing] = await tx
              .select({ id: appRecords.id, kind: appRecords.kind, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
              .from(appRecords)
              .where(and(eq(appRecords.id, w.id), eq(appRecords.appId, app.id), isNull(appRecords.deletedAt)))
              .limit(1)
              .for(`update`)
            if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
            assertReadable(app, viewer, existing)
            assertRecordOp(app, existing.kind, `delete`, viewer, existing)
            await tx.update(appRecords).set({ deletedAt: sql`now()`, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
            ids.push(existing.id)
            events.push({ recordId: existing.id, kind: existing.kind, op: `delete`, author: recordAuthor(existing), patch: null, before: existing.data })
            continue
          }
          const schema = schemas[i]!
          if (w.id !== undefined) {
            const [existing] = await tx
              .select({ id: appRecords.id, kind: appRecords.kind, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
              .from(appRecords)
              .where(and(eq(appRecords.id, w.id), eq(appRecords.appId, app.id), eq(appRecords.kind, w.kind), isNull(appRecords.deletedAt)))
              .limit(1)
              .for(`update`)
            if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
            assertReadable(app, viewer, existing)
            assertRecordOp(app, w.kind, `update`, viewer, existing)
            const data = checkedAgainst(schema, { ...existing.data, ...w.data })
            await tx.update(appRecords).set({ data, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
            ids.push(existing.id)
            events.push({ recordId: existing.id, kind: existing.kind, op: `update`, author: recordAuthor(existing), patch: w.data, before: existing.data })
            continue
          }
          assertRecordOp(app, w.kind, `create`, viewer)
          const data = checkedAgainst(schema, w.data)
          const [row] = await tx
            .insert(appRecords)
            .values({ appId: app.id, teamId: app.teamId, kind: w.kind, data, createdBy: userId, authorId: userId, updatedBy: userId })
            .returning({ id: appRecords.id })
          ids.push(row!.id)
          events.push({ recordId: row!.id, kind: w.kind, op: `create`, author: userId, patch: data, before: null })
        } catch (e) {
          at(i, e)
        }
      }
      const eventIds = await logEvents(tx, app, userId, `action`, events)
      return { txid: txId, ids, eventIds }
    })
  })
}

// ── VAPP-215 (R4): history, restore, undo ─────────────────────────────────

/** The columns a history read returns. */
const EVENT_COLUMNS = {
  id: appRecordEvents.id,
  recordId: appRecordEvents.recordId,
  kind: appRecordEvents.kind,
  op: appRecordEvents.op,
  actor: appRecordEvents.actor,
  author: appRecordEvents.author,
  via: appRecordEvents.via,
  patch: appRecordEvents.patch,
  before: appRecordEvents.before,
  createdAt: appRecordEvents.createdAt,
}
/** Newest first; a transaction's writes share created_at, so then the write order. */
const eventsNewestFirst = [desc(appRecordEvents.createdAt), desc(appRecordEvents.seq)] as const

/** A record's events, newest first (`history.historyLimit`). Needs the
 *  record's read policy (else NOT_FOUND); a deleted record keeps its history. */
export async function listRecordEvents(userId: string, ref: AppRef, recordId: string, executor: Executor = defaultDb) {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  return inRecordsScope(
    { teamId: app.teamId, appIds: [app.id] },
    async (tx) => {
      const [record] = await tx
        .select({ id: appRecords.id, kind: appRecords.kind, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
        .from(appRecords)
        .where(and(eq(appRecords.id, recordId), eq(appRecords.appId, app.id)))
        .limit(1)
      if (!record) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
      assertReadable(app, viewer, record)
      const events = await tx
        .select(EVENT_COLUMNS)
        .from(appRecordEvents)
        .where(and(eq(appRecordEvents.recordId, record.id), eq(appRecordEvents.appId, app.id)))
        .orderBy(...eventsNewestFirst)
        .limit(APP_RECORD_HISTORY.historyLimit)
      return { events }
    },
    executor
  )
}

/** The state to write back: data (a restore, live again) or null (a delete). */
type Target = { recordId: string; state: Record<string, unknown> | null }

/** VAPP-215: write each target as a NEW write (op restore, or delete), in
 *  order, in ONE transaction; each needs the record's update policy (a
 *  delete: delete). A delete of an already deleted record is a no-op. */
async function writeTargets(
  userId: string,
  app: App,
  viewer: RecordViewer & { id: string; role: string },
  via: RecordWriteVia,
  resolve: (tx: Tx) => Promise<Target[]>,
  executor: Executor
): Promise<{ txid: number; eventIds: string[] }> {
  assertCanWrite(viewer)
  const scope = { teamId: app.teamId, appIds: [app.id], write: viewer.access === `write` }
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    return asEapps(tx, scope, async (tx) => {
      const events: EventInput[] = []
      for (const target of await resolve(tx)) {
        const [existing] = await tx
          .select({ id: appRecords.id, kind: appRecords.kind, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy, deletedAt: appRecords.deletedAt })
          .from(appRecords)
          .where(and(eq(appRecords.id, target.recordId), eq(appRecords.appId, app.id)))
          .limit(1)
          .for(`update`)
        if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
        assertReadable(app, viewer, existing)
        const live = existing.deletedAt === null
        if (target.state === null) {
          if (!live) continue
          assertRecordOp(app, existing.kind, `delete`, viewer, existing)
          await tx.update(appRecords).set({ deletedAt: sql`now()`, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
          events.push({ recordId: existing.id, kind: existing.kind, op: `delete`, author: recordAuthor(existing), patch: null, before: existing.data })
          continue
        }
        assertRecordOp(app, existing.kind, `update`, viewer, existing)
        const schemas = kindSchemas(app, existing.kind)
        if (schemas.length === 0) throw new TRPCError({ code: `BAD_REQUEST`, message: `The app declares no record kind "${existing.kind}" any more` })
        const data = checkedAgainst(schemas, target.state)
        await tx.update(appRecords).set({ data, deletedAt: null, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
        events.push({ recordId: existing.id, kind: existing.kind, op: `restore`, author: recordAuthor(existing), patch: data, before: live ? existing.data : null })
      }
      return { txid: txId, eventIds: await logEvents(tx, app, userId, via, events) }
    })
  })
}

/** History's Restore: the record as `eventId` left it, as a new write. */
export async function restoreRecord(
  userId: string,
  ref: AppRef,
  input: { recordId: string; eventId: string; via?: RecordWriteVia },
  executor: Executor = defaultDb
): Promise<{ txid: number; eventIds: string[] }> {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  return writeTargets(
    userId,
    app,
    viewer,
    input.via ?? `app`,
    async (tx) => {
      const [event] = await tx
        .select(EVENT_COLUMNS)
        .from(appRecordEvents)
        .where(and(eq(appRecordEvents.id, input.eventId), eq(appRecordEvents.recordId, input.recordId), eq(appRecordEvents.appId, app.id)))
        .limit(1)
      if (!event) throw new TRPCError({ code: `NOT_FOUND`, message: `Event not found` })
      return [{ recordId: event.recordId, state: eventState(event) }]
    },
    executor
  )
}

/** The toast's Undo: each event's `before` (`inverseWrite`), newest first,
 *  all in one transaction. */
export async function undoEvents(
  userId: string,
  ref: AppRef,
  input: { eventIds: readonly string[]; via?: RecordWriteVia },
  executor: Executor = defaultDb
): Promise<{ txid: number; eventIds: string[] }> {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  return writeTargets(
    userId,
    app,
    viewer,
    input.via ?? `app`,
    async (tx) => {
      const events = await tx
        .select({ ...EVENT_COLUMNS, seq: appRecordEvents.seq })
        .from(appRecordEvents)
        .where(and(inArray(appRecordEvents.id, [...input.eventIds]), eq(appRecordEvents.appId, app.id)))
        .orderBy(...eventsNewestFirst)
      if (events.length !== new Set(input.eventIds).size) throw new TRPCError({ code: `NOT_FOUND`, message: `Event not found` })
      return events.map((event) => {
        const inverse = inverseWrite(event)
        return { recordId: event.recordId, state: inverse.op === `delete` ? null : inverse.data }
      })
    },
    executor
  )
}

// ── the apps broker (`/api/mcp/apps`): reads of one app's own kind ───────
// The broker resolved the grant, membership and level already; writes go
// through `upsertRecord` like every other writer.

/** One live record of the app's kind the member may read, or null. */
export async function getAppRecord(app: { id: string; teamId: string }, kind: string, id: string, userId: string) {
  const full = await loadApp(defaultDb, { id: app.id })
  const viewer = await viewerOf(defaultDb, full, userId)
  const [row] = await inRecordsScope({ teamId: app.teamId, appIds: [app.id] }, (tx) =>
    tx
      .select(BROKER_COLUMNS)
      .from(appRecords)
      .where(and(eq(appRecords.appId, app.id), eq(appRecords.kind, kind), isNull(appRecords.deletedAt), eq(appRecords.id, id), readPolicyWhere(full, viewer)))
      .limit(1)
  )
  return row ? withComputedFields(full, [row], kind)[0]! : null
}

// ── #VAPP-216 (Q4): the full structured query on the server ─────────────
// Tools (the broker's `run.records` list, MCP `apps records_list` with
// `query`) get the SAME query as the UI: the rows the caller may read (read
// policy + RLS first, an optional jsonb containment prefilter) are handed to
// the eUI evaluator (`@exponential_app/eui/host/query`) the web host runs.

/** The server row read under one query: the Team plan's record quota (every
 *  row a capped team can hold), newest first beyond it. */
export const RECORDS_SERVER_READ_MAX = PLAN_LIMITS.team.appRecords

/** The keys of a JSON query a tool may not set: the kind is the tool's, and
 *  a tool reads only its own app. */
const SERVER_QUERY_FIXED = [`from`, `app`]

/** The app's live rows of `kind` the member may read, as the evaluator takes
 *  them, capped at `RECORDS_SERVER_READ_MAX` (`truncated` = the cap cut). */
export async function readAppRecordRows(
  app: { id: string; teamId: string },
  kind: string,
  contains: Record<string, unknown>,
  userId: string,
  executor: Executor = defaultDb
): Promise<{ rows: RecordRow[]; truncated: boolean }> {
  const full = await loadApp(executor, { id: app.id })
  const viewer = await viewerOf(executor, full, userId)
  const read = await inRecordsScope(
    { teamId: app.teamId, appIds: [app.id] },
    (tx) =>
      tx
        .select(RECORD_COLUMNS)
        .from(appRecords)
        .where(
          and(
            eq(appRecords.appId, app.id),
            eq(appRecords.kind, kind),
            isNull(appRecords.deletedAt),
            Object.keys(contains).length > 0 ? sql`${appRecords.data} @> ${JSON.stringify(contains)}::jsonb` : undefined,
            readPolicyWhere(full, viewer)
          )
        )
        .orderBy(...newestFirst)
        .limit(RECORDS_SERVER_READ_MAX + 1),
    executor
  )
  const rows = read.slice(0, RECORDS_SERVER_READ_MAX).map((r) => ({
    id: r.id,
    kind: r.kind,
    data: (r.data ?? {}) as Record<string, unknown>,
    createdBy: r.createdBy,
    authorId: r.authorId,
    createdAt: iso(r.createdAt),
    updatedAt: iso(r.updatedAt),
  }))
  // VAPP-214: the kind's computed fields ride in each row's data (the query filters, sorts and groups on them).
  return { rows: [...withComputed(rows, kind, kindComputed(effectiveRecords(full), kind), new Date().toISOString())], truncated: read.length > RECORDS_SERVER_READ_MAX }
}

const iso = (v: unknown): string => (v instanceof Date ? v.toISOString() : String(v ?? ``))

/** The team's members by id → display name (the value's `author`). */
export async function teamMemberNames(teamId: string, executor: Pick<Tx, `select`> = defaultDb): Promise<Record<string, string>> {
  const rows = await executor
    .select({ id: users.id, name: users.name })
    .from(teamMembers)
    .innerJoin(users, eq(users.id, teamMembers.userId))
    .where(eq(teamMembers.teamId, teamId))
  return Object.fromEntries(rows.map((r) => [r.id, r.name ?? ``]))
}

/** A JSON query over one kind's rows → the records value, or every problem
 *  with the query (a tool refuses rather than run half a query). Pure: the
 *  same `recordsValue` as the UI, the kind fixed, no other app. */
export function serverRecordsValue(
  rows: readonly RecordRow[],
  kind: string,
  json: Record<string, unknown>,
  ctx: { me: string; names: Record<string, string>; now?: string }
): { value: RecordsValue; issues: [] } | { value: null; issues: RecordsQueryIssue[] } {
  const fixed = SERVER_QUERY_FIXED.filter((key) => json[key] !== undefined && json[key] !== null && !(key === `from` && json[key] === kind))
  const { query, issues } = parseRecordsJson(json, kind)
  const all = [...fixed.map((key) => ({ path: `/${key}`, message: key === `app` ? `a tool reads only its own app's records` : `the kind is the tool's (${kind})` })), ...issues]
  if (all.length > 0) return { value: null, issues: all }
  return { value: recordsValue(rows, query, { me: ctx.me, names: ctx.names, now: ctx.now })!, issues: [] }
}

/** The issues as one line (`/where/0: …; /limit: …`). */
export const queryIssuesText = (issues: readonly RecordsQueryIssue[]) =>
  issues
    .slice(0, 10)
    .map((i) => `${i.path || `/`}: ${i.message}`)
    .join(`; `)

/** tRPC `apps.records.query` (MCP `apps records_list` with `query`): the
 *  full query over the app's rows of `kind` the member may read. */
export async function queryRecords(userId: string, ref: AppRef, input: { kind: string; query: Record<string, unknown> }, executor: Executor = defaultDb) {
  const app = await loadApp(executor, ref)
  const { rows, truncated } = await readAppRecordRows(app, input.kind, {}, userId, executor)
  const out = serverRecordsValue(rows, input.kind, input.query, { me: userId, names: await teamMemberNames(app.teamId, executor) })
  if (out.value === null) throw new TRPCError({ code: `BAD_REQUEST`, message: `query is invalid: ${queryIssuesText(out.issues)}` })
  return truncated ? { ...out.value, truncated } : out.value
}

// ── `.app.json` import ───────────────────────────────────────────────────

/** VAPP-121: an import's records, inside the import's transaction after the
 *  app and its version are written (the quota first, then the role: nothing
 *  after this in `tx` may touch another table). */
export async function importRecords(
  tx: Tx,
  app: Pick<App, `id` | `teamId` | `package`>,
  userId: string,
  role: string,
  records: readonly { kind: string; data: Record<string, unknown>; author?: string }[]
): Promise<void> {
  if (records.length === 0) return
  // eApps R3: the importer creates every record (a new app: no access row yet).
  for (const kind of new Set(records.map((r) => r.kind))) assertRecordOp(app, kind, `create`, { id: userId, role })
  await lockAndCheckQuota(tx, app.teamId, records.length)
  await asEapps(tx, { teamId: app.teamId, appIds: [app.id], write: true }, async (tx) => {
    const rows = await tx
      .insert(appRecords)
      .values(records.map((r) => ({ appId: app.id, teamId: app.teamId, kind: r.kind, data: r.data, createdBy: userId, authorId: r.author ?? userId, updatedBy: userId })))
      .returning({ id: appRecords.id, kind: appRecords.kind, data: appRecords.data, authorId: appRecords.authorId })
    await logEvents(
      tx,
      app,
      userId,
      `import`,
      rows.map((r) => ({ recordId: r.id, kind: r.kind, op: `create` as const, author: r.authorId, patch: r.data, before: null }))
    )
  })
}
