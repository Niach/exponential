import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import type { CodingSessionBlocked } from "@exp/db-schema/domain"
import { reportBlocked } from "./watchdog"

const flag = (kind: string): CodingSessionBlocked => ({
  kind,
  agent: null,
  window: null,
  resetsAt: null,
  since: `2026-11-01T10:00:00.000Z`,
})

describe(`reportBlocked (sessions.report's blocked verdict)`, () => {
  it(`EXP-1536: a failed start is stored, on a person, but never said as a wall`, () => {
    const failed = flag(`start_failed`)
    expect(reportBlocked(null, failed, false)).toEqual({ next: failed, newly: false })
    expect(reportBlocked(null, failed, true)).toEqual({ next: failed, newly: false })
  })

  it(`EXP-1536: the next report without blocked (the relaunch) clears a failed start`, () => {
    expect(reportBlocked(flag(`start_failed`), undefined, false)).toEqual({ next: null, newly: false })
  })

  it(`clears stuck and the interruptions, keeps a usage wall and unclaimed`, () => {
    for (const kind of [`stuck`, `device_offline`, `restarted`]) {
      expect(reportBlocked(flag(kind), undefined, false).next, kind).toBeNull()
    }
    for (const kind of [`rate_limit`, `unclaimed`]) {
      expect(reportBlocked(flag(kind), undefined, false).next, kind).toEqual(flag(kind))
    }
  })

  it(`#FEED-235: a busy report (a turn started) clears an undelivered unclaimed flag; an idle one keeps it`, () => {
    expect(reportBlocked(flag(`unclaimed`), undefined, false, true).next).toBeNull()
    expect(reportBlocked(flag(`unclaimed`), undefined, false, false).next).toEqual(flag(`unclaimed`))
    expect(reportBlocked(flag(`rate_limit`), undefined, false, true).next).toEqual(flag(`rate_limit`))
  })

  it(`says a new usage wall once, unless the device handles it`, () => {
    const wall = flag(`rate_limit`)
    expect(reportBlocked(null, wall, false)).toEqual({ next: wall, newly: true })
    expect(reportBlocked(null, wall, true).newly).toBe(false)
    expect(reportBlocked(flag(`stuck`), wall, false).newly).toBe(false)
    expect(reportBlocked(wall, null, false)).toEqual({ next: null, newly: false })
  })
})
