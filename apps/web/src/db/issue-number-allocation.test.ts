import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { getTableConfig } from "drizzle-orm/pg-core"
import { issueNumberCounters, issues } from "@/db/schema"

// Locks the issue-number allocation contract (REV-9): identifiers are handed
// out by generate_issue_number() from the per-project monotonic counter table
// issue_number_counters (row-locked via INSERT … ON CONFLICT DO UPDATE — no
// duplicate numbers under concurrency, no identifier recycling after the
// top-numbered issue is deleted), with the unique index
// uniq_issues_project_number as the loud backstop. This test can't prove
// runtime interleaving — it pins the schema + trigger-file shape so a revert
// to the racy unlocked SELECT MAX(number)+1 allocator fails CI.
// NOT a `new URL(..., import.meta.url)` construction — Vite's asset transform
// rewrites that pattern to a non-file URL under the jsdom test environment.
const __dirname = dirname(fileURLToPath(import.meta.url))
const triggersSql = readFileSync(
  join(__dirname, `out`, `custom`, `0001_triggers.sql`),
  `utf8`
)

describe(`issue number allocation`, () => {
  it(`issues has the uniq_issues_project_number unique index on (project_id, number)`, () => {
    const { indexes } = getTableConfig(issues)
    const backstop = indexes.find(
      (idx) => idx.config.name === `uniq_issues_project_number`
    )
    expect(backstop).toBeDefined()
    expect(backstop!.config.unique).toBe(true)
    const columnNames = backstop!.config.columns.map(
      (column) => (column as { name?: string }).name
    )
    expect(columnNames).toEqual([`project_id`, `number`])
  })

  it(`issue_number_counters is keyed by project_id with a NOT NULL counter`, () => {
    const config = getTableConfig(issueNumberCounters)
    expect(config.name).toBe(`issue_number_counters`)
    const primaryColumns = config.columns.filter((column) => column.primary)
    expect(primaryColumns.map((column) => column.name)).toEqual([`project_id`])
    const counter = config.columns.find((column) => column.name === `counter`)
    expect(counter).toBeDefined()
    expect(counter!.notNull).toBe(true)
  })

  it(`generate_issue_number allocates from the locked counter row, not MAX+1`, () => {
    // The allocator must take the counter row lock (serializes concurrent
    // same-project inserts) …
    expect(triggersSql).toContain(`issue_number_counters`)
    expect(triggersSql).toContain(`ON CONFLICT (project_id) DO UPDATE`)
    // … and must never regress to the racy unlocked MAX+1 read that let two
    // concurrent inserts commit the same identifier (and recycled the numbers
    // of deleted top issues).
    expect(triggersSql).not.toMatch(/COALESCE\(MAX\(number\), 0\) \+ 1/)
  })

  it(`issue_number_counters gets the shared update_updated_at trigger`, () => {
    expect(triggersSql).toContain(
      `CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_number_counters`
    )
  })
})
