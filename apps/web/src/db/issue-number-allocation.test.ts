import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { getTableConfig } from "drizzle-orm/pg-core"
import { issueNumberCounters, issues } from "@/db/schema"

// Locks the issue-number allocation contract (REV-9): identifiers are handed
// out by generate_issue_number() from the per-board monotonic counter table
// issue_number_counters (row-locked via INSERT … ON CONFLICT DO UPDATE — no
// duplicate numbers under concurrency, no identifier recycling after the
// top-numbered issue is deleted), with the unique index
// uniq_issues_board_number as the loud backstop. This test can't prove
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
  it(`issues has the uniq_issues_board_number unique index on (board_id, number)`, () => {
    const { indexes } = getTableConfig(issues)
    const backstop = indexes.find(
      (idx) => idx.config.name === `uniq_issues_board_number`
    )
    expect(backstop).toBeDefined()
    expect(backstop!.config.unique).toBe(true)
    const columnNames = backstop!.config.columns.map(
      (column) => (column as { name?: string }).name
    )
    expect(columnNames).toEqual([`board_id`, `number`])
  })

  it(`issue_number_counters is keyed by board_id with a NOT NULL counter`, () => {
    const config = getTableConfig(issueNumberCounters)
    expect(config.name).toBe(`issue_number_counters`)
    const primaryColumns = config.columns.filter((column) => column.primary)
    expect(primaryColumns.map((column) => column.name)).toEqual([`board_id`])
    const counter = config.columns.find((column) => column.name === `counter`)
    expect(counter).toBeDefined()
    expect(counter!.notNull).toBe(true)
  })

  it(`generate_issue_number allocates from the locked counter row, not MAX+1`, () => {
    // The allocator must take the counter row lock (serializes concurrent
    // same-board inserts) …
    expect(triggersSql).toContain(`issue_number_counters`)
    expect(triggersSql).toContain(`ON CONFLICT (board_id) DO UPDATE`)
    // … and must never regress to the racy unlocked MAX+1 read that let two
    // concurrent inserts commit the same identifier (and recycled the numbers
    // of deleted top issues).
    expect(triggersSql).not.toMatch(/COALESCE\(MAX\(number\), 0\) \+ 1/)
  })

  // EXP-630: a tracker import supplies the source number so identifiers
  // survive migration. The explicit branch must still go through the counter
  // upsert (same row lock as ordinary inserts) and clamp the counter PAST the
  // supplied number so later allocations can never collide with it — and the
  // ordinary allocation must remain for every insert that leaves number at
  // its drizzle default of 0.
  it(`generate_issue_number honours an explicitly supplied number and clamps the counter past it`, () => {
    const body = triggersSql.slice(
      triggersSql.indexOf(`CREATE OR REPLACE FUNCTION generate_issue_number()`),
      triggersSql.indexOf(`CREATE OR REPLACE TRIGGER generate_issue_number`)
    )
    expect(body).toContain(`IF NEW.number IS NOT NULL AND NEW.number > 0 THEN`)
    expect(body).toContain(
      `SET counter = GREATEST(c.counter, current_max, NEW.number)`
    )
    expect(body).toContain(`next_number := NEW.number;`)
    // The ordinary path is untouched behind the ELSE.
    expect(body).toContain(`SET counter = GREATEST(c.counter, current_max) + 1`)
    expect(body).toContain(`RETURNING counter INTO next_number`)
    // Both branches still derive the identifier from the board prefix.
    expect(body).toContain(`NEW.identifier := board_prefix || '-' || next_number;`)
  })

  it(`issue_number_counters gets the shared update_updated_at trigger`, () => {
    expect(triggersSql).toContain(
      `CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON issue_number_counters`
    )
  })
})
