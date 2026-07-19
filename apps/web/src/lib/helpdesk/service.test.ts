import { describe, expect, it, vi } from "vitest"

// Keep Postgres out of the test — service.ts only needs db for the query
// helpers, which these tests don't touch.
vi.mock(`@/db/connection`, () => ({ db: {} }))

import { supportTicketTitle } from "@/lib/helpdesk/service"

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
