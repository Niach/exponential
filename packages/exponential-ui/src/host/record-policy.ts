// eApps R3 (docs/spec/apps.md §3.4, §9): who may read, create, update and
// delete a kind's records. A kind's `policy` names a level per op:
// `member` (any member of the team), `author` (the record's author, or a
// team owner) or `owners` (team owners only); `create` is never `author`
// (a new record has no author yet). Over it sits the member's `app_access`
// level for the app: `none` (no access at all), `read` (reads only) or
// `write` (no row = write). Pure, mirrored line by line in the Rust core
// (`host::record_policy`); fixtures/record-policy.json is the spec. The
// Exponential server enforces it (records service, shape row filter); a
// client host uses it to disable what the server would refuse and for the
// editor's "Run as <member>" preview; package tests (`as`, `role`) run under it.

import type { PackageIssue } from "./package"

export type RecordOp = `read` | `create` | `update` | `delete`
export type RecordLevel = `member` | `author` | `owners`
export type AppAccessLevel = `none` | `read` | `write`
export type RecordPolicy = Record<RecordOp, RecordLevel>

export const RECORD_POLICY_OPS: readonly RecordOp[] = [`read`, `create`, `update`, `delete`]
export const RECORD_POLICY_LEVELS: readonly RecordLevel[] = [`member`, `author`, `owners`]
export const APP_ACCESS_LEVELS: readonly AppAccessLevel[] = [`none`, `read`, `write`]
/** The policy of a kind that names none (or leaves an op out). */
export const DEFAULT_RECORD_POLICY: Readonly<RecordPolicy> = { read: `member`, create: `member`, update: `author`, delete: `author` }

/** The refusals (a toast, a disabled control's reason, the server's FORBIDDEN). */
export const RECORD_POLICY_COPY = {
  none: `You have no access to this app`,
  readOnly: `You have read-only access to this app`,
  author: {
    read: `Only the record's author or a team owner can read it`,
    update: `Only the record's author or a team owner can change it`,
    delete: `Only the record's author or a team owner can change it`,
  },
  owners: {
    read: `Only team owners can read these records`,
    create: `Only team owners can add these records`,
    update: `Only team owners can change these records`,
    delete: `Only team owners can change these records`,
  },
} as const

/** Who is asking: the member id (null = signed out), their team role (null
 *  = not a member of the app's team) and their access level for the app
 *  (absent = write). */
export interface RecordViewer {
  id: string | null
  role: string | null
  access?: AppAccessLevel
}

/** A record's ownership columns: the author is `authorId ?? createdBy`. */
export interface RecordOwnership {
  authorId?: string | null
  createdBy?: string | null
}

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)
const own = (o: Record<string, unknown>, k: string) => Object.prototype.hasOwnProperty.call(o, k)

/** The issues of a kind's `policy` (each under `at`). */
export function validateRecordPolicy(policy: unknown, at: string): PackageIssue[] {
  if (!isObject(policy)) return [{ path: at, message: `policy is {read?, create?, update?, delete?}` }]
  const issues: PackageIssue[] = []
  for (const key of Object.keys(policy).sort()) {
    const value = policy[key]
    if (!(RECORD_POLICY_OPS as readonly string[]).includes(key)) issues.push({ path: `${at}/${key}`, message: `unknown policy op ${key}: read, create, update or delete` })
    else if (key === `create` && value === `author`) issues.push({ path: `${at}/create`, message: `create is member or owners: a new record has no author yet` })
    else if (typeof value !== `string` || !(RECORD_POLICY_LEVELS as readonly string[]).includes(value)) issues.push({ path: `${at}/${key}`, message: `${key} is member, author or owners` })
  }
  return issues
}

/** A kind's effective policy: its `policy` over the defaults (an invalid
 *  entry keeps the default; validation reports it). */
export function recordPolicy(kind: unknown): RecordPolicy {
  const out: RecordPolicy = { ...DEFAULT_RECORD_POLICY }
  const policy = isObject(kind) && isObject(kind.policy) ? kind.policy : null
  if (!policy) return out
  for (const op of RECORD_POLICY_OPS) {
    const value = own(policy, op) ? policy[op] : undefined
    if (typeof value === `string` && (RECORD_POLICY_LEVELS as readonly string[]).includes(value) && !(op === `create` && value === `author`)) out[op] = value as RecordLevel
  }
  return out
}

/** The author of a record (`authorId ?? createdBy`), or null. */
export const recordAuthorOf = (row: RecordOwnership | null | undefined): string | null => row?.authorId ?? row?.createdBy ?? null

/** Why `viewer` may NOT `op` a record of a kind with `policy` (`row` = the
 *  record, for read/update/delete), or null when they may. Order: not a
 *  member → `none`; access `none`; access `read` and a write; a team owner
 *  may; the op's level. */
export function recordRefusal(policy: RecordPolicy, op: RecordOp, viewer: RecordViewer, row?: RecordOwnership | null): string | null {
  const access = viewer.access ?? `write`
  if (viewer.id === null || viewer.role === null || access === `none`) return RECORD_POLICY_COPY.none
  if (access === `read` && op !== `read`) return RECORD_POLICY_COPY.readOnly
  if (viewer.role === `owner`) return null
  const level = policy[op]
  if (level === `member`) return null
  if (level === `owners` || op === `create`) return RECORD_POLICY_COPY.owners[op]
  const author = recordAuthorOf(row)
  return author !== null && author === viewer.id ? null : RECORD_POLICY_COPY.author[op]
}

/** May `viewer` `op` the record? (`recordRefusal` is null.) */
export const recordAllowed = (policy: RecordPolicy, op: RecordOp, viewer: RecordViewer, row?: RecordOwnership | null): boolean => recordRefusal(policy, op, viewer, row) === null

/** The rows `viewer` may read, each judged by its kind's policy (`kinds` =
 *  the package's `records`): the "Run as" preview and package tests. */
export function readableRows<T extends RecordOwnership & { kind: string }>(rows: readonly T[], kinds: Record<string, unknown> | undefined, viewer: RecordViewer): T[] {
  const policies = new Map<string, RecordPolicy>()
  const policyOf = (kind: string) => {
    let p = policies.get(kind)
    if (!p) policies.set(kind, (p = recordPolicy(kinds && own(kinds, kind) ? kinds[kind] : undefined)))
    return p
  }
  return rows.filter((r) => recordAllowed(policyOf(r.kind), `read`, viewer, r))
}
