/**
 * #ERR-1: applies every deferred schema contract step (db/contract-steps.ts)
 * NOW, skipping the rollout grace. Only for a database no older release
 * serves: CI's migrated-from-scratch drift check and a fresh local stack.
 * Production applies them from the server (lib/contract-step-scheduler.ts).
 *
 * Usage (from apps/web, DATABASE_URL = the target database):
 *   bun run migrate:contract
 */
import { db, pool } from "@/db/connection"
import { applyDueContractSteps, recordContractSteps } from "@/db/contract-runner"
import { CONTRACT_STEPS } from "@/db/contract-steps"

await recordContractSteps(db, CONTRACT_STEPS)
const applied = await applyDueContractSteps(db, CONTRACT_STEPS, 0)
console.log(`[contract-steps] ${CONTRACT_STEPS.length} shipped, applied ${applied.join(`, `) || `none`}`)
await pool.end()
