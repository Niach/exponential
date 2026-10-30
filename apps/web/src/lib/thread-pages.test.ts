import { describe, expect, it } from "vitest"
import { swipeTarget } from "@/hooks/use-face-swipe"
import { threadPageOf, threadPages, threadPageSearch, type ThreadPage } from "@/lib/thread-pages"

// #EXP-1433: the phone pager = Conversation · (the one issue | the item open
// from the group, only while open) · Stories; `+` and the group never pages.
describe(`threadPages`, () => {
  it(`chips: the one issue sits between Conversation and Stories`, () => {
    expect(threadPages({ single: `i1`, open: null })).toEqual([`conversation`, `issue:i1`, `stories`])
    expect(threadPages({ single: `i1`, open: `issue:i1` })).toEqual([`conversation`, `issue:i1`, `stories`])
  })

  it(`group: only the item open from the menu is a page, while it is open`, () => {
    expect(threadPages({ single: null, open: null })).toEqual([`conversation`, `stories`])
    expect(threadPages({ single: null, open: `conversation` })).toEqual([`conversation`, `stories`])
    expect(threadPages({ single: null, open: `session:s1` })).toEqual([`conversation`, `session:s1`, `stories`])
    expect(threadPages({ single: null, open: `issue:i2` })).toEqual([`conversation`, `issue:i2`, `stories`])
  })

  it(`swipes walk the pages and stop at the ends`, () => {
    const pages = threadPages({ single: null, open: `session:s1` })
    expect(swipeTarget(pages, `conversation`, `left`)).toBe(`session:s1`)
    expect(swipeTarget(pages, `session:s1`, `left`)).toBe(`stories`)
    expect(swipeTarget(pages, `stories`, `left`)).toBeNull()
    expect(swipeTarget(pages, `conversation`, `right`)).toBeNull()
  })

  it(`round-trips a page through the route search`, () => {
    const pages: ThreadPage[] = [`conversation`, `issue:i1`, `session:s1`, `stories`]
    for (const page of pages) {
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
