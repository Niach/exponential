import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-search-phrase.json"
import searchList from "@exp/domain-contract/fixtures/search-list.json"
import { issueSearchSuggestion, translateIssueSearchPhrase, type IssueSearchPhraseMember } from "./issue-search-phrase"

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

// search-list.json SUGGESTION: the row the global search puts on top, ×4
// (desktop `domain::search_list::suggestion`, iOS + Android `SearchList`).
describe(`issue search suggestion (search-list.json)`, () => {
  for (const c of searchList.suggestion.cases) {
    it(c.name, () => {
      expect(issueSearchSuggestion(c.text, { members: c.members, now: Date.parse(c.now) })).toEqual(c.expected)
    })
  }
})
