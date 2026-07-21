import { beforeEach, describe, expect, it, vi } from "vitest"

// deliverableRecipients() is the membership guard at deliver()'s chokepoint
// (REV-8): stale issue_subscribers rows / assignee ids left behind by a
// removed member must never fan out to inbox/push/email. The db is mocked
// billing.test.ts-style — `db.select()` shifts the next pre-seeded result
// array off a FIFO queue, so the joined membership query can be scripted
// without Postgres.
const { selectResults, selectCalls } = vi.hoisted(() => ({
  selectResults: [] as unknown[][],
  selectCalls: { count: 0 },
}))

function chain(): Promise<unknown[]> & Record<string, () => unknown> {
  const p = Promise.resolve(
    selectResults.shift() ?? []
  ) as Promise<unknown[]> & Record<string, () => unknown>
  for (const m of [`from`, `where`, `innerJoin`, `leftJoin`, `limit`]) {
    p[m] = () => p
  }
  return p
}

vi.mock(`@/db/connection`, () => ({
  db: {
    select: () => {
      selectCalls.count += 1
      return chain()
    },
  },
}))

// Importing the module under test must touch neither FCM nor the email
// transport.
vi.mock(`@/lib/integrations/fcm`, () => ({
  sendToUsers: vi.fn(async () => {}),
}))
vi.mock(`@/lib/email`, () => ({
  emailEnabled: false,
  sendReporterResolutionEmail: vi.fn(),
}))

import { deliverableRecipients } from "./notifications"

beforeEach(() => {
  selectResults.length = 0
  selectCalls.count = 0
})

describe(`deliverableRecipients — membership guard at the deliver() chokepoint`, () => {
  it(`drops recipients who are not current team members, preserving input order`, async () => {
    // The team_members ⋈ users query returns only the current,
    // non-agent members among the candidates — `removed-b` (an ex-member
    // with a stale subscriber row) is absent.
    selectResults.push([{ id: `member-a` }, { id: `member-c` }])

    const result = await deliverableRecipients(`ws-1`, [
      `member-c`,
      `removed-b`,
      `member-a`,
    ])

    expect(result).toEqual([`member-c`, `member-a`])
    expect(selectCalls.count).toBe(1)
  })

  it(`short-circuits an empty recipient list without querying the db`, async () => {
    const result = await deliverableRecipients(`ws-1`, [])

    expect(result).toEqual([])
    expect(selectCalls.count).toBe(0)
  })
})
