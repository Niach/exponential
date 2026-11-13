import { describe, expect, test } from "vitest"
import fixture from "@exp/domain-contract/fixtures/waiting-cards.json" with { type: "json" }
import { deepLinkStep, type DeepLinkStep } from "@exp/domain-contract/waiting-cards"
import {
  centreOffset,
  DEEP_LINK_PAGE_GRACE_MS,
  deepLinkTick,
  takeDeepLink,
  type DeepLinkCursor,
  type DeepLinkProbe,
} from "@/lib/message-deep-link"

// waiting-cards.json DEEP LINKS: the web driver over a simulated transcript
// (a chat of 500 messages that loads its newest 200, pages 100 older ones
// per page, lands a page a few frames later and lays a new row out a frame
// after it mounts).

describe(`deepLinkCases (fixture replay)`, () => {
  for (const testCase of fixture.deepLinkCases) {
    test(testCase.name, () => {
      expect(deepLinkStep(testCase.input)).toBe(testCase.expected as DeepLinkStep)
      // The driver takes the same step from the same probe.
      const { elapsedMs, ...probe } = testCase.input
      const ran: string[] = []
      const { step } = deepLinkTick(takeDeepLink(0), { ...probe, mark: 0 }, elapsedMs, {
        page: () => ran.push(`page`),
        scroll: () => ran.push(`scroll`),
        ring: () => ran.push(`ring`),
        missing: () => ran.push(`missing`),
      })
      expect(step).toBe(testCase.expected as DeepLinkStep)
      expect(ran).toEqual(step === `wait` ? [] : [step])
    })
  }
})

const ROW = 80
const VIEWPORT = 600

/** A chat of `total` rows, the newest `loaded` mounted, bottom-anchored. */
class Transcript {
  loaded = 200
  /** Frames until the in-flight page lands. */
  landing: number | null = null
  /** Rows mounted this frame are measured the next. */
  measuredFrom = Number.POSITIVE_INFINITY
  scrollTop = 0
  pages = 0
  log: DeepLinkStep[] = []
  constructor(readonly total: number) {
    this.scrollTop = this.maxTop()
  }
  first(): number {
    return this.total - this.loaded
  }
  maxTop(): number {
    return Math.max(0, this.loaded * ROW - VIEWPORT)
  }
  /** One frame passes: a page in flight lands, mounted rows lay out. */
  frame(): void {
    if (this.measuredFrom !== Number.POSITIVE_INFINITY) this.measuredFrom = Number.POSITIVE_INFINITY
    if (this.landing !== null && --this.landing <= 0) {
      this.landing = null
      const before = this.loaded
      this.loaded = Math.min(this.total, this.loaded + 100)
      // Older rows paging in keep the reader's place.
      this.scrollTop += (this.loaded - before) * ROW
      // The new rows are still estimates this frame.
      this.measuredFrom = this.first()
    }
  }
  probe(target: number): DeepLinkProbe {
    const found = target >= this.first() && target < this.total
    const laidOut = found && this.measuredFrom === Number.POSITIVE_INFINITY
    const top = (target - this.first()) * ROW - this.scrollTop
    const { centred } = centreOffset(
      { top, height: ROW },
      { top: 0, height: VIEWPORT, scrollTop: this.scrollTop, scrollHeight: this.loaded * ROW }
    )
    return { found, laidOut, centred, paging: this.landing !== null, more: this.loaded < this.total, mark: this.loaded }
  }
  /** Drive a link to row `target` at 16 ms a frame until it settles. */
  drive(target: number, maxFrames = 2_000): { step: DeepLinkStep; ringed: boolean; frames: number } {
    let cursor: DeepLinkCursor = takeDeepLink(0)
    let ringed = false
    for (let frame = 0; frame < maxFrames; frame++) {
      this.frame()
      const result = deepLinkTick(cursor, this.probe(target), frame * 16, {
        page: () => {
          this.pages++
          this.landing = 3
        },
        scroll: () => {
          const top = (target - this.first()) * ROW - this.scrollTop
          const delta = top + ROW / 2 - VIEWPORT / 2
          this.scrollTop = Math.min(this.maxTop(), Math.max(0, this.scrollTop + delta))
        },
        ring: () => {
          ringed = true
        },
        missing: () => {},
      })
      cursor = result.cursor
      this.log.push(result.step)
      if (result.settled) return { step: result.step, ringed, frames: frame }
    }
    throw new Error(`never settled`)
  }
}

