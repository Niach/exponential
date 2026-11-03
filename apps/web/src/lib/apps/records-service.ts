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
import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, isNull, notInArray, or, sql, type SQL } from "drizzle-orm"
import { validateRecord, type RecordSchema } from "@exponential_app/eui"
import { RECORD_POLICY_COPY, recordPolicy, recordRefusal, type AppAccessLevel, type RecordOp, type RecordViewer } from "@exponential_app/eui/host"
import { parseRecordsJson, recordsValue, type RecordRow, type RecordsQueryIssue, type RecordsValue } from "@exponential_app/eui/host/query"
import { APP_RECORD_MAX_BYTES } from "@exp/domain-contract/apps"
import { APP_RECORDS_ON_BEHALF, recordAuthor, writeAsVerdict } from "@exp/domain-contract/app-records"
import { db as defaultDb } from "@/db/connection"
import { appAccess, appRecords, apps, teamMembers, users, type App } from "@/db/schema"
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
  return { records: rows }
}

/** Create a record, or (with `recordId`) patch that record's data (shallow).
 *  VAPP-188: `author` = an on-behalf write (owners only): the create's
 *  author, or an update's reassignment; created_by/updated_by stay the real
 *  writer. */
export async function upsertRecord(
  userId: string,
  ref: AppRef,
  input: { kind: string; data: Record<string, unknown>; recordId?: string; author?: string },
  executor: Executor = defaultDb
): Promise<{ txid: number; id: string; created: boolean }> {
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
        return { txid: txId, id: existing.id, created: false }
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
      return { txid: txId, id: row!.id, created: true }
    })
  })
}

/** Soft-delete one record (its author or a team owner). */
export async function deleteRecord(userId: string, ref: AppRef, recordId: string, executor: Executor = defaultDb): Promise<{ txid: number }> {
  const app = await loadApp(executor, ref)
  const viewer = await viewerOf(executor, app, userId)
  assertCanWrite(viewer)
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    return asEapps(tx, { teamId: app.teamId, appIds: [app.id], write: viewer.access === `write` }, async (tx) => {
      const [existing] = await tx
        .select({ id: appRecords.id, kind: appRecords.kind, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
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
      return { txid: txId }
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
): Promise<{ txid: number; ids: string[] }> {
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
      for (const [i, w] of writes.entries()) {
        try {
          if (w.op === `delete`) {
            const [existing] = await tx
              .select({ id: appRecords.id, kind: appRecords.kind, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
              .from(appRecords)
              .where(and(eq(appRecords.id, w.id), eq(appRecords.appId, app.id), isNull(appRecords.deletedAt)))
              .limit(1)
              .for(`update`)
            if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
            assertReadable(app, viewer, existing)
            assertRecordOp(app, existing.kind, `delete`, viewer, existing)
            await tx.update(appRecords).set({ deletedAt: sql`now()`, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
            ids.push(existing.id)
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
            continue
          }
          assertRecordOp(app, w.kind, `create`, viewer)
          const data = checkedAgainst(schema, w.data)
          const [row] = await tx
            .insert(appRecords)
            .values({ appId: app.id, teamId: app.teamId, kind: w.kind, data, createdBy: userId, authorId: userId, updatedBy: userId })
            .returning({ id: appRecords.id })
          ids.push(row!.id)
        } catch (e) {
          at(i, e)
        }
      }
      return { txid: txId, ids }
    })
  })
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
  return row ?? null
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
  return { rows, truncated: read.length > RECORDS_SERVER_READ_MAX }
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
    await tx
      .insert(appRecords)
      .values(records.map((r) => ({ appId: app.id, teamId: app.teamId, kind: r.kind, data: r.data, createdBy: userId, authorId: r.author ?? userId, updatedBy: userId })))
  })
}
