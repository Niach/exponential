import { describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// EXP-630 worker: the two SQL rules that keep a cancel and a purge honest —
// every worker write is fenced on a LIVE status plus the claim token, and
// the payload sweep touches only aged-out finished jobs. Rendered through
// PgDialect (the session-attachment-sweep.test.ts pattern); the worker's
// runtime is never started here.

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/metrics/registry`, () => ({ reportSchedulerRun: vi.fn() }))
vi.mock(`@/lib/import/apply-db`, () => ({ createDbApplyPorts: vi.fn() }))
vi.mock(`@/lib/import/sources`, () => ({ getImportSource: vi.fn() }))
vi.mock(`@/lib/import/team-state`, () => ({ loadImportTeamState: vi.fn() }))

import {
  claimFence,
  IMPORT_FAILED_PAYLOAD_TTL_MS,
  IMPORT_PAYLOAD_TTL_MS,
  payloadPurgeCondition,
} from "@/lib/import/worker"

const JOB = `22222222-2222-4222-8222-222222222222`
const TOKEN = `33333333-3333-4333-8333-333333333333`

describe(`claimFence`, () => {
  it(`matches only a previewing/running row that still carries this worker's token`, () => {
    const query = new PgDialect().sqlToQuery(claimFence({ id: JOB, claimToken: TOKEN })!)
    expect(query.sql).toMatch(/"import_jobs"\."id" = \$1/)
    expect(query.sql).toMatch(/"import_jobs"\."status" in \(\$2, \$3\)/)
    expect(query.sql).toMatch(/"import_jobs"\."claim_token" = \$4/)
    // `cancel` writes status=cancelled + claim_token=NULL: this fence then
    // matches zero rows, so neither a finishing discovery (`draft`) nor
    // failJob (`failed`) can land on top of the cancel.
    expect(query.params).toEqual([JOB, `previewing`, `running`, TOKEN])
  })
})

describe(`payloadPurgeCondition`, () => {
  const now = new Date(`2026-09-24T12:00:00Z`)

  it(`purges completed/cancelled payloads after 7 days and failed ones after 30`, () => {
    const query = new PgDialect().sqlToQuery(payloadPurgeCondition(now)!)
    expect(query.sql).toMatch(/"import_jobs"\."payload" is not null/)
    expect(query.sql).toMatch(/"import_jobs"\."status" in \(\$1, \$2\)/)
    expect(query.sql).toMatch(/"import_jobs"\."status" = \$4/)
    expect(query.sql).toMatch(
      /coalesce\("import_jobs"\."finished_at", "import_jobs"\."updated_at"\) < \$3/
    )
    expect(query.params).toEqual([
      `completed`,
      `cancelled`,
      new Date(now.getTime() - IMPORT_PAYLOAD_TTL_MS),
      `failed`,
      new Date(now.getTime() - IMPORT_FAILED_PAYLOAD_TTL_MS),
    ])
  })

  it(`uses a 7 day / 30 day window`, () => {
    expect(IMPORT_PAYLOAD_TTL_MS).toBe(7 * 24 * 60 * 60 * 1000)
    expect(IMPORT_FAILED_PAYLOAD_TTL_MS).toBe(30 * 24 * 60 * 60 * 1000)
  })
})
