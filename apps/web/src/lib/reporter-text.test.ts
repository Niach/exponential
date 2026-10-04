import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/reporter-text.json"
import { extractIssueRefs } from "@/lib/issue-refs"
import { extractMentionEmails } from "@/lib/mention-refs"
import {
  escapeReporterText,
  titleFromReporterMessage,
  unescapeReporterText,
} from "./reporter-text"

describe(`escapeReporterText (SLOP-4, fixture-locked)`, () => {
  for (const c of fixture.cases) {
    it(c.name, () => {
      expect(escapeReporterText(c.input)).toBe(c.output)
    })
  }

  it(`is idempotent on already-escaped text only through the backslash rule`, () => {
    // Escaping twice doubles the backslashes — callers escape exactly once,
    // at the server boundary.
    const once = escapeReporterText(`a *b*`)
    expect(escapeReporterText(once)).not.toBe(once)
  })

  it(`never leaves a bare mention or issue-ref token`, () => {
    const out = escapeReporterText(`@ada@x.test #PRD-1 #prd-22`)
    expect(out).not.toMatch(/(^|[^\\])@/)
    expect(out).not.toMatch(/(^|[^\\])#/)
    // The resolvers would still see them (a backslash is not a word char)
    // — which is why the server never runs them on reporter text.
    expect(extractIssueRefs(`#PRD-1`)).toEqual([`PRD-1`])
    expect(extractMentionEmails(`@ada@x.test`)).toEqual([`ada@x.test`])
  })
})

describe(`titleFromReporterMessage`, () => {
  for (const c of fixture.title.cases) {
    it(JSON.stringify(c.input).slice(0, 40), () => {
      expect(titleFromReporterMessage(c.input)).toBe(c.output)
    })
  }
})

describe(`unescapeReporterText`, () => {
  it(`undoes the escape for unindented lines`, () => {
    const inputs = [
      `Checkout on example.com fails`,
      `a *b* _c_ [d](e) <f> #EXP-1 @ada@x.test ~g~ | h & i \\ j`,
      `- item`,
      `1. first`,
      `= heading`,
    ]
    for (const input of inputs) {
      expect(unescapeReporterText(escapeReporterText(input))).toBe(input)
    }
  })

  it(`leaves plain text alone`, () => {
    expect(unescapeReporterText(`Login button unresponsive`)).toBe(
      `Login button unresponsive`
    )
  })
})
