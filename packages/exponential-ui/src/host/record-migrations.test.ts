// eApps R8 (VAPP-233): fixtures/record-migrations.json replayed through the TS reference.

import { describe, expect, test } from "bun:test"
import fixture from "../../fixtures/record-migrations.json" with { type: "json" }
import { RECORD_MIGRATION_OPS_MAX, RECORD_MIGRATIONS_MAX, isVersionRange, kindChanges, migrateRecord, migrationRefusal, packageMigrations, rangeHolds, validateMigrations } from "./record-migrations"

type Case = Record<string, unknown> & { name: string }

describe(`record-migrations.json`, () => {
  test(`the limits`, () => expect({ migrations: RECORD_MIGRATIONS_MAX, ops: RECORD_MIGRATION_OPS_MAX }).toEqual(fixture.limits))
  for (const c of fixture.rangeCases) test(`range: ${c.version} ${c.range}`, () => expect({ holds: rangeHolds(c.version, c.range), range: isVersionRange(c.range) }).toEqual(c.expected))
  for (const c of fixture.migrateCases as Case[])
    test(`migrate: ${c.name}`, () => expect(migrateRecord(packageMigrations(c.package), c.version as string, c.kind as string, c.data as Record<string, unknown>)).toEqual(c.expected as never))
  for (const c of fixture.validateCases as Case[]) test(`validate: ${c.name}`, () => expect(validateMigrations(c.package)).toEqual(c.expected as never))
  for (const c of fixture.changeCases as Case[]) test(`changes: ${c.name}`, () => expect(kindChanges(c.before, c.after)).toEqual(c.expected as never))
  for (const c of fixture.refusalCases as Case[]) test(`refusal: ${c.name}`, () => expect(migrationRefusal(c.before, c.after)).toEqual(c.expected as never))
})