describe(`a pending deep link`, () => {
  test(`a card OUTSIDE the newest 200 rows: pages one page at a time until found, waits for layout, scrolls, rings`, () => {
    const chat = new Transcript(500)
    const target = 120 // the 380th newest
    const result = chat.drive(target)
    expect(result.step).toBe(`ring`)
    expect(result.ringed).toBe(true)
    // 200 → 300 → 400 loaded: two pages, one at a time.
    expect(chat.pages).toBe(2)
    expect(chat.loaded).toBe(400)
    const firstFound = chat.log.indexOf(`scroll`)
    // It paged, waited for each page, then waited a frame for the layout.
    expect(chat.log.slice(0, firstFound)).toContain(`page`)
    expect(chat.log.slice(0, firstFound)).toContain(`wait`)
    expect(chat.log[firstFound - 1]).toBe(`wait`)
    expect(chat.log.at(-1)).toBe(`ring`)
    // Rung centred in the viewport.
    const top = (target - chat.first()) * ROW - chat.scrollTop
    expect(Math.abs(top + ROW / 2 - VIEWPORT / 2)).toBeLessThanOrEqual(2)
  })

  test(`a card in the newest rows: no page, scroll then ring`, () => {
    const chat = new Transcript(500)
    const result = chat.drive(420)
    expect(result.step).toBe(`ring`)
    expect(chat.pages).toBe(0)
    expect(chat.log.filter((step) => step === `scroll`).length).toBe(1)
  })

  test(`the newest row: the list cannot scroll further, so it rings where it stands`, () => {
    const chat = new Transcript(500)
    expect(chat.drive(499).step).toBe(`ring`)
  })

  test(`a missing card: pages to the oldest row, then missing (never rings)`, () => {
    const chat = new Transcript(450)
    const result = chat.drive(-1)
    expect(result.step).toBe(`missing`)
    expect(result.ringed).toBe(false)
    expect(chat.loaded).toBe(450)
    expect(chat.pages).toBe(3)
  })

  test(`a page that never lands: the next page waits out the grace, the link gives up at deepLinkMs`, () => {
    let cursor = takeDeepLink(0)
    const probe: DeepLinkProbe = { found: false, laidOut: false, centred: false, paging: false, more: true, mark: 1 }
    let pages = 0
    const effects = { page: () => pages++, scroll: () => {}, ring: () => {}, missing: () => {} }
    let last: DeepLinkStep = `wait`
    for (let now = 0; now <= fixture.constants.deepLinkMs; now += 16) {
      const result = deepLinkTick(cursor, probe, now, effects)
      cursor = result.cursor
      last = result.step
      if (result.settled) break
    }
    expect(last).toBe(`missing`)
    expect(pages).toBe(Math.ceil(fixture.constants.deepLinkMs / DEEP_LINK_PAGE_GRACE_MS))
  })
})

describe(`centreOffset`, () => {
  const viewport = { top: 100, height: 600, scrollTop: 400, scrollHeight: 4_000 }
  test(`within 2 px of the centre is centred`, () => {
    expect(centreOffset({ top: 361, height: 80 }, viewport)).toEqual({ delta: 1, centred: true })
    expect(centreOffset({ top: 300, height: 80 }, viewport).centred).toBe(false)
  })
  test(`the list cannot scroll further toward it`, () => {
    expect(centreOffset({ top: 100, height: 80 }, { ...viewport, scrollTop: 0 }).centred).toBe(true)
    expect(centreOffset({ top: 600, height: 80 }, { ...viewport, scrollTop: 3_400 }).centred).toBe(true)
    expect(centreOffset({ top: 600, height: 80 }, viewport).centred).toBe(false)
  })
})
