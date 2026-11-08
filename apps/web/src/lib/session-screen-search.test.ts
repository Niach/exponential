// #EXP-1732 (session-screen.json): THE session screen's search; legacy
// `?tab=` values (the retired strip's) land on their successor.
import { describe, expect, it } from "vitest"
import { validateChatSearch } from "@/lib/session-screen-search"

describe(`validateChatSearch`, () => {
  it(`keeps the new tab ids and the opened item with its id`, () => {
    expect(validateChatSearch({ tab: `sessions`, open: `session`, session: `s2` })).toEqual({
      tab: `sessions`,
      open: `session`,
      session: `s2`,
    })
    expect(validateChatSearch({ tab: `issues`, open: `issue`, issue: `i1`, session: `s9` })).toEqual({
      tab: `issues`,
      open: `issue`,
      issue: `i1`,
    })
    expect(validateChatSearch({ open: `settings` })).toEqual({ open: `settings` })
  })

  it(`maps the legacy strip values`, () => {
    expect(validateChatSearch({ tab: `issue`, issue: `i1` })).toEqual({ tab: `issues`, open: `issue`, issue: `i1` })
    expect(validateChatSearch({ tab: `session`, session: `s2`, from: `inbox` })).toEqual({
      tab: `sessions`,
      open: `session`,
      session: `s2`,
      from: `inbox`,
    })
    expect(validateChatSearch({ tab: `plan`, plan: `m1` })).toEqual({ open: `plan`, plan: `m1` })
    expect(validateChatSearch({ tab: `guide` })).toEqual({ tab: `results` })
    expect(validateChatSearch({ tab: `stories` })).toEqual({ tab: `results` })
    expect(validateChatSearch({ tab: `conversation` })).toEqual({ tab: `chat` })
    expect(validateChatSearch({ tab: `settings` })).toEqual({ tab: `settings` })
  })

  it(`drops what it does not know; the review pane only on Results`, () => {
    expect(validateChatSearch({ tab: `nope`, open: `nope` })).toEqual({})
    expect(validateChatSearch({ tab: `results`, section: `all`, file: `a.ts` })).toEqual({
      tab: `results`,
      section: `all`,
      file: `a.ts`,
    })
    expect(validateChatSearch({ tab: `chat`, section: `all`, file: `a.ts` })).toEqual({ tab: `chat` })
  })
})
