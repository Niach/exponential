import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { activityPayload, shouldSend } from "@/lib/integrations/live-activities"

// #EXP-1639 (live-activity.json): the activity's sealed plaintext is the push
// shape every phone already opens, and only a change goes out.
describe(`activityPayload`, () => {
  it(`carries the content as title, body and routing data`, () => {
    const payload = activityPayload(
      { id: `s1`, startedAt: `2026-11-06T08:00:00.000Z` },
      { phase: `needsYou`, title: `Fix the login`, subtitle: `Needs you`, detail: `Question: Ship?`, badge: `needs-you`, segment: 2 },
      `u1`
    )
    expect(payload).toEqual({
      title: `Fix the login`,
      body: `Needs you`,
      data: {
        type: `session_activity`,
        sessionId: `s1`,
        phase: `needsYou`,
        badge: `needs-you`,
        segment: `2`,
        detail: `Question: Ship?`,
        startedAt: `2026-11-06T08:00:00.000Z`,
        userId: `u1`,
      },
    })
  })
})

describe(`shouldSend`, () => {
  const at = (phase: string, detail = ``) => JSON.stringify({ title: `t`, body: phase, data: { phase, detail } })
  const sent = new Date(`2026-11-06T08:00:00.000Z`)
  const soon = new Date(`2026-11-06T08:00:05.000Z`)
  const later = new Date(`2026-11-06T08:01:00.000Z`)

  it(`never repeats the same content`, () => expect(shouldSend(at(`working`), at(`working`), sent, later)).toBe(false))
  it(`a needsYou or done change goes out at once`, () => {
    expect(shouldSend(at(`working`), at(`needsYou`), sent, soon)).toBe(true)
    expect(shouldSend(at(`needsYou`), at(`working`), sent, soon)).toBe(true)
    expect(shouldSend(at(`working`), at(`done`), sent, soon)).toBe(true)
  })
  it(`any other change waits for minUpdateSeconds`, () => {
    expect(shouldSend(at(`working`, `a`), at(`working`, `b`), sent, soon)).toBe(false)
    expect(shouldSend(at(`working`, `a`), at(`working`, `b`), sent, later)).toBe(true)
  })
  it(`a phone-registered follow (empty content) always gets the current one`, () =>
    expect(shouldSend(``, at(`working`), sent, soon)).toBe(true))
})

