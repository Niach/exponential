import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/issue-nesting.json"
import {
  nestIssueRows,
  type NestedIssueRow,
  type NestingRelation,
} from "./issue-nesting"

// EXP-980: the list-nesting rule, locked ×4 (Android IssueNestingTest, iOS
// IssueNestingTests, desktop domain::issue_nesting) against the ONE contract
// fixture — same cases, same test names.
interface FixtureCase {
  name: string
  groups: string[][]
  identifiers: Record<string, string>
  relations: NestingRelation[]
  expected: NestedIssueRow[][]
}

const cases = fixture as unknown as FixtureCase[]

describe(`issue nesting (contract fixture)`, () => {
  for (const c of cases) {
    it(c.name, () => {
      expect(
        nestIssueRows(c.groups, c.relations, (id) => c.identifiers[id] ?? id)
      ).toEqual(c.expected)
    })
  }
})
