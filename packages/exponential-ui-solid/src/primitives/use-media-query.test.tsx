// FEED-131: the Solid hooks createMediaQuery / createIsMobile.
import { describe, expect, it } from "vitest"
import { createRoot } from "solid-js"
import { createMediaQuery } from "./use-media-query"
import { createIsMobile } from "./use-mobile"

function stubMatchMedia(matches: boolean) {
  const listeners = new Set<() => void>()
  const mql = { matches, addEventListener: (_: string, l: () => void) => listeners.add(l), removeEventListener: (_: string, l: () => void) => listeners.delete(l) }
  const prev = window.matchMedia
  window.matchMedia = (() => mql) as unknown as typeof window.matchMedia
  return { mql, fire: () => listeners.forEach((l) => l()), listeners, restore: () => (window.matchMedia = prev) }
}

describe(`createMediaQuery`, () => {
  it(`follows the query and stops listening on dispose`, () => {
    const stub = stubMatchMedia(false)
    createRoot((dispose) => {
      const matches = createMediaQuery(`(min-width: 10px)`)
      expect(matches()).toBe(false)
      stub.mql.matches = true
      stub.fire()
      expect(matches()).toBe(true)
      dispose()
    })
    expect(stub.listeners.size).toBe(0)
    stub.restore()
  })
})

describe(`createIsMobile`, () => {
  it(`reads the viewport width synchronously`, () => {
    createRoot((dispose) => {
      const mobile = createIsMobile()
      expect(mobile()).toBe(window.innerWidth < 768)
      dispose()
    })
  })
})
