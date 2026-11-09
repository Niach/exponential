// VAPP-289 (round 9 contract §14.1): the one-shot stored-data migration of the
// A2UI 1.0 flag day. DORMANT until the flag-day PR runs it: `--dry-run` from
// `scripts/a2ui10-migrate.ts`, then this module at boot.
//
// What moves: `apps.package`, `apps.draft_package`, `app_versions.package` →
// format 6; `apps.granted_tools` → flat names; every clear card surface in
// `messages.card` → `"version": "1.0"`. A sealed card stays (the server cannot
// read it): clients show it through `upgradeCard09`.
//
// Safe beside a live server and after a crash. Rows are read outside any
// transaction and each write compares the fingerprint of the columns it read,
// so a row changed since is never clobbered: it is read again, and one that
// keeps changing is reported. A row that fails stays as it is, every column of
// it. 1.0 data is skipped, so a second run rewrites nothing.
//
// Electric: `exponential.preserve_timestamps` keeps `updated_at`, so a rewritten
// row's delta is the rewritten column alone. The report's `electric` block sizes
// it. `app_versions.package` sits behind its shape's allowlist and costs nothing;
// `conversations.digest` never names a surface, so it does not move.

import { Buffer } from "node:buffer"
import { sql, type SQL } from "drizzle-orm"
import { flatAction } from "@exp/domain-contract/app-actions"
import { A2UI_VERSION, PACKAGE_FORMAT_VERSION, PACKAGE_FORMAT_VERSION_09, cardV10, signPackageFile, upgradeCard09, upgradePackage09, upgradePackageBody09, verifyPackageFile } from "@exponential_app/eui"
import type { db as appDb } from "@/db/connection"
import { migratedPackageIssues } from "@/lib/apps/package"
import { validateCardSurface, type CardSurfaceInput } from "@/lib/sessions/plan"
import { isSealedCard } from "@/lib/sessions/sealed"

/** The connection or transaction a run goes through. */
export type MigrationDb = Pick<typeof appDb, `execute` | `transaction`>

/** What one stored value needs. `empty` = nothing of ours in it (not counted). */
export type Verdict = { kind: `empty` } | { kind: `current` } | { kind: `sealed` } | { kind: `rewrite`; value: unknown } | { kind: `failed`; reason: string }

export interface ColumnReport {
  /** Rows holding a value of ours (a NULL package is none; for
   *  `messages.card`, the cards with a surface or sealed content). */
  total: number
  alreadyV10: number
  /** Wanted a rewrite; `rewritten` of them were written (0 on a dry run). */
  toRewrite: number
  rewritten: number
  sealedSkipped: number
  /** Upgradable, but another column of the row failed: the row stays whole. */
  heldBack: number
  failed: { id: string; reason: string }[]
  /** Changed under every read of this run; a later run takes them. */
  conflicted: string[]
  /** The JSON bytes of `toRewrite`. */
  bytes: number
  /** Does a client shape carry this column? */
  synced: boolean
}

export interface ElectricTally {
  rows: number
  bytes: number
}

export interface MigrationReport {
  mode: `dry-run` | `apply`
  batchSize: number
  startedAt: string
  finishedAt: string
  /** No failed and no conflicted row. */
  ok: boolean
  columns: Record<string, ColumnReport>
  /** What Electric re-sends (a dry run: would): one change per synced row
   *  rewritten, these bytes of JSON; deltas, so `updated_at` is not among them. */
  electric: ElectricTally
}

export interface MigrationOptions {
  apply: boolean
  batchSize?: number
  /** Test seam: runs between a batch's read and its write. */
  afterRead?: (table: string, ids: string[]) => Promise<void>
  /** Test seam: walks one team's rows only (every table here carries
   *  `team_id`), so a real-DB suite never reads or row-locks the rows the
   *  suites beside it commit. The script and boot walk everything. */
  teamId?: string
}

const isObj = (v: unknown): v is Record<string, unknown> => typeof v === `object` && v !== null && !Array.isArray(v)
const messageOf = (e: unknown) => (e instanceof Error ? e.message : String(e))
const MAX_CONFLICT_ROUNDS = 3
const DEFAULT_BATCH = 200
const NIL_UUID = `00000000-0000-0000-0000-000000000000`

