import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-search-phrase.json"
import { translateIssueSearchPhrase, type IssueSearchPhraseMember } from "./issue-search-phrase"

// #NEW-133: the phrase grammar, locked ×4 (Android IssueSearchPhraseTest, iOS
// IssueSearchPhraseTests, desktop domain::issue_search_phrase) against the
// ONE contract fixture: same cases, same test names.
interface PhraseCase {
  name: string
  now: string
  members: IssueSearchPhraseMember[]
  phrase: string
  expected: string | null
}

describe(`issue search phrase (contract fixture)`, () => {
  for (const c of (fixture as { cases: PhraseCase[] }).cases) {
    it(c.name, () => {
      expect(translateIssueSearchPhrase(c.phrase, { members: c.members, now: Date.parse(c.now) })).toBe(c.expected)
    })
  }
})
