import { beforeEach, describe, expect, it, vi } from "vitest"

// EXP-1342: Stop hands the unread messages' text back to the composer at
// once; a REFUSED `sessions.stop` re-takes it — the device still holds the
// queue (its next `queue` frame repaints the strip), so text left in a
// field would be sent twice. The store's draft is the hand-over; a composer
// that drains it into its own field (`takeDraftText`) registers how to give
// the text back (`onStopRetraction`), and only while nothing was typed over
// it.

const mocks = vi.hoisted(() => ({
  stop: vi.fn(),
  draft: { text: `` },
  queued: [] as string[],
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { sessions: { stop: { mutate: mocks.stop } } },
}))
vi.mock(`@/lib/steer-session-store`, async (importOriginal) => {
  // eslint-disable-next-line quotes -- `typeof import()` requires a string literal; the backtick autofix breaks it
  const actual = await importOriginal<typeof import("@/lib/steer-session-store")>()
  return {
    ...actual,
    acquireSteerSession: () => ({
      getDraftSnapshot: () => mocks.draft,
      noteStopped: () => {
        if (mocks.queued.length === 0) return false
        mocks.draft = {
          text: actual.mergeHeldIntoDraft(mocks.draft.text, mocks.queued),
        }
        mocks.queued = []
        return true
      },
      takeDraftText: () => {
        const text = mocks.draft.text
        mocks.draft = { text: `` }
        return text
      },
      setDraftText: (text: string) => {
        mocks.draft = { text }
      },
    }),
  }
})

import { onStopRetraction, stopSessionTurn } from "./session-stop"

/** The composer's side: drains the store's draft into `field` the way the
 *  `SessionComposer` effect does. */
const drain = () => {
  mocks.draft = { text: `` }
}

beforeEach(() => {
  mocks.stop.mockReset()
  mocks.draft = { text: `` }
  mocks.queued = []
})

describe(`stopSessionTurn (EXP-1342)`, () => {
  it(`a Stop that goes through leaves the handed-back text in the draft`, async () => {
    mocks.stop.mockResolvedValue({ txid: 1 })
    mocks.queued = [`one`, `two`]
    await stopSessionTurn(`s1`)
    expect(mocks.stop).toHaveBeenCalledWith({ sessionId: `s1` }, undefined)
    expect(mocks.draft.text).toBe(`one\ntwo`)
  })

  it(`a refused Stop with the text still in the store puts the draft back as it was`, async () => {
    mocks.stop.mockRejectedValue(new Error(`nope`))
    mocks.draft = { text: `typing` }
    mocks.queued = [`one`, `two`]
    await expect(stopSessionTurn(`s1`)).rejects.toThrow(`nope`)
    expect(mocks.draft.text).toBe(`typing`)
  })

  it(`a refused Stop after a composer drained the text asks that composer for it back`, async () => {
    const retract = vi.fn((_held: string) => true)
    const off = onStopRetraction(`s1`, retract)
    mocks.queued = [`one`, `two`]
    mocks.stop.mockImplementation(async () => {
      // The composer's effect drains the store before the call settles.
      expect(mocks.draft.text).toBe(`one\ntwo`)
      drain()
      throw new Error(`nope`)
    })
    await expect(stopSessionTurn(`s1`)).rejects.toThrow(`nope`)
    expect(retract).toHaveBeenCalledWith(`one\ntwo`)
    // Nothing was in the draft before the Stop: nothing comes back to it.
    expect(mocks.draft.text).toBe(``)
    off()
  })

  it(`the draft in progress before the Stop returns through the store once the composer gave the text back`, async () => {
    const off = onStopRetraction(`s1`, () => true)
    mocks.draft = { text: `typing` }
    mocks.queued = [`one`]
    mocks.stop.mockImplementation(async () => {
      expect(mocks.draft.text).toBe(`typing\n\none`)
      drain()
      throw new Error(`nope`)
    })
    await expect(stopSessionTurn(`s1`)).rejects.toThrow(`nope`)
    expect(mocks.draft.text).toBe(`typing`)
    off()
  })

  it(`text typed over the handed-back text stays: nothing is retracted or re-added`, async () => {
    const retract = vi.fn(() => false)
    const off = onStopRetraction(`s1`, retract)
    mocks.draft = { text: `typing` }
    mocks.queued = [`one`]
    mocks.stop.mockImplementation(async () => {
      drain()
      throw new Error(`nope`)
    })
    await expect(stopSessionTurn(`s1`)).rejects.toThrow(`nope`)
    expect(retract).toHaveBeenCalledWith(`typing\n\none`)
    expect(mocks.draft.text).toBe(``)
    off()
  })

  it(`an unsubscribed composer is never asked; another session's never is`, async () => {
    const mine = vi.fn(() => true)
    const other = vi.fn(() => true)
    onStopRetraction(`s1`, mine)()
    const off = onStopRetraction(`s2`, other)
    mocks.queued = [`one`]
    mocks.stop.mockImplementation(async () => {
      drain()
      throw new Error(`nope`)
    })
    await expect(stopSessionTurn(`s1`)).rejects.toThrow(`nope`)
    expect(mine).not.toHaveBeenCalled()
    expect(other).not.toHaveBeenCalled()
    off()
  })

  it(`a Stop with nothing queued retracts nothing when refused`, async () => {
    const retract = vi.fn(() => true)
    const off = onStopRetraction(`s1`, retract)
    mocks.draft = { text: `typing` }
    mocks.stop.mockRejectedValue(new Error(`nope`))
    await expect(stopSessionTurn(`s1`)).rejects.toThrow(`nope`)
    expect(retract).not.toHaveBeenCalled()
    expect(mocks.draft.text).toBe(`typing`)
    off()
  })
})
