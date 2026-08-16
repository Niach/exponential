import { describe, expect, it } from "vitest"
import { getTableConfig } from "drizzle-orm/pg-core"
import { conversionEvents } from "@/db/schema"

// Locks the conversion_events idempotency contract (EXP-362): writers always
// insert with ON CONFLICT DO NOTHING and rely on these PARTIAL UNIQUE indexes
// to define what "once" means (once per user for signup/first_issue_created,
// once per subscription for the paid lifecycle, once per visitor-day for
// landing — the cookieless anonymous id rotates daily — and once per user-day
// for return_visit). Silently dropping one
// of them would double-count funnel stages on every webhook redelivery.

describe(`conversion_events schema`, () => {
  const { indexes, columns } = getTableConfig(conversionEvents)
  const byName = new Map(indexes.map((idx) => [idx.config.name, idx.config]))

  it(`has the once-per-user partial unique index`, () => {
    const idx = byName.get(`uniq_conversion_events_once_per_user`)
    expect(idx).toBeDefined()
    expect(idx!.unique).toBe(true)
    expect(idx!.where).toBeDefined()
  })

  it(`has the once-per-subscription partial unique index`, () => {
    const idx = byName.get(`uniq_conversion_events_once_per_sub`)
    expect(idx).toBeDefined()
    expect(idx!.unique).toBe(true)
    expect(idx!.where).toBeDefined()
  })

  it(`has the landing-daily partial unique index`, () => {
    const idx = byName.get(`uniq_conversion_events_landing_daily`)
    expect(idx).toBeDefined()
    expect(idx!.unique).toBe(true)
    expect(idx!.where).toBeDefined()
  })

  it(`has the return-visit-daily partial unique index (EXP-522)`, () => {
    const idx = byName.get(`uniq_conversion_events_return_visit_daily`)
    expect(idx).toBeDefined()
    expect(idx!.unique).toBe(true)
    expect(idx!.where).toBeDefined()
  })

  it(`is append-only: created_at but no updated_at`, () => {
    const names = columns.map((column) => column.name)
    expect(names).toContain(`created_at`)
    expect(names).not.toContain(`updated_at`)
  })

  it(`user_id detaches on account deletion (funnel history survives)`, () => {
    const userId = columns.find((column) => column.name === `user_id`)
    expect(userId).toBeDefined()
    expect(userId!.notNull).toBe(false)
  })
})
