import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { is } from "drizzle-orm"
import { getTableConfig, PgTable } from "drizzle-orm/pg-core"
import * as schema from "@/db/schema"

// Locks the updated_at maintenance contract (REV2-70): every table that carries
// an updated_at column either gets the shared update_updated_at trigger in
// 0001_triggers.sql, or is listed below with the reason its writers stamp the
// column themselves. creem_subscriptions used to be silently in neither camp —
// frozen at insert through seat changes, plan switches and team re-binding.
// NOT a `new URL(..., import.meta.url)` construction — Vite's asset transform
// rewrites that pattern to a non-file URL under the jsdom test environment.
const __dirname = dirname(fileURLToPath(import.meta.url))
const triggersSql = readFileSync(
  join(__dirname, `out`, `custom`, `0001_triggers.sql`),
  `utf8`
)

// Tables that carry updated_at but deliberately have NO trigger.
const APP_STAMPED: Record<string, string> = {
  // Better Auth owns these tables and its Drizzle adapter applies its own
  // onUpdate hook to the declared updatedAt field on every write.
  users: `better-auth adapter stamps it`,
  sessions: `better-auth adapter stamps it`,
  accounts: `better-auth adapter stamps it`,
  verifications: `better-auth adapter stamps it`,
  apikeys: `better-auth adapter stamps it`,
  oauth_applications: `better-auth adapter stamps it`,
  oauth_access_tokens: `better-auth adapter stamps it`,
  oauth_consents: `better-auth adapter stamps it`,
  // The support tables stamp updated_at SELECTIVELY: the member inbox sorts by
  // it, and the reporter's read-receipt / poll heartbeat writes
  // (routes/api/support/thread.ts, poll.ts) must not reorder that list. A
  // blanket trigger would bump the thread on every poll.
  support_threads: `stamped selectively — reporter read receipts must not bump`,
  support_messages: `thread bump is the ordering signal, not the message row`,
  // Single writer (the SES webhook), which stamps updatedAt on both the upsert
  // and the auto-suppress write.
  email_bounces: `ses webhook stamps it on every write`,
  // EXP-403: the devices router is the ONLY writer and stamps updatedAt on
  // register/heartbeat/rename explicitly (last_seen_at is the liveness
  // column; updated_at just mirrors the same writes).
  devices: `devices router stamps it on every write`,
  // EXP-702: write-once steer images for issue-less sessions — nothing ever
  // updates a row after insert (only the session FK's SET NULL touches it).
  session_attachments: `write-once — no app writer updates rows`,
}

function tablesWithUpdatedAt(): string[] {
  const names: string[] = []
  for (const value of Object.values(schema)) {
    if (!is(value, PgTable)) continue
    const config = getTableConfig(value)
    if (config.columns.some((column) => column.name === `updated_at`)) {
      names.push(config.name)
    }
  }
  return names.sort()
}

function hasUpdatedAtTrigger(table: string): boolean {
  return triggersSql.includes(
    `CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON ${table} FOR EACH ROW`
  )
}

describe(`updated_at triggers`, () => {
  it(`covers every table carrying updated_at, or documents why it does not`, () => {
    const uncovered = tablesWithUpdatedAt().filter(
      (table) => !hasUpdatedAtTrigger(table) && !(table in APP_STAMPED)
    )
    expect(uncovered).toEqual([])
  })

  it(`stamps creem_subscriptions on seat/plan/binding and webhook updates`, () => {
    // The Creem plugin's model declares no updatedAt field, so better-auth's
    // adapter never stamps it and app-side .set() calls cannot cover the
    // webhook persistence path — only the trigger does.
    expect(hasUpdatedAtTrigger(`creem_subscriptions`)).toBe(true)
  })

  it(`has no stale entries in the app-stamped exemption list`, () => {
    const carriers = new Set(tablesWithUpdatedAt())
    const unknown = Object.keys(APP_STAMPED).filter(
      (table) => !carriers.has(table)
    )
    expect(unknown).toEqual([])
  })

  it(`guards the board mirror tables with a WHEN clause`, () => {
    // REV2-5: the board trash/restore fan-out only flips board_deleted_at, and
    // bumping updated_at there would stamp a whole board's history as freshly
    // edited on restore. EXP-500 adds the same guard for board_archived_at —
    // archive/unarchive is the same bookkeeping flip, and an unarchive that
    // restamped a whole board's history would be the same bug. REV-49 extends
    // the ISSUE-CHILD guards with a
    // board_id arm: issues.move's re-point UPDATEs only change board_id, and
    // restamping every comment/attachment/... on a move is the same class of
    // bookkeeping churn. issues itself must NOT carry the board_id arm — the
    // move's own UPDATE (board_id + number + identifier) has to bump the
    // issue exactly once.
    for (const value of Object.values(schema)) {
      if (!is(value, PgTable)) continue
      const config = getTableConfig(value)
      const mirrors = config.columns.some(
        (column) => column.name === `board_deleted_at`
      )
      if (!mirrors || !hasUpdatedAtTrigger(config.name)) continue
      const statement = triggersSql.slice(
        triggersSql.indexOf(
          `CREATE OR REPLACE TRIGGER update_updated_at BEFORE UPDATE ON ${config.name} FOR EACH ROW`
        )
      )
      const trigger = statement.slice(0, statement.indexOf(`;`))
      expect(trigger).toContain(
        `NEW.board_deleted_at IS NOT DISTINCT FROM OLD.board_deleted_at`
      )
      expect(trigger).toContain(
        `NEW.board_archived_at IS NOT DISTINCT FROM OLD.board_archived_at`
      )
      if (config.name === `issues`) {
        expect(trigger).not.toContain(
          `NEW.board_id IS NOT DISTINCT FROM OLD.board_id`
        )
      } else {
        expect(trigger).toContain(
          `NEW.board_id IS NOT DISTINCT FROM OLD.board_id`
        )
      }
    }
  })

  it(`skips the comment→issue bump for board re-points and trash fan-outs`, () => {
    // REV-49: bump_issue_updated_at_from_comment must treat every bookkeeping
    // rewrite of comment rows — the trash fan-out's board_deleted_at flip, the
    // EXP-500 archive fan-out's board_archived_at flip, AND the move's
    // board_id re-point — as non-discussion, or a single
    // move/trash/archive amplifies into one issues-UPDATE (and one Electric op
    // fan-out) per comment.
    const start = triggersSql.indexOf(
      `CREATE OR REPLACE FUNCTION bump_issue_updated_at_from_comment()`
    )
    const body = triggersSql.slice(start, triggersSql.indexOf(`$$ LANGUAGE plpgsql;`, start))
    expect(body).toContain(
      `NEW.board_deleted_at IS DISTINCT FROM OLD.board_deleted_at`
    )
    expect(body).toContain(
      `NEW.board_archived_at IS DISTINCT FROM OLD.board_archived_at`
    )
    expect(body).toContain(`NEW.board_id IS DISTINCT FROM OLD.board_id`)
  })
})
