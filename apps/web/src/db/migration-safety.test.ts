import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { getTableConfig, PgTable } from "drizzle-orm/pg-core"
import { is } from "drizzle-orm"
import * as schema from "@exp/db-schema/schema"
import * as authSchema from "./auth-schema"
import {
  CONTRACT_STEPS,
  MIGRATION_SAFETY_FROM_IDX,
  destructiveStatements,
} from "./contract-steps"

// #ERR-1: a migration runs while the previous release still serves (the
// image migrates at container start, Coolify rolls it out beside the old
// one), so it may only EXPAND the schema. Drops wait as contract steps
// (contract-steps.ts explains the rule and how to move a generated DROP).
const __dirname = dirname(fileURLToPath(import.meta.url))
const out = join(__dirname, `out`)
const journal = JSON.parse(readFileSync(join(out, `meta`, `_journal.json`), `utf8`)) as {
  entries: Array<{ idx: number; tag: string }>
}
const migrationSql = (tag: string) => readFileSync(join(out, `${tag}.sql`), `utf8`)

const columnsByTable = new Map<string, Set<string>>()
for (const value of [...Object.values(schema), ...Object.values(authSchema)]) {
  if (!is(value, PgTable)) continue
  const config = getTableConfig(value)
  columnsByTable.set(config.name, new Set(config.columns.map((c) => c.name)))
}

describe(`migration safety (expand only)`, () => {
  it(`catches the ERR-1 incident: 0268 dropped sessions.plan_mode in-band`, () => {
    expect(destructiveStatements(migrationSql(`0268_silky_ma_gnuci`))).toEqual([
      { statement: `ALTER TABLE "sessions" DROP COLUMN "plan_mode"`, what: `DROP COLUMN` },
    ])
  })

  it(`grandfathers only the migrations before the rule`, () => {
    expect(MIGRATION_SAFETY_FROM_IDX).toBeLessThanOrEqual(journal.entries.length)
  })

  for (const entry of journal.entries.filter((e) => e.idx >= MIGRATION_SAFETY_FROM_IDX)) {
    it(`${entry.tag} only expands the schema`, () => {
      const found = destructiveStatements(migrationSql(entry.tag))
      expect(
        found,
        `${entry.tag} would break the release still serving during the rollout: ` +
          `${found.map((f) => `${f.what}: ${f.statement}`).join(` | `)}. Move a drop into ` +
          `apps/web/src/db/contract-steps.ts (CONTRACT_STEPS); a rename or type change = ` +
          `add the new column, backfill, then drop the old one as a contract step`
      ).toEqual([])
    })
  }
})

describe(`destructiveStatements`, () => {
  const flagged = (s: string) => destructiveStatements(s).map((f) => f.what)
  it(`flags drops, renames, type changes and NOT NULL adds without a default`, () => {
    expect(flagged(`DROP TABLE "x";`)).toEqual([`DROP TABLE`])
    expect(flagged(`ALTER TABLE "x" RENAME COLUMN "a" TO "b";`)).toEqual([`RENAME`])
    expect(flagged(`ALTER TABLE "x" RENAME TO "y";`)).toEqual([`RENAME`])
    expect(flagged(`ALTER TABLE "x" ALTER COLUMN "a" SET DATA TYPE text;`)).toEqual([`column type change`])
    expect(flagged(`ALTER TABLE "x" ADD COLUMN "a" text NOT NULL;`)).toEqual([`ADD COLUMN NOT NULL without DEFAULT`])
  })
  it(`lets additive DDL, comments and explicit exemptions through`, () => {
    expect(flagged(`ALTER TABLE "x" ADD COLUMN "a" text DEFAULT 'v' NOT NULL;`)).toEqual([])
    expect(flagged(`CREATE TABLE "x" ("a" text NOT NULL);`)).toEqual([])
    expect(flagged(`DROP INDEX "x_idx";--> statement-breakpoint\nALTER TABLE "x" DROP CONSTRAINT "c";`)).toEqual([])
    expect(flagged(`-- DROP COLUMN in a comment\nSELECT 1;`)).toEqual([])
    expect(flagged(`-- migration-safety: allow never shipped\nDROP TABLE "x";`)).toEqual([])
  })
})

describe(`contract steps`, () => {
  it(`read the schema the server selects from`, () => {
    expect(columnsByTable.get(`sessions`)?.has(`root_id`)).toBe(true)
    expect(columnsByTable.get(`users`)?.has(`email`)).toBe(true)
  })
  it(`have unique ids`, () => {
    const ids = CONTRACT_STEPS.map((s) => s.id)
    expect(new Set(ids).size).toBe(ids.length)
  })
  for (const step of CONTRACT_STEPS) {
    it(`${step.id} drops only what the schema no longer reads, idempotently`, () => {
      expect(step.drops.length).toBeGreaterThan(0)
      for (const drop of step.drops) {
        if (drop.column) {
          expect(columnsByTable.get(drop.table)?.has(drop.column), `${drop.table}.${drop.column} is still in the schema`).not.toBe(true)
        } else {
          expect(columnsByTable.has(drop.table), `table ${drop.table} is still in the schema`).toBe(false)
        }
      }
      const drops = step.sql.match(/\bDROP\s+(TABLE|COLUMN)\b(?!\s+IF\s+EXISTS)/gi) ?? []
      expect(drops, `${step.id}: every DROP needs IF EXISTS`).toEqual([])
    })
  }
})
