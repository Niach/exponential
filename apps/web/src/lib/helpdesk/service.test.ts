import { describe, expect, it, vi } from "vitest"
import { PgDialect } from "drizzle-orm/pg-core"

// Keep Postgres out of the test — a chainable fake covers the two query
// helpers (`selectDistinctOn` for the inbox snippets, the token lookup's
// team join); the rest of service.ts is pure.
const h = vi.hoisted(() => {
  const state: {
    distinctOn: unknown
    projection: Record<string, unknown> | undefined
    orderBy: unknown[]
    rows: unknown[]
  } = { distinctOn: undefined, projection: undefined, orderBy: [], rows: [] }

  function chain() {
    const p = Promise.resolve(state.rows) as Promise<unknown[]> &
      Record<string, (...args: unknown[]) => unknown>
    p.from = () => p
    p.leftJoin = () => p
    p.where = () => p
    p.limit = () => p
    p.orderBy = (...args: unknown[]) => {
      state.orderBy = args
      return p
    }
    return p
  }

  const db = {
    select: (projection?: Record<string, unknown>) => {
      state.projection = projection
      return chain()
    },
    selectDistinctOn: (on: unknown, projection: Record<string, unknown>) => {
      state.distinctOn = on
      state.projection = projection
      return chain()
    },
  }
  return { state, db }
})

vi.mock(`@/db/connection`, () => ({ db: h.db }))

import {
  SUPPORT_SNIPPET_CHARS,
  isSupportThreadFrozen,
  latestMessagesByThread,
  supportTicketTitle,
  type ResolvedSupportThread,
} from "@/lib/helpdesk/service"
import { supportMessages } from "@/db/schema"

describe(`supportTicketTitle`, () => {
  it(`uses the first line`, () => {
    expect(supportTicketTitle(`Broken login\nmore detail`)).toBe(`Broken login`)
  })

  it(`clamps long first lines with an ellipsis`, () => {
    const title = supportTicketTitle(`x`.repeat(300))
    expect(title.length).toBeLessThanOrEqual(120)
    expect(title.endsWith(`…`)).toBe(true)
  })

  it(`falls back when the message starts blank`, () => {
    expect(supportTicketTitle(`\n\nactual text`)).toBe(`Support request`)
  })
})

// REV2-40: this used to pull EVERY public message body of every listed thread
// (up to 10k chars each) and pick first-per-thread in JS — on a 30s poll.
describe(`latestMessagesByThread`, () => {
  it(`asks Postgres for one truncated row per thread`, async () => {
    h.state.rows = [
      {
        threadId: `t-1`,
        body: `newest`,
        direction: `inbound`,
        createdAt: new Date(),
      },
    ]
    const latest = await latestMessagesByThread([`t-1`])

    expect(h.state.distinctOn).toEqual([supportMessages.threadId])
    // DISTINCT ON demands its expression leads the ORDER BY.
    expect(h.state.orderBy[0]).toBe(supportMessages.threadId)
    const { sql, params } = new PgDialect().sqlToQuery(
      h.state.projection!.body as never
    )
    expect(sql).toContain(`left(`)
    expect(params).toContain(SUPPORT_SNIPPET_CHARS)
    expect(latest.get(`t-1`)?.body).toBe(`newest`)
  })

  it(`short-circuits on an empty id list`, async () => {
    h.state.distinctOn = undefined
    expect((await latestMessagesByThread([])).size).toBe(0)
    expect(h.state.distinctOn).toBeUndefined()
  })
})

// REV2-23: turning a team's helpdesk off freezes its threads — reporter
// replies are refused like a closed thread, reads still work, re-enabling
// thaws. Nothing is auto-closed.
describe(`isSupportThreadFrozen`, () => {
  const resolved = (over: Partial<ResolvedSupportThread> = {}) =>
    ({
      thread: { tokenRevokedAt: null },
      teamName: `Acme`,
      helpdeskEnabled: true,
      ...over,
    }) as ResolvedSupportThread

  it(`is open while the helpdesk is on and the token lives`, () => {
    expect(isSupportThreadFrozen(resolved())).toBe(false)
  })

  it(`freezes when the team's helpdesk is off`, () => {
    expect(isSupportThreadFrozen(resolved({ helpdeskEnabled: false }))).toBe(
      true
    )
  })

  it(`still reports a member-closed thread as frozen`, () => {
    expect(
      isSupportThreadFrozen(
        resolved({
          thread: { tokenRevokedAt: new Date() } as never,
        })
      )
    ).toBe(true)
  })
})