// ---------------------------------------------------------------------------
// One value

/** A stored package → format 6: the SDK's validator (no 0.9 bridge) and the
 *  caps must pass on the result. A stored package carries no file signature
 *  (§18.1); a stray one must verify, like any file's. */
export async function upgradeStoredPackage(value: unknown): Promise<Verdict> {
  if (value === null || value === undefined) return { kind: `empty` }
  if (!isObj(value)) return { kind: `failed`, reason: `a package is an object` }
  if (value.formatVersion === PACKAGE_FORMAT_VERSION) return { kind: `current` }
  if (value.formatVersion !== PACKAGE_FORMAT_VERSION_09) return { kind: `failed`, reason: `format version ${JSON.stringify(value.formatVersion)} has no upgrade` }
  const upgraded = await upgradePackage09(value)
  if (upgraded.package === null) return { kind: `failed`, reason: `its signature does not verify` }
  const issues = migratedPackageIssues(upgraded.package)
  if (issues.length > 0) {
    const listed = issues.slice(0, 3).map((issue) => `${issue.path}: ${issue.message}`)
    return { kind: `failed`, reason: listed.join(`; `) + (issues.length > 3 ? `; and ${issues.length - 3} more` : ``) }
  }
  return { kind: `rewrite`, value: upgraded.package }
}

/** A `messages.card`: its surface, if clear and 0.9, → 1.0. The upgraded
 *  surface passes what a new card passes (`validateCardSurface`: the caps, the
 *  entry check, the reducer, Motion) and the entry with the bridge off. */
export function upgradeStoredCard(card: unknown): Verdict {
  if (!isObj(card)) return { kind: `empty` }
  // EXP-1458: `{kind, sealed, ...clear}`; an `ask` mirror holds it in `sealedCard`.
  if (isSealedCard(card) || `sealedCard` in card) return { kind: `sealed` }
  const surface = card.surface
  if (!isObj(surface)) return { kind: `empty` }
  if (surface.version === A2UI_VERSION) return { kind: `current` }
  if (surface.version !== undefined) return { kind: `failed`, reason: `unsupported surface version ${JSON.stringify(surface.version)}` }
  const upgraded = upgradeCard09(surface)
  const entry = cardV10(upgraded, false)
  if (`error` in entry) return { kind: `failed`, reason: entry.error }
  try {
    validateCardSurface(upgraded as CardSurfaceInput)
  } catch (e) {
    return { kind: `failed`, reason: messageOf(e) }
  }
  return { kind: `rewrite`, value: { ...card, surface: upgraded } }
}

/** `apps.granted_tools` → flat names, stored order kept. */
export function upgradeGrants(value: unknown): Verdict {
  if (value === null || value === undefined) return { kind: `empty` }
  if (!Array.isArray(value) || value.some((name) => typeof name !== `string`)) return { kind: `failed`, reason: `granted_tools is a list of names` }
  const flat = [...new Set((value as string[]).map(flatAction))]
  return flat.length === value.length && flat.every((name, i) => name === value[i]) ? { kind: `current` } : { kind: `rewrite`, value: flat }
}

// ---------------------------------------------------------------------------
// One table

interface ColumnSpec {
  column: string
  plan: (value: unknown) => Verdict | Promise<Verdict>
  synced: boolean
}

interface TableSpec {
  table: string
  /** Narrows the walk to the rows that can matter. */
  where?: string
  columns: ColumnSpec[]
}

/** §14.1's list. Identifiers only: every `sql.raw` below reads these. */
const TABLES: readonly TableSpec[] = [
  {
    table: `apps`,
    columns: [
      { column: `package`, plan: upgradeStoredPackage, synced: true },
      { column: `draft_package`, plan: upgradeStoredPackage, synced: true },
      { column: `granted_tools`, plan: upgradeGrants, synced: true },
    ],
  },
  { table: `app_versions`, columns: [{ column: `package`, plan: upgradeStoredPackage, synced: false }] },
  {
    table: `messages`,
    where: `jsonb_typeof(card) = 'object' AND (jsonb_typeof(card->'surface') = 'object' OR card ? 'sealed' OR card ? 'sealedCard')`,
    columns: [{ column: `card`, plan: upgradeStoredCard, synced: true }],
  },
]

