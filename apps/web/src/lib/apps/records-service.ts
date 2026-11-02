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
import { and, asc, desc, eq, isNull, sql } from "drizzle-orm"
import { validateRecord, type RecordSchema } from "@exponential_app/eui"
import { RECORDS_QUERY } from "@exponential_app/eui/host/query"
import { APP_RECORD_MAX_BYTES } from "@exp/domain-contract/apps"
import { APP_RECORDS_ON_BEHALF, APP_RECORDS_OWNERSHIP, canChangeRecord, recordAuthor, writeAsVerdict } from "@exp/domain-contract/app-records"
import { db as defaultDb } from "@/db/connection"
import { appRecords, apps, teamMembers, type App } from "@/db/schema"
import { assertTeamMember } from "@/lib/auth/membership"
import { assertCanCreateAppRecord } from "@/lib/billing"
import { packageBytes, recordKindSchema } from "@/lib/apps/package"
import { generateTxId } from "@/lib/trpc"
import type { Db, Tx } from "@/lib/sessions/tx"

/** The role a records statement runs as (custom SQL: `eapps` + its policy). */
export const RECORDS_ROLE = `eapps`
/** A records statement's ceiling under the role. */
export const RECORDS_STATEMENT_TIMEOUT = `2s`

/** What a records transaction may touch: one team, these apps. */
export interface RecordsScope {
  teamId: string
  appIds: readonly string[]
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
  await tx.execute(sql`SELECT set_config('eapps.team', ${scope.teamId}, true), set_config('eapps.apps', ${scope.appIds.join(`,`)}, true), set_config('statement_timeout', ${RECORDS_STATEMENT_TIMEOUT}, true)`)
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

/** VAPP-187: a record's author or a team owner may patch/delete it
 *  (app-records.json `ownership`); anyone else = FORBIDDEN. */
function assertCanChangeRecord(row: { authorId: string | null; createdBy: string | null }, userId: string, role: string, op: `update` | `delete`) {
  if (!canChangeRecord(row, userId, role, op)) throw new TRPCError({ code: `FORBIDDEN`, message: APP_RECORDS_OWNERSHIP.refusal })
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
  await assertTeamMember(userId, app.teamId)
  const rows = await inRecordsScope(
    { teamId: app.teamId, appIds: [app.id] },
    (tx) =>
      tx
        .select(RECORD_COLUMNS)
        .from(appRecords)
        .where(and(eq(appRecords.appId, app.id), isNull(appRecords.deletedAt), query.kind ? eq(appRecords.kind, query.kind) : undefined))
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
  const member = await assertTeamMember(userId, app.teamId)
  const schema = recordKindSchema(app.package, input.kind)
  if (!schema) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `The app declares no record kind "${input.kind}": add it to the package's records first` })
  }
  const checked = (data: Record<string, unknown>) => checkedRecordData(schema, data)
  // VAPP-188: an on-behalf author must be a member (team_members: read now).
  const onBehalf = input.author !== undefined && input.author !== userId && member.role === `owner`
  const members = onBehalf ? await teamMemberIds(executor, app.teamId) : []
  const scope = { teamId: app.teamId, appIds: [app.id] }
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    if (input.recordId) {
      const recordId = input.recordId
      return asEapps(tx, scope, async (tx) => {
        // Release train 2026-10-25: the patch merges onto the row as locked
        // in THIS transaction, so two concurrent patches queue here and
        // neither loses the other's fields.
        const [existing] = await tx
          .select({ id: appRecords.id, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
          .from(appRecords)
          .where(and(eq(appRecords.id, recordId), eq(appRecords.appId, app.id), eq(appRecords.kind, input.kind), isNull(appRecords.deletedAt)))
          .limit(1)
          .for(`update`)
        if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
        assertCanChangeRecord(existing, userId, member.role, `update`)
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
  const member = await assertTeamMember(userId, app.teamId)
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    return asEapps(tx, { teamId: app.teamId, appIds: [app.id] }, async (tx) => {
      const [existing] = await tx
        .select({ id: appRecords.id, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
        .from(appRecords)
        .where(and(eq(appRecords.id, recordId), eq(appRecords.appId, app.id), isNull(appRecords.deletedAt)))
        .limit(1)
        .for(`update`)
      if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
      assertCanChangeRecord(existing, userId, member.role, `delete`)
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
  const member = await assertTeamMember(userId, app.teamId)
  const at = (i: number, e: unknown): never => {
    if (e instanceof TRPCError) throw new TRPCError({ code: e.code, message: `write ${i}: ${e.message}` })
    throw e
  }
  const schemas = writes.map((w, i) => {
    if (w.op !== `upsert`) return null
    const schema = recordKindSchema(app.package, w.kind)
    if (!schema) at(i, new TRPCError({ code: `BAD_REQUEST`, message: `The app declares no record kind "${w.kind}": add it to the package's records first` }))
    return schema
  })
  const creates = writes.filter((w) => w.op === `upsert` && w.id === undefined).length
  const scope = { teamId: app.teamId, appIds: [app.id] }
  return await executor.transaction(async (tx) => {
    const txId = await generateTxId(tx)
    if (creates > 0) await lockAndCheckQuota(tx, app.teamId, creates)
    return asEapps(tx, scope, async (tx) => {
      const ids: string[] = []
      for (const [i, w] of writes.entries()) {
        try {
          if (w.op === `delete`) {
            const [existing] = await tx
              .select({ id: appRecords.id, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
              .from(appRecords)
              .where(and(eq(appRecords.id, w.id), eq(appRecords.appId, app.id), isNull(appRecords.deletedAt)))
              .limit(1)
              .for(`update`)
            if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
            assertCanChangeRecord(existing, userId, member.role, `delete`)
            await tx.update(appRecords).set({ deletedAt: sql`now()`, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
            ids.push(existing.id)
            continue
          }
          const schema = schemas[i]!
          if (w.id !== undefined) {
            const [existing] = await tx
              .select({ id: appRecords.id, data: appRecords.data, authorId: appRecords.authorId, createdBy: appRecords.createdBy })
              .from(appRecords)
              .where(and(eq(appRecords.id, w.id), eq(appRecords.appId, app.id), eq(appRecords.kind, w.kind), isNull(appRecords.deletedAt)))
              .limit(1)
              .for(`update`)
            if (!existing) throw new TRPCError({ code: `NOT_FOUND`, message: `Record not found` })
            assertCanChangeRecord(existing, userId, member.role, `update`)
            const data = checkedRecordData(schema, { ...existing.data, ...w.data })
            await tx.update(appRecords).set({ data, updatedBy: userId, updatedAt: new Date() }).where(eq(appRecords.id, existing.id))
            ids.push(existing.id)
            continue
          }
          const data = checkedRecordData(schema, w.data)
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

/** One live record of the app's kind, or null. */
export async function getAppRecord(app: { id: string; teamId: string }, kind: string, id: string) {
  const [row] = await inRecordsScope({ teamId: app.teamId, appIds: [app.id] }, (tx) =>
    tx
      .select(BROKER_COLUMNS)
      .from(appRecords)
      .where(and(eq(appRecords.appId, app.id), eq(appRecords.kind, kind), isNull(appRecords.deletedAt), eq(appRecords.id, id)))
      .limit(1)
  )
  return row ?? null
}

/** The app's live records of a kind whose data CONTAINS `where` (jsonb `@>`,
 *  one bound parameter), newest first, `limit` clamped to the query caps. */
export async function queryAppRecords(app: { id: string; teamId: string }, kind: string, where: Record<string, unknown>, limit: number | undefined) {
  const asked = typeof limit === `number` && Number.isFinite(limit) ? Math.floor(limit) : RECORDS_QUERY.limitDefault
  const capped = Math.min(Math.max(asked, 1), RECORDS_QUERY.limitMax)
  return inRecordsScope({ teamId: app.teamId, appIds: [app.id] }, (tx) =>
    tx
      .select(BROKER_COLUMNS)
      .from(appRecords)
      .where(
        and(
          eq(appRecords.appId, app.id),
          eq(appRecords.kind, kind),
          isNull(appRecords.deletedAt),
          Object.keys(where).length > 0 ? sql`${appRecords.data} @> ${JSON.stringify(where)}::jsonb` : undefined
        )
      )
      .orderBy(...newestFirst)
      .limit(capped)
  )
}

// ── `.app.json` import ───────────────────────────────────────────────────

/** VAPP-121: an import's records, inside the import's transaction after the
 *  app and its version are written (the quota first, then the role: nothing
 *  after this in `tx` may touch another table). */
export async function importRecords(
  tx: Tx,
  app: { id: string; teamId: string },
  userId: string,
  records: readonly { kind: string; data: Record<string, unknown>; author?: string }[]
): Promise<void> {
  if (records.length === 0) return
  await lockAndCheckQuota(tx, app.teamId, records.length)
  await asEapps(tx, { teamId: app.teamId, appIds: [app.id] }, async (tx) => {
    await tx
      .insert(appRecords)
      .values(records.map((r) => ({ appId: app.id, teamId: app.teamId, kind: r.kind, data: r.data, createdBy: userId, authorId: r.author ?? userId, updatedBy: userId })))
  })
}
