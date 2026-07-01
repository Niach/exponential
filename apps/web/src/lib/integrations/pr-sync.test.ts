import { describe, expect, it } from "vitest"
import { parseIssueIdentifierFromBranch } from "@/lib/integrations/pr-sync"

describe(`parseIssueIdentifierFromBranch`, () => {
  it(`parses the launcher's exp/<IDENTIFIER> convention`, () => {
    expect(parseIssueIdentifierFromBranch(`exp/MET-12`)).toBe(`MET-12`)
    expect(parseIssueIdentifierFromBranch(`exp/EXP-1`)).toBe(`EXP-1`)
    expect(parseIssueIdentifierFromBranch(`exp/ABC123-9`)).toBe(`ABC123-9`)
  })

  it(`accepts custom prefixes and nested slashes (trailing tail wins)`, () => {
    expect(parseIssueIdentifierFromBranch(`feature/MET-12`)).toBe(`MET-12`)
    expect(parseIssueIdentifierFromBranch(`user/foo/MET-42`)).toBe(`MET-42`)
    expect(parseIssueIdentifierFromBranch(`MET-7`)).toBe(`MET-7`)
  })

  it(`rejects branches without an identifier tail`, () => {
    expect(parseIssueIdentifierFromBranch(`exp/foo`)).toBeNull()
    expect(parseIssueIdentifierFromBranch(`main`)).toBeNull()
    expect(parseIssueIdentifierFromBranch(`develop`)).toBeNull()
    expect(parseIssueIdentifierFromBranch(`release/1.2.0`)).toBeNull()
  })

  it(`is case-sensitive on the identifier (stored identifiers are uppercase)`, () => {
    expect(parseIssueIdentifierFromBranch(`exp/met-12`)).toBeNull()
  })

  it(`rejects extra segments after the identifier tail`, () => {
    expect(parseIssueIdentifierFromBranch(`exp/MET-12-extra`)).toBeNull()
    expect(parseIssueIdentifierFromBranch(`exp/MET-12/sub`)).toBeNull()
  })
})
