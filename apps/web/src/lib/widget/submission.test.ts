import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { loadWidgetSubmissions, widgetSubmissionSummary, widgetSubmissionWire } from "./submission"

// NEW-19 (FEED-116): the ONE submission read behind the "Reported via
// widget" card and the MCP `widget` field.
describe(`widget submission wire`, () => {
  const row = {
    reporterName: `Emma`,
    reporterEmail: `emma@example.com`,
    reporterExternalId: null,
    pageUrl: null,
    userAgent: null,
    viewportWidth: null,
    viewportHeight: 900,
    screenWidth: null,
    screenHeight: null,
    devicePixelRatio: null,
    customData: null,
    lastReporterSeenAt: null,
    createdAt: new Date(`2026-10-01T00:00:00Z`),
  }

  it(`drops half-known displays and nulls missing custom data`, () => {
    const wire = widgetSubmissionWire(row)
    expect(wire.viewport).toBeNull()
    expect(wire.screen).toBeNull()
    expect(wire.customData).toBeNull()
  })

  it(`the summary is reporter + custom data only`, () => {
    expect(widgetSubmissionSummary({ ...row, customData: { a: 1 } })).toEqual({
      reporter: { name: `Emma`, email: `emma@example.com` },
      customData: { a: 1 },
    })
  })

  it(`skips the query for an empty page`, async () => {
    const select = vi.fn()
    await expect(loadWidgetSubmissions([], { select } as never)).resolves.toEqual([])
    expect(select).not.toHaveBeenCalled()
  })

  it(`reads one batch for many issues`, async () => {
    const where = vi.fn(async () => [{ issueId: `a` }])
    const select = vi.fn(() => ({ from: () => ({ where }) }))
    await loadWidgetSubmissions([`a`, `b`, `a`], { select } as never)
    expect(select).toHaveBeenCalledTimes(1)
    expect(where).toHaveBeenCalledTimes(1)
  })
})