type Row = { id: string; fingerprint: string } & Record<string, unknown>

/** One row's rewrites. */
interface RowPlan {
  id: string
  fingerprint: string
  writes: { spec: ColumnSpec; value: unknown; bytes: number }[]
}

const rowsOf = <T>(result: unknown): T[] => (result as { rows: T[] }).rows
const keyOf = (spec: TableSpec, column: ColumnSpec) => `${spec.table}.${column.column}`

const emptyColumn = (synced: boolean): ColumnReport => ({ total: 0, alreadyV10: 0, toRewrite: 0, rewritten: 0, sealedSkipped: 0, heldBack: 0, failed: [], conflicted: [], bytes: 0, synced })

function selectRows(spec: TableSpec): SQL {
  const columns = spec.columns.map((c) => c.column)
  return sql.raw(`SELECT id::text AS id, ${columns.join(`, `)}, ${fingerprintOf(spec)} AS fingerprint FROM ${spec.table}`)
}

/** Over the jsonb's text: a JS number round trip would never compare equal
 *  for a value finer than a double. */
function fingerprintOf(spec: TableSpec): string {
  return `md5(jsonb_build_array(${spec.columns.map((c) => c.column).join(`, `)})::text)`
}

async function readPage(client: MigrationDb, spec: TableSpec, after: string, limit: number, teamId: string | undefined): Promise<Row[]> {
  const where = spec.where ? sql` AND ${sql.raw(spec.where)}` : sql``
  const team = teamId ? sql` AND team_id = ${teamId}::uuid` : sql``
  return rowsOf<Row>(await client.execute(sql`${selectRows(spec)} WHERE id > ${after}::uuid${where}${team} ORDER BY id LIMIT ${limit}`))
}

async function readIds(client: MigrationDb, spec: TableSpec, ids: string[]): Promise<Row[]> {
  return rowsOf<Row>(await client.execute(sql`${selectRows(spec)} WHERE id = ANY(${sql.param(ids)}::uuid[]) ORDER BY id`))
}

/** The row's verdicts into `columns`, and what to write. `recount` false = a
 *  conflict's second read: the row is in the totals already. */
async function planRow(spec: TableSpec, row: Row, columns: Record<string, ColumnReport>, recount: boolean): Promise<RowPlan | null> {
  const verdicts: { column: ColumnSpec; report: ColumnReport; verdict: Verdict }[] = []
  for (const column of spec.columns) {
    let verdict: Verdict
    try {
      verdict = await column.plan(row[column.column])
    } catch (e) {
      verdict = { kind: `failed`, reason: messageOf(e) }
    }
    verdicts.push({ column, report: columns[keyOf(spec, column)]!, verdict })
  }
  const failed = verdicts.some((v) => v.verdict.kind === `failed`)
  const writes: RowPlan[`writes`] = []
  for (const { column, report, verdict } of verdicts) {
    if (verdict.kind === `empty`) continue
    if (recount) report.total += 1
    if (verdict.kind === `failed`) report.failed.push({ id: row.id, reason: verdict.reason })
    else if (verdict.kind === `current` && recount) report.alreadyV10 += 1
    else if (verdict.kind === `sealed` && recount) report.sealedSkipped += 1
    else if (verdict.kind === `rewrite`) {
      const bytes = Buffer.byteLength(JSON.stringify(verdict.value))
      if (recount) {
        report.toRewrite += 1
        report.bytes += bytes
      }
      if (failed) report.heldBack += 1
      else writes.push({ spec: column, value: verdict.value, bytes })
    }
  }
  return writes.length > 0 ? { id: row.id, fingerprint: row.fingerprint, writes } : null
}

