import { describe, expect, it } from "vitest"
import listItem from "@exp/domain-contract/fixtures/list-item.json"
import display from "@exp/domain-contract/fixtures/session-display.json"
import { listElapsed, sessionRowCaption } from "@/lib/session-row-caption"
import type { SessionDisplayState } from "@/lib/coding-session-display"

// EXP-1248: the big session row's caption, replayed off list-item.json (x4).

describe(`listElapsed`, () => {
  it.each(listItem.elapsed)(`$name`, ({ ms, expected }) => {
    expect(listElapsed(ms)).toBe(expected)
  })
})

describe(`sessionRowCaption`, () => {
  it.each(listItem.captions)(`$name`, (c) => {
    expect(
      sessionRowCaption({
        ended: c.ended,
        paused: c.paused,
        state: c.state as SessionDisplayState,
        device: c.device,
        startedAt: c.startedAt,
        updatedAt: c.updatedAt,
        endedAt: c.endedAt,
        blockedLabel: c.blockedLabel,
        now: new Date(c.now),
      })
    ).toEqual(c.expected)
  })

  it(`paints every live state in session-display.json's statusTone`, () => {
    for (const c of display.cases) {
      if (c.status === `ended`) continue
      const { tone } = sessionRowCaption({
        ended: false,
        paused: false,
        state: c.state as SessionDisplayState,
        device: `mint`,
        startedAt: null,
        updatedAt: null,
        endedAt: null,
        now: 0,
      })
      expect(tone, c.name).toBe(c.statusTone)
    }
  })
})
