import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-search-translation.json"
import type { IssueSearchPhraseMember } from "./issue-search-phrase"
import {
  ISSUE_SEARCH_TRANSLATION,
  issueSearchTranslationPrompt,
  issueSearchWantsTranslation,
  sanitizeIssueSearchTranslation,
} from "./issue-search-translation"

// #NEW-136: the model fallback's pure half, locked on the three clients that
// carry it (desktop `domain::issue_search_translation`, iOS
// `IssueSearchTranslationTests`; web is the reference and calls no model)
// against the ONE contract fixture: same cases, same test names.
interface Case {
  name: string
  now: string
  members: IssueSearchPhraseMember[]
  text: string
}

const options = (c: Case) => ({ members: c.members, now: Date.parse(c.now) })

describe(`issue search translation: wants (contract fixture)`, () => {
  for (const c of fixture.wantsCases as (Case & { expected: boolean })[]) {
    it(c.name, () => {
      expect(issueSearchWantsTranslation(c.text, options(c))).toBe(c.expected)
    })
  }
})

describe(`issue search translation: sanitize (contract fixture)`, () => {
  for (const c of fixture.sanitizeCases as (Case & { output: string; expected: string | null })[]) {
    it(c.name, () => {
      expect(sanitizeIssueSearchTranslation(c.output, c.text, options(c))).toBe(c.expected)
    })
  }
})

describe(`issue search translation: prompt (contract fixture)`, () => {
  for (const c of fixture.promptCases as (Case & { expected: string })[]) {
    it(c.name, () => {
      expect(issueSearchTranslationPrompt(c.text, options(c))).toBe(c.expected)
    })
  }

  it(`never carries an email`, () => {
    for (const c of fixture.promptCases as (Case & { expected: string })[]) {
      expect(c.expected).not.toContain(`@`)
      expect(c.expected).not.toContain(`acme`)
    }
  })

  it(`the limits are the decided ones`, () => {
    expect(ISSUE_SEARCH_TRANSLATION.minWords).toBe(4)
    expect(ISSUE_SEARCH_TRANSLATION.timeoutSeconds).toBe(15)
    expect(ISSUE_SEARCH_TRANSLATION.model).toBe(`haiku`)
  })
})