/** The compare-and-swap of one row: false = it changed since it was read. */
async function writeRow(tx: Pick<MigrationDb, `execute`>, spec: TableSpec, plan: RowPlan): Promise<boolean> {
  const sets = sql.join(
    plan.writes.map((w) => sql`${sql.raw(w.spec.column)} = ${JSON.stringify(w.value)}::jsonb`),
    sql`, `
  )
  const result = await tx.execute(sql`UPDATE ${sql.raw(spec.table)} SET ${sets} WHERE id = ${plan.id}::uuid AND ${sql.raw(fingerprintOf(spec))} = ${plan.fingerprint} RETURNING id`)
  return rowsOf(result).length > 0
}

/** What a run tallies as it goes. */
interface Tally {
  columns: Record<string, ColumnReport>
  electric: ElectricTally
}

function tallyElectric(electric: ElectricTally, plan: RowPlan) {
  const synced = plan.writes.filter((write) => write.spec.synced)
  if (synced.length === 0) return
  electric.rows += 1
  electric.bytes += synced.reduce((n, write) => n + write.bytes, 0)
}

/** Writes `plans` in one transaction; the ones it found changed come back. */
async function writeBatch(client: MigrationDb, spec: TableSpec, plans: RowPlan[]): Promise<{ written: RowPlan[]; changed: RowPlan[] }> {
  return client.transaction(async (tx) => {
    // The importer's GUC (0001_triggers.sql §1): no `updated_at` bump.
    await tx.execute(sql`SELECT set_config('exponential.preserve_timestamps', 'on', true)`)
    const written: RowPlan[] = []
    const changed: RowPlan[] = []
    for (const plan of plans) ((await writeRow(tx, spec, plan)) ? written : changed).push(plan)
    return { written, changed }
  })
}

/** The batch, or, when Postgres refuses it, each row on its own: the one row
 *  it refuses is reported and the others land. */
async function writePlans(client: MigrationDb, spec: TableSpec, plans: RowPlan[], tally: Tally): Promise<string[]> {
  if (plans.length === 0) return []
  const { columns } = tally
  const credit = (done: RowPlan[]) => {
    for (const plan of done) {
      for (const write of plan.writes) columns[keyOf(spec, write.spec)]!.rewritten += 1
      tallyElectric(tally.electric, plan)
    }
  }
  try {
    const { written, changed } = await writeBatch(client, spec, plans)
    credit(written)
    return changed.map((plan) => plan.id)
  } catch {
    const changedIds: string[] = []
    for (const plan of plans) {
      try {
        const { written, changed } = await writeBatch(client, spec, [plan])
        credit(written)
        changedIds.push(...changed.map((p) => p.id))
      } catch (e) {
        for (const write of plan.writes) columns[keyOf(spec, write.spec)]!.failed.push({ id: plan.id, reason: messageOf(e) })
      }
    }
    return changedIds
  }
}

async function migrateTable(client: MigrationDb, spec: TableSpec, options: MigrationOptions & { batchSize: number }, tally: Tally): Promise<void> {
  const { columns } = tally
  let after = NIL_UUID
  let conflicted: string[] = []
  for (;;) {
    const rows = await readPage(client, spec, after, options.batchSize, options.teamId)
    if (rows.length === 0) break
    after = rows[rows.length - 1]!.id
    const plans: RowPlan[] = []
    for (const row of rows) {
      const plan = await planRow(spec, row, columns, true)
      if (plan) plans.push(plan)
    }
    if (options.apply) {
      await options.afterRead?.(spec.table, rows.map((row) => row.id))
      conflicted.push(...(await writePlans(client, spec, plans, tally)))
    } else for (const plan of plans) tallyElectric(tally.electric, plan)
  }
  // A row a live server changed under us: read it again, a few times.
  for (let round = 0; round < MAX_CONFLICT_ROUNDS && conflicted.length > 0; round += 1) {
    const rows = await readIds(client, spec, conflicted)
    const plans: RowPlan[] = []
    for (const row of rows) {
      const plan = await planRow(spec, row, columns, false)
      if (plan) plans.push(plan)
    }
    await options.afterRead?.(spec.table, rows.map((row) => row.id))
    conflicted = await writePlans(client, spec, plans, tally)
  }
  // A surviving conflict is the table's, whichever column it would have moved.
  for (const id of conflicted) columns[keyOf(spec, spec.columns[0]!)]!.conflicted.push(id)
}

