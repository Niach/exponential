import { beforeEach, describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"
import type { SQL } from "drizzle-orm"

// NEW-19 (FEED-144): the reporter sees the reporter conversation ONLY —
// audience `reporter` comments — named per reporter-reply.json authorNames.

const h = vi.hoisted(() => ({
  where: null as unknown,
  rows: [] as unknown[],
}))

vi.mock(`@/db/connection`, () => {
  const b: Record<string, unknown> = {}
  for (const m of [`from`, `leftJoin`, `orderBy`, `limit`]) b[m] = () => b
  b.where = (condition: unknown) => {
    h.where = condition
    return b
  }
  b.offset = async () => h.rows
  return { db: { select: vi.fn(() => b) } }
})

import { listReporterComments, reporterIssueView } from "./reporter-view"

const resolved = {
  issue: {
    id: `11111111-1111-4111-8111-111111111111`,
    identifier: `FEED-144`,
    title: `Issue not found for my own report`,
    description: `It fails.\n\n![Screenshot](/api/attachments/abc)\n`,
    createdAt: new Date(`2026-10-26T10:00:00Z`),
    updatedAt: new Date(`2026-10-26T11:00:00Z`),
    assigneeId: `vendor-member`,
  },
  submission: { reporterName: `Emma Fischer`, reporterEmail: `Emma@X.test` },
  teamName: `Exponential`,
  statusCategory: `started`,
} as never

beforeEach(() => {
  h.where = null
  h.rows = []
})

describe(`listReporterComments`, () => {
  it(`reads audience reporter comments of the issue only`, async () => {
    await listReporterComments(resolved, { limit: 10 })
    const { sql, params } = new PgDialect().sqlToQuery(h.where as SQL)
    expect(sql).toContain(`"comments"."issue_id" = $1`)
    expect(sql).toContain(`"comments"."audience" = $2`)
    expect(params).toEqual([`11111111-1111-4111-8111-111111111111`, `reporter`])
  })

  it(`names the reporter, and every team reply "Support"`, async () => {
    const at = new Date(`2026-10-26T12:00:00Z`)
    h.rows = [
      { id: `a`, source: `reporter`, body: `Still broken`, createdAt: at, userName: null, agentName: null },
      { id: `b`, source: `user`, body: `Fixed`, createdAt: at, userName: `Dennis`, agentName: null },
      { id: `c`, source: `mcp`, body: `Looking`, createdAt: at, userName: `Dennis`, agentName: `Triage` },
      { id: `d`, source: `user`, body: `Old`, createdAt: at, userName: null, agentName: null },
    ]
    const out = await listReporterComments(resolved, { limit: 10 })
    expect(out.map((c) => [c.author, c.fromReporter])).toEqual([
      [`Emma Fischer`, true],
      [`Support`, false],
      [`Support`, false],
      [`Support`, false],
    ])
  })

  // NEW-85 (reporter-reply.json reporterView): a member's @mention never
  // names them; the reporter's own address and their own text pass.
  it(`reads member mentions as "@Support" in team replies only`, async () => {
    const at = new Date(`2026-10-26T12:00:00Z`)
    h.rows = [
      { id: `a`, source: `user`, body: `@ada@vendor.test fixed it, thanks @emma@x.test`, createdAt: at },
      { id: `b`, source: `reporter`, body: `ping \\@ada\\@vendor\\.test`, createdAt: at },
    ]
    const out = await listReporterComments(resolved, { limit: 10 })
    expect(out.map((c) => c.body)).toEqual([
      `@Support fixed it, thanks @emma@x.test`,
      `ping \\@ada\\@vendor\\.test`,
    ])
  })

  it(`falls back to the anonymous label without a reporter name`, async () => {
    h.rows = [
      { id: `a`, source: `reporter`, body: `x`, createdAt: new Date(), userName: null, agentName: null },
    ]
    const out = await listReporterComments(
      { ...(resolved as object), submission: { reporterName: null } } as never,
      { limit: 10 }
    )
    expect(out[0].author).toBe(`Anonymous visitor`)
  })
})

describe(`reporterIssueView`, () => {
  it(`is the report plus its reporter conversation, nothing team-internal`, async () => {
    const view = await reporterIssueView(resolved, 50)
    expect(view).toEqual({
      id: `11111111-1111-4111-8111-111111111111`,
      identifier: `FEED-144`,
      title: `Issue not found for my own report`,
      description: `It fails.`,
      statusCategory: `started`,
      teamName: `Exponential`,
      createdAt: new Date(`2026-10-26T10:00:00Z`),
      updatedAt: new Date(`2026-10-26T11:00:00Z`),
      access: `reporter`,
      recentComments: [],
    })
    expect(view).not.toHaveProperty(`assigneeId`)
  })

  it(`reads member mentions in the title and description as "@Support"`, async () => {
    const view = await reporterIssueView(
      {
        ...(resolved as object),
        issue: {
          ...(resolved as { issue: object }).issue,
          title: `Ask @ada@vendor.test`,
          description: `@bob@vendor.test owns this`,
        },
      } as never,
      50
    )
    expect(view.title).toBe(`Ask @Support`)
    expect(view.description).toBe(`@Support owns this`)
  })
})
