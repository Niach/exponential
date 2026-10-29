import { describe, expect, it } from "vitest"
import { swipeTarget } from "@/hooks/use-face-swipe"
import { threadPageOf, threadPages, threadPageSearch } from "@/lib/thread-pages"

// EXP-1343: the phone pager walks EVERY strip tab in strip order, nested
// chips flattened after theirs, the `+` never a page.
describe(`threadPages`, () => {
  const tabs = [
    { kind: `add` as const },
    { kind: `conversation` as const },
    { kind: `issue` as const, issueId: `i1`, nested: [{ issueId: `i1a` }] },
    { kind: `issue` as const, issueId: `i2`, nested: [] },
    { kind: `session` as const, sessionId: `s1`, nested: [{ sessionId: `s1a` }, { sessionId: `s1b` }] },
  ]

  it(`orders conversation, issues, sessions, stories last`, () => {
    expect(threadPages(tabs)).toEqual([
      `conversation`,
      `issue:i1`,
      `issue:i1a`,
      `issue:i2`,
      `session:s1`,
      `session:s1a`,
      `session:s1b`,
      `stories`,
    ])
  })

  it(`swipes walk the pages and stop at the ends`, () => {
    const pages = threadPages(tabs)
    expect(swipeTarget(pages, `conversation`, `left`)).toBe(`issue:i1`)
    expect(swipeTarget(pages, `issue:i2`, `left`)).toBe(`session:s1`)
    expect(swipeTarget(pages, `session:s1b`, `left`)).toBe(`stories`)
    expect(swipeTarget(pages, `stories`, `left`)).toBeNull()
    expect(swipeTarget(pages, `conversation`, `right`)).toBeNull()
  })

  it(`round-trips a page through the route search`, () => {
    for (const page of threadPages(tabs)) {
      const search = threadPageSearch(page)
      const tab =
        search.tab === `issue`
          ? { kind: `issue` as const, issueId: search.issue! }
          : search.tab === `session`
            ? { kind: `session` as const, sessionId: search.session! }
            : search.tab === `stories`
              ? { kind: `stories` as const }
              : { kind: `conversation` as const }
      expect(threadPageOf(tab)).toBe(page)
    }
  })
})