/** THE entry: the script's and, from the flag-day PR on, boot's. Dry run
 *  unless `apply`. Throws only when the database does. */
export async function runA2ui10Migration(client: MigrationDb, options: MigrationOptions): Promise<MigrationReport> {
  const startedAt = new Date().toISOString()
  const batchSize = options.batchSize ?? DEFAULT_BATCH
  const columns: Record<string, ColumnReport> = {}
  for (const spec of TABLES) for (const column of spec.columns) columns[keyOf(spec, column)] = emptyColumn(column.synced)
  const electric: ElectricTally = { rows: 0, bytes: 0 }
  for (const spec of TABLES) await migrateTable(client, spec, { ...options, batchSize }, { columns, electric })
  const reports = Object.values(columns)
  return {
    mode: options.apply ? `apply` : `dry-run`,
    batchSize,
    startedAt,
    finishedAt: new Date().toISOString(),
    ok: reports.every((c) => c.failed.length === 0 && c.conflicted.length === 0),
    columns,
    electric,
  }
}

/** The report as the operator reads it. */
export function formatMigrationReport(report: MigrationReport): string {
  const header = [`column`, `total`, `1.0`, `to rewrite`, `rewritten`, `sealed`, `held`, `failed`, `conflicted`]
  const rows = Object.entries(report.columns).map(([name, c]) => [name, c.total, c.alreadyV10, c.toRewrite, c.rewritten, c.sealedSkipped, c.heldBack, c.failed.length, c.conflicted.length].map(String))
  const widths = header.map((h, i) => Math.max(h.length, ...rows.map((r) => r[i]!.length)))
  const line = (cells: string[]) => cells.map((cell, i) => (i === 0 ? cell.padEnd(widths[i]!) : cell.padStart(widths[i]!))).join(`  `)
  const failures = Object.entries(report.columns).flatMap(([name, c]) => [...c.failed.map((f) => `  ${name} ${f.id}: ${f.reason}`), ...c.conflicted.map((id) => `  ${name} ${id}: changed under every read; run it again`)])
  return [
    `a2ui10-migrate (${report.mode}): ${report.ok ? `ok` : `NOT ok`}`,
    line(header),
    ...rows.map(line),
    `Electric re-sends ${report.electric.rows} rows, ${report.electric.bytes} bytes${report.mode === `dry-run` ? ` (on --apply)` : ``}`,
    ...failures,
  ].join(`\n`)
}

// ---------------------------------------------------------------------------
// Package FILES (§18.1): the repo's signed samples, never a table

export interface UpgradedFile {
  /** null = the original's signature does not verify. */
  file: Record<string, unknown> | null
  status: `unsigned` | `valid` | `invalid`
  /** Re-signed with `signer`. A `valid` file that is not has lost its
   *  signature: only its verified status survives (§14.3 Packages). */
  resigned: boolean
}

/** A package file, bare or `{format, app, package, records, author, signature}`,
 *  → format 6. A file signed with the key of `signer` is signed again over its
 *  new content; any other signature cannot be, and its verified status stands. */
export async function upgradePackageFile(file: unknown, signer?: { name: string; seed: Uint8Array }): Promise<UpgradedFile> {
  const verdict = await verifyPackageFile(file)
  if (verdict.status === `invalid` || !isObj(file)) return { file: null, status: `invalid`, resigned: false }
  const { signature: _signature, ...rest } = file
  const next = (isObj(file.package) ? { ...rest, package: upgradePackageBody09(file.package) } : (await upgradePackage09(file)).package) as Record<string, unknown>
  if (verdict.status === `unsigned`) return { file: next, status: `unsigned`, resigned: false }
  if (signer) {
    const signed = await signPackageFile(next, signer)
    if (signed.author.key === verdict.author.key) return { file: signed, status: `valid`, resigned: true }
  }
  return { file: next, status: `valid`, resigned: false }
}
