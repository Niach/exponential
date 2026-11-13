// @vitest-environment node
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import { applyDueContractSteps, recordContractSteps } from "./contract-runner"
import type { ContractStep } from "./contract-steps"

// #ERR-1: deferred contract steps run only after the rollout grace, once,
// and a rolled-back build (one that does not ship a pending step) resets it.

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'sessions') AS ok`)
if (!ready.ok) console.info(`[contract-runner.real-db] skipped: ${ready.reason}`)

const GRACE = 30 * 60 * 1000

async function probeTable(tx: RealDbTx): Promise<{ table: string; step: ContractStep }> {
  const table = `exp_contract_probe_${crypto.randomUUID().slice(0, 8)}`
  await tx.execute(sql.raw(`CREATE TABLE "${table}" (id int, retired text)`))
  return {
    table,
    step: {
      id: `test_drop_${table}`,
      drops: [{ table, column: `retired` }],
      sql: `ALTER TABLE "${table}" DROP COLUMN IF EXISTS "retired"`,
    },
  }
}

const hasColumn = async (tx: RealDbTx, table: string) =>
  (
    await tx.execute<{ n: number }>(
      sql`SELECT count(*)::int AS n FROM information_schema.columns WHERE table_name = ${table} AND column_name = 'retired'`
    )
  ).rows[0]!.n === 1

const ledger = async (tx: RealDbTx, id: string) =>
  (
    await tx.execute<{ applied: boolean }>(
      sql`SELECT applied_at IS NOT NULL AS applied FROM drizzle.__exp_contract_steps WHERE id = ${id}`
    )
  ).rows[0]

describe.skipIf(!ready.ok)(`contract steps (real DB)`, () => {
  it(`wait out the grace after their first sighting, then apply once`, async () => {
    await inRollback(async (tx) => {
      const { table, step } = await probeTable(tx)
      await recordContractSteps(tx, [step])
      expect(await applyDueContractSteps(tx, [step], GRACE)).toEqual([])
      expect(await hasColumn(tx, table)).toBe(true)

      await tx.execute(sql`UPDATE drizzle.__exp_contract_steps SET first_seen_at = now() - interval '31 minutes' WHERE id = ${step.id}`)
      // A reboot of the same build keeps the first sighting.
      await recordContractSteps(tx, [step])
      expect(await applyDueContractSteps(tx, [step], GRACE)).toEqual([step.id])
      expect(await hasColumn(tx, table)).toBe(false)
      expect(await ledger(tx, step.id)).toEqual({ applied: true })
      expect(await applyDueContractSteps(tx, [step], GRACE)).toEqual([])
    })
  })

  it(`forget a pending step when a build without it boots (rollback)`, async () => {
    await inRollback(async (tx) => {
      const { table, step } = await probeTable(tx)
      await recordContractSteps(tx, [step])
      await tx.execute(sql`UPDATE drizzle.__exp_contract_steps SET first_seen_at = now() - interval '2 hours' WHERE id = ${step.id}`)
      await recordContractSteps(tx, [])
      expect(await ledger(tx, step.id)).toBeUndefined()
      // Redeployed: the grace starts over.
      await recordContractSteps(tx, [step])
      expect(await applyDueContractSteps(tx, [step], GRACE)).toEqual([])
      expect(await hasColumn(tx, table)).toBe(true)
    })
  })

  it(`apply at once with no grace (CI's from-scratch drift check)`, async () => {
    await inRollback(async (tx) => {
      const { table, step } = await probeTable(tx)
      await recordContractSteps(tx, [step])
      expect(await applyDueContractSteps(tx, [step], 0)).toEqual([step.id])
      expect(await hasColumn(tx, table)).toBe(false)
    })
  })
})
