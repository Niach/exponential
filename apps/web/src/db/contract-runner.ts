// Runs the deferred contract steps of contract-steps.ts (#ERR-1).
//
// The ledger lives next to drizzle's own (`drizzle.__exp_contract_steps`),
// outside packages/db-schema: it is migration bookkeeping, not app data.
//
// - At boot, recordContractSteps() stamps every shipped step's first sighting
//   and forgets the unapplied steps this build does not ship (an older build
//   booting = a rollback: its successor's drops must wait for the next
//   rollout's full grace again).
// - applyDueContractSteps() runs every step known for CONTRACT_GRACE_MS,
//   once, under a transaction-scoped advisory lock (multi-instance safe).
//   It never runs at boot: during a rollout the previous container still
//   serves, and the grace outlasts it.

import { sql } from "drizzle-orm"
import type { NodePgDatabase } from "drizzle-orm/node-postgres"
import { CONTRACT_GRACE_MS, type ContractStep } from "./contract-steps"

type Db = Pick<NodePgDatabase, `execute` | `transaction`>

// pg_advisory lock key: "ERR1" in ASCII.
const LOCK_KEY = 0x45525231

const ENSURE_LEDGER = sql`
  CREATE SCHEMA IF NOT EXISTS drizzle;
  CREATE TABLE IF NOT EXISTS drizzle.__exp_contract_steps (
    id text PRIMARY KEY,
    first_seen_at timestamptz NOT NULL DEFAULT now(),
    applied_at timestamptz
  );
`

export async function recordContractSteps(
  db: Db,
  steps: ContractStep[]
): Promise<void> {
  await db.execute(ENSURE_LEDGER)
  const ids = steps.map((s) => s.id)
  await db.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(${LOCK_KEY})`)
    await tx.execute(
      sql`DELETE FROM drizzle.__exp_contract_steps
          WHERE applied_at IS NULL AND NOT (id = ANY(${sql.param(ids)}::text[]))`
    )
    if (ids.length > 0) {
      await tx.execute(
        sql`INSERT INTO drizzle.__exp_contract_steps (id)
            SELECT unnest(${sql.param(ids)}::text[])
            ON CONFLICT (id) DO NOTHING`
      )
    }
  })
}

/** Applies every shipped step known for at least `graceMs` and not yet
 *  applied; returns the ids it applied. `graceMs: 0` = now (CI's drift
 *  check, a fresh database). */
export async function applyDueContractSteps(
  db: Db,
  steps: ContractStep[],
  graceMs: number = CONTRACT_GRACE_MS
): Promise<string[]> {
  if (steps.length === 0) return []
  await db.execute(ENSURE_LEDGER)
  return db.transaction(async (tx) => {
    const locked = await tx.execute<{ ok: boolean }>(
      sql`SELECT pg_try_advisory_xact_lock(${LOCK_KEY}) AS ok`
    )
    if (!locked.rows[0]?.ok) return []
    const due = await tx.execute<{ id: string }>(
      sql`SELECT id FROM drizzle.__exp_contract_steps
          WHERE applied_at IS NULL
            AND first_seen_at <= now() - make_interval(secs => ${graceMs / 1000}::double precision)
          ORDER BY id`
    )
    const dueIds = new Set(due.rows.map((r) => r.id))
    const applied: string[] = []
    for (const step of steps) {
      if (!dueIds.has(step.id)) continue
      await tx.execute(sql.raw(step.sql))
      await tx.execute(
        sql`UPDATE drizzle.__exp_contract_steps SET applied_at = now() WHERE id = ${step.id}`
      )
      applied.push(step.id)
    }
    return applied
  })
}
