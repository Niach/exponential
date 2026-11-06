// #EXP-1713: the tables a team import may carry (fixtures/team-import.json
// `tables`, in dependency order) and what the commit derives from the
// drizzle schema: each table's columns, which columns are foreign keys into
// another imported table (ids follow a remap), into `users` (the identity
// rule), into `teams` (the new team), and which reference the table itself
// (written after the whole table is in). Pure metadata, no database.
import { getTableConfig, type PgTable } from "drizzle-orm/pg-core"
import {
  agents,
  appRecords,
  appVersions,
  apps,
  attachments,
  boards,
  comments,
  conversations,
  issueLabels,
  issueRelations,
  issueStatuses,
  issues,
  labels,
  messages,
  prompts,
  sessionIssues,
  sessions,
} from "@/db/schema"
import { TEAM_IMPORT_TABLES } from "@exp/domain-contract/team-import"

export const IMPORT_TABLES: Readonly<Record<string, PgTable>> = {
  boards,
  issue_statuses: issueStatuses,
  labels,
  agents,
  prompts,
  apps,
  issues,
  issue_labels: issueLabels,
  issue_relations: issueRelations,
  comments,
  sessions,
  session_issues: sessionIssues,
  conversations,
  messages,
  app_versions: appVersions,
  app_records: appRecords,
  attachments,
}

/** Columns the server never takes from a local row: what stays local
 *  (repositories, worktrees, skills, MCP servers, drafts, email deliveries). */
export const DROPPED_COLUMNS: Readonly<Record<string, readonly string[]>> = {
  boards: [`repository_id`, `default_branch`],
  issues: [`draft_id`, `board_deleted_at`, `board_archived_at`],
  issue_labels: [`board_deleted_at`, `board_archived_at`],
  issue_relations: [`board_deleted_at`, `board_archived_at`],
  comments: [`email_delivery_id`, `board_deleted_at`, `board_archived_at`],
  attachments: [`draft_id`, `board_deleted_at`, `board_archived_at`],
  sessions: [`repository_id`, `mcp_server_ids`, `skill_ids`, `wake_on`],
  agents: [`skill_ids`],
  app_records: [`app_deleted_at`],
}

/** Array columns of ids (no FK in the schema) that follow a remap. */
export const ARRAY_REFERENCES: Readonly<Record<string, Readonly<Record<string, string>>>> = {
  messages: { attachment_ids: `attachments`, ref_issue_ids: `issues`, mention_user_ids: `users` },
}

export interface ColumnMeta {
  name: string
  key: string
  notNull: boolean
  hasDefault: boolean
  /** drizzle's data type: `date` (timestamps), `json`, `array`, `string`, `number`, `boolean`, `bigint` */
  dataType: string
}

export interface TableMeta {
  name: string
  table: PgTable
  columns: ColumnMeta[]
  /** column name → the imported table it references (ids follow a remap) */
  references: Record<string, string>
  /** column names referencing `users.id` (the identity rule) */
  userColumns: string[]
  /** column names referencing `teams.id` (the new team) */
  teamColumns: string[]
  /** nullable columns referencing this very table (written after the insert) */
  selfReferences: string[]
  hasId: boolean
}

function buildMeta(name: string, table: PgTable): TableMeta {
  const config = getTableConfig(table)
  const columns = config.columns.map((c) => ({
    name: c.name,
    key: columnKey(table, c.name),
    notNull: c.notNull,
    hasDefault: c.hasDefault,
    dataType: c.dataType,
  }))
  const references: Record<string, string> = {}
  const userColumns: string[] = []
  const teamColumns: string[] = []
  const selfReferences: string[] = []
  for (const fk of config.foreignKeys) {
    const ref = fk.reference()
    const column = ref.columns[0]
    const target = getTableConfig(ref.foreignTable).name
    if (!column || ref.columns.length !== 1) continue
    if (target === `users`) userColumns.push(column.name)
    else if (target === `teams`) teamColumns.push(column.name)
    else if (target === name) {
      if (column.notNull) references[column.name] = target
      else selfReferences.push(column.name)
    } else if (target in IMPORT_TABLES) references[column.name] = target
  }
  return { name, table, columns, references, userColumns, teamColumns, selfReferences, hasId: columns.some((c) => c.name === `id`) }
}

function columnKey(table: PgTable, columnName: string): string {
  for (const [key, column] of Object.entries(table as unknown as Record<string, { name?: string }>)) {
    if (column && typeof column === `object` && column.name === columnName) return key
  }
  return columnName
}

const META = new Map<string, TableMeta>()

/** The metadata of an imported table, built once from the schema. */
export function tableMeta(name: string): TableMeta {
  let meta = META.get(name)
  if (!meta) {
    const table = IMPORT_TABLES[name]
    if (!table) throw new Error(`not an import table: ${name}`)
    meta = buildMeta(name, table)
    META.set(name, meta)
  }
  return meta
}

/** Every import table's metadata, in the fixture's order. */
export function allTableMeta(): TableMeta[] {
  return TEAM_IMPORT_TABLES.map((name) => tableMeta(name))
}
