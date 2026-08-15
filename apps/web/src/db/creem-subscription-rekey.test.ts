import { describe, expect, it } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, join } from "node:path"
import { getTableConfig } from "drizzle-orm/pg-core"
import { creem_subscriptions } from "@/db/schema"

// Locks the creem_subscriptions re-key protection (REV-12): the Creem
// plugin's webhook persistence falls back to matching by creem_customer_id
// when its creem_subscription_id lookup misses (a subscription.* event
// landing before its checkout.completed) and then writes the NEW id onto
// whatever row it found — re-keying the customer's existing subscription row
// and silently merging two paying subscriptions into one. The
// reject_creem_subscription_rekey trigger makes the key immutable once set,
// and the unique index pins one row per subscription (it is also the
// arbiter for the bind-path upsert in lib/billing/creem-binding.ts). This
// test can't prove runtime behavior — it pins the schema + trigger-file
// shape so removing either protection fails CI.
// NOT a `new URL(..., import.meta.url)` construction — Vite's asset transform
// rewrites that pattern to a non-file URL under the jsdom test environment.
const __dirname = dirname(fileURLToPath(import.meta.url))
const triggersSql = readFileSync(
  join(__dirname, `out`, `custom`, `0001_triggers.sql`),
  `utf8`
)

describe(`creem_subscriptions re-key protection (REV-12)`, () => {
  it(`has the unique index on creem_subscription_id`, () => {
    const { indexes } = getTableConfig(creem_subscriptions)
    const backstop = indexes.find(
      (idx) =>
        idx.config.name === `uniq_creem_subscriptions_creem_subscription_id`
    )
    expect(backstop).toBeDefined()
    expect(backstop!.config.unique).toBe(true)
    const columnNames = backstop!.config.columns.map(
      (column) => (column as { name?: string }).name
    )
    expect(columnNames).toEqual([`creem_subscription_id`])
  })

  it(`rejects re-keying a row whose creem_subscription_id is already set`, () => {
    expect(triggersSql).toContain(
      `CREATE OR REPLACE FUNCTION reject_creem_subscription_rekey()`
    )
    expect(triggersSql).toContain(
      `CREATE OR REPLACE TRIGGER reject_creem_subscription_rekey`
    )
    // The guard must fire only on an actual key CHANGE: setting the key on a
    // row that never had one (NULL) is creation, not theft, and the plugin's
    // same-id updates must keep flowing.
    expect(triggersSql).toContain(`WHEN (OLD.creem_subscription_id IS NOT NULL`)
    expect(triggersSql).toContain(
      `NEW.creem_subscription_id IS DISTINCT FROM OLD.creem_subscription_id`
    )
  })
})
