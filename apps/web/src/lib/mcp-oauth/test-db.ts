// EXP-792 test helper: a table-aware in-memory stand-in for the drizzle
// query builder, keyed on the REAL schema tables (so the code under test
// keeps its `eq(mcpServers.id, …)` references). Supports the chains the
// mcpServers router, the OAuth flow helpers and the callback use:
// select().from(t)[.innerJoin()].where(c)[.orderBy()][.limit()],
// insert(t).values(v)[.returning()][.onConflictDoUpdate({target, set})],
// update(t).set(s).where(c)[.returning()], delete(t).where(c),
// transaction(fn). `where` conditions are matched on their `eq`/`inArray`
// parts (a column followed by its value(s)); anything else is ignored.
import { randomUUID } from "node:crypto"
import { getTableName } from "drizzle-orm"

export type Row = Record<string, unknown>

type WherePart = { column?: string; value?: unknown }

function walkWhere(node: unknown, out: WherePart[] = []): WherePart[] {
  if (!node || typeof node !== `object`) return out
  if (Array.isArray(node)) {
    for (const child of node) walkWhere(child, out)
    return out
  }
  const rec = node as Record<string, unknown>
  if (Array.isArray(rec.queryChunks)) return walkWhere(rec.queryChunks, out)
  if (`value` in rec && `encoder` in rec) {
    out.push({ value: rec.value })
    return out
  }
  if (typeof rec.name === `string` && rec.table) {
    out.push({ column: rec.name })
    return out
  }
  return out
}

const camel = (name: string) =>
  name.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase())

/** Group a flattened condition into column → accepted values. */
function groups(cond: unknown): Array<{ key: string; values: unknown[] }> {
  const parts = walkWhere(cond)
  const out: Array<{ key: string; values: unknown[] }> = []
  for (const part of parts) {
    if (part.column !== undefined) {
      out.push({ key: camel(part.column), values: [] })
    } else if (out.length > 0) {
      out[out.length - 1]!.values.push(part.value)
    }
  }
  return out.filter((g) => g.values.length > 0)
}

function matches(row: Row, cond: unknown): boolean {
  return groups(cond).every((g) =>
    g.values.some((value) =>
      value instanceof Date && row[g.key] instanceof Date
        ? (value as Date).getTime() === (row[g.key] as Date).getTime()
        : row[g.key] === value
    )
  )
}

const TABLE_DEFAULTS: Record<string, () => Row> = {
  device_commands: () => ({ status: `pending`, result: null, completedAt: null }),
  mcp_oauth_flows: () => ({
    status: `pending`,
    authorizeUrl: null,
    error: null,
    completedAt: null,
    redirect: `hosted`,
  }),
  mcp_servers: () => ({
    headerNames: [],
    args: [],
    envNames: [],
    scopes: [],
    enabledByDefault: false,
    url: null,
    command: null,
    transport: `http`,
    auth: `none`,
    createdById: null,
  }),
  mcp_server_readiness: () => ({ expiresAt: null, error: null }),
}

export function createFakeDb(seed: Record<string, Row[]> = {}) {
  const tables = new Map<string, Row[]>()
  for (const [name, rows] of Object.entries(seed)) {
    tables.set(name, rows.map((row) => ({ ...row })))
  }
  const rowsOf = (name: string): Row[] => {
    let rows = tables.get(name)
    if (!rows) {
      rows = []
      tables.set(name, rows)
    }
    return rows
  }
  const name = (table: unknown) => getTableName(table as never)

  const select = () => {
    let root = ``
    let cond: unknown
    const run = () => rowsOf(root).filter((row) => matches(row, cond))
    const chain = {
      from: (table: unknown) => {
        root = name(table)
        return chain
      },
      innerJoin: () => chain,
      where: (value: unknown) => {
        cond = value
        return chain
      },
      orderBy: () => chain,
      limit: (n: number) => Promise.resolve(run().slice(0, n)),
      then: (
        onFulfilled?: (value: Row[]) => unknown,
        onRejected?: (reason: unknown) => unknown
      ) => Promise.resolve(run()).then(onFulfilled, onRejected),
    }
    return chain
  }

  const insert = (table: unknown) => {
    const tableName = name(table)
    return {
      values: (values: Row) => {
        const now = new Date()
        const row: Row = {
          id: randomUUID(),
          createdAt: now,
          updatedAt: now,
          ...(TABLE_DEFAULTS[tableName]?.() ?? {}),
          ...values,
        }
        let inserted: Row | null = null
        const commit = () => {
          if (!inserted) {
            rowsOf(tableName).push(row)
            inserted = row
          }
          return [inserted]
        }
        const done = {
          returning: () => Promise.resolve(commit()),
          then: (
            onFulfilled?: (value: unknown) => unknown,
            onRejected?: (reason: unknown) => unknown
          ) => Promise.resolve(commit()).then(onFulfilled, onRejected),
          onConflictDoUpdate: (args: { target: unknown[]; set: Row }) => {
            const keys = (args.target as Array<{ name: string }>).map((c) =>
              camel(c.name)
            )
            const existing = rowsOf(tableName).find((r) =>
              keys.every((key) => r[key] === row[key])
            )
            let result: Row[]
            if (existing) {
              Object.assign(existing, args.set)
              inserted = existing
              result = [existing]
            } else {
              result = commit()
            }
            return {
              returning: () => Promise.resolve(result),
              then: (
                onFulfilled?: (value: unknown) => unknown,
                onRejected?: (reason: unknown) => unknown
              ) => Promise.resolve(result).then(onFulfilled, onRejected),
            }
          },
        }
        return done
      },
    }
  }

  const update = (table: unknown) => {
    const tableName = name(table)
    return {
      set: (set: Row) => ({
        where: (cond: unknown) => {
          const hit = rowsOf(tableName).filter((row) => matches(row, cond))
          for (const row of hit) Object.assign(row, set)
          return {
            returning: () => Promise.resolve(hit),
            then: (
              onFulfilled?: (value: unknown) => unknown,
              onRejected?: (reason: unknown) => unknown
            ) => Promise.resolve(hit).then(onFulfilled, onRejected),
          }
        },
      }),
    }
  }

  const del = (table: unknown) => {
    const tableName = name(table)
    return {
      where: (cond: unknown) => {
        const rows = rowsOf(tableName)
        const keep = rows.filter((row) => !matches(row, cond))
        rows.splice(0, rows.length, ...keep)
        return Promise.resolve()
      },
    }
  }

  const db = {
    select,
    insert,
    update,
    delete: del,
    transaction: async <T>(fn: (tx: unknown) => Promise<T>): Promise<T> =>
      fn(db),
    /** Test access: the rows of one table by its SQL name. */
    rows: (tableName: string): Row[] => rowsOf(tableName),
  }
  return db
}

export type FakeDb = ReturnType<typeof createFakeDb>
