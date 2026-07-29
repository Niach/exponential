import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"

// Locks the drizzle journal ordering contract (EXP-361): drizzle-kit migrate
// applies a migration ONLY when its journal `when` exceeds the max created_at
// already recorded in the target DB — an entry stamped below its predecessor
// is silently skipped on every DB that migrated past that predecessor.
// 0054_team_tier_rebrand was hand-stamped with a FUTURE `when`
// (1785650000000 ≈ 2026-08-02), so the auto-generated 0055 (ADD VALUE
// 'merged') sorted below it and never applied on prod/staging — "Merge and
// close" then died with: invalid input value for enum coding_session_status.
//
// Until real time passes the newest (future) stamp, `drizzle-kit generate`
// keeps minting Date.now() stamps that sort below it. If this test fails on
// a freshly generated migration: bump its `when` in meta/_journal.json above
// the previous entry's, and keep the migration idempotent (IF NOT EXISTS) in
// case some environment ends up replaying it.
// NOT a `new URL(..., import.meta.url)` construction — Vite's asset transform
// rewrites that pattern to a non-file URL under the jsdom test environment.
const __dirname = dirname(fileURLToPath(import.meta.url))
const journal = JSON.parse(
  readFileSync(join(__dirname, `out`, `meta`, `_journal.json`), `utf8`)
) as { entries: Array<{ idx: number; when: number; tag: string }> }

describe(`migration journal`, () => {
  it(`has contiguous ascending idx values`, () => {
    journal.entries.forEach((entry, i) => {
      expect(entry.idx, `entry ${entry.tag}`).toBe(i)
    })
  })

  it(`has strictly increasing when stamps (drizzle skips out-of-order entries)`, () => {
    for (let i = 1; i < journal.entries.length; i++) {
      const prev = journal.entries[i - 1]
      const curr = journal.entries[i]
      expect(
        curr.when,
        `${curr.tag} (when=${curr.when}) must be stamped after ${prev.tag} ` +
          `(when=${prev.when}) or drizzle-kit migrate silently skips it on ` +
          `any DB that already applied ${prev.tag} — bump its when in ` +
          `meta/_journal.json`
      ).toBeGreaterThan(prev.when)
    }
  })
})
