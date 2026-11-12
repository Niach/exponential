// Expand/contract for the schema (#ERR-1). The web image runs
// `drizzle-kit migrate` at container start, and Coolify rolls the new
// container out WHILE the previous one still serves. Drizzle selects every
// column of its schema by name, so a migration that drops a column the old
// release still knows breaks every query of the old server until it stops:
// on 2026-11-11 0268 dropped `sessions.plan_mode` and the 0.20.54 server
// that booted at 11:14 failed `select … "plan_mode" … from "sessions"`.
//
// The rule, enforced by migration-safety.test.ts for every migration after
// MIGRATION_SAFETY_FROM_IDX:
// - a journal migration only EXPANDS: no DROP TABLE, DROP COLUMN, RENAME,
//   column type change, or ADD COLUMN … NOT NULL without a DEFAULT;
// - a drop is a CONTRACT step below. The server records each step it ships
//   at boot and applies it only once the step has been known for
//   CONTRACT_GRACE_MS (lib: contract-runner.ts), long after the previous
//   container has stopped. A step may only drop what the current schema no
//   longer has, so the release that records it never reads it.
// - a rename or type change = add the new column, backfill, move the code,
//   then drop the old one as a contract step.
//
// `drizzle-kit generate` still writes the DROP into the new migration (its
// snapshot must lose the column): move the statement here, leave the
// migration file with `SELECT 1;` or its other statements.

export const MIGRATION_SAFETY_FROM_IDX = 272

/** How long a step must be known to the database before it runs. */
export const CONTRACT_GRACE_MS = 30 * 60 * 1000

export type ContractDrop = { table: string; column?: string }

export type ContractStep = {
  /** Stable, unique id, e.g. `0273_drop_sessions_foo`. Never reuse one. */
  id: string
  /** What it removes; the gate checks the schema no longer has it. */
  drops: ContractDrop[]
  /** Idempotent SQL (`IF EXISTS`), statements separated by `;`. */
  sql: string
}

export const CONTRACT_STEPS: ContractStep[] = []

// A statement that breaks the release still serving during a rollout.
const DESTRUCTIVE: Array<{ re: RegExp; what: string }> = [
  { re: /\bDROP\s+TABLE\b/i, what: `DROP TABLE` },
  { re: /\bDROP\s+COLUMN\b/i, what: `DROP COLUMN` },
  { re: /\bRENAME\s+(COLUMN\b|TO\b)/i, what: `RENAME` },
  { re: /\bALTER\s+COLUMN\s+\S+\s+(SET\s+DATA\s+)?TYPE\b/i, what: `column type change` },
]

const ALLOW = /--\s*migration-safety:\s*allow\b/i

function stripComments(statement: string): string {
  return statement
    .split(`\n`)
    .map((line) => line.replace(/--.*$/, ``))
    .join(`\n`)
}

/** The statements of a drizzle migration file that would break the
 *  release still serving during a rollout. A statement carrying a
 *  `-- migration-safety: allow <reason>` comment is exempt. */
export function destructiveStatements(
  migrationSql: string
): Array<{ statement: string; what: string }> {
  const found: Array<{ statement: string; what: string }> = []
  for (const raw of migrationSql.split(`--> statement-breakpoint`)) {
    if (ALLOW.test(raw)) continue
    for (const statement of stripComments(raw).split(`;`)) {
      const text = statement.trim()
      if (!text) continue
      const hit = DESTRUCTIVE.find((d) => d.re.test(text))
      if (hit) {
        found.push({ statement: text, what: hit.what })
        continue
      }
      if (
        /\bADD\s+COLUMN\b/i.test(text) &&
        /\bNOT\s+NULL\b/i.test(text) &&
        !/\bDEFAULT\b/i.test(text) &&
        !/\bGENERATED\b/i.test(text)
      ) {
        found.push({ statement: text, what: `ADD COLUMN NOT NULL without DEFAULT` })
      }
    }
  }
  return found
}
