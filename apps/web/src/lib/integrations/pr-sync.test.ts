import { describe, expect, it } from "vitest"
import {
  parseIssueIdentifierFromBranch,
  prStateTransitionAllowed,
} from "@/lib/integrations/pr-sync"

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

  // Batch coding runs work on exp/batch-<id8> with a LOWERCASE hex id by
  // construction — the desktop launcher guarantees it so this parser can never
  // mis-link a batch PR to an issue. Batch PRs resolve to their issues by
  // exact pr_url only. If this contract changes, the desktop's
  // batch_branch_name guard test must change with it.
  it(`never matches batch integration branches (lowercase batch-<hex> tail)`, () => {
    expect(parseIssueIdentifierFromBranch(`exp/batch-a1b2c3d4`)).toBeNull()
    expect(parseIssueIdentifierFromBranch(`exp/batch-12345678`)).toBeNull()
    expect(parseIssueIdentifierFromBranch(`exp/batch-deadbeef`)).toBeNull()
  })
})

// One issue = one PR: the merge/close writers must only act for the issue's
// LINKED PR — the webhook's branch-identifier fallback can resolve a second
// PR (e.g. `backport/EXP-42`) onto an issue whose real PR is still open
// (REV-22), and a closed-unmerged PR must flip open→closed exactly once
// (REV-23).
describe(`prStateTransitionAllowed`, () => {
  const LINKED = `https://github.com/org/a/pull/1`
  const OTHER = `https://github.com/org/a/pull/2`

  it(`allows the linked PR to merge an open issue`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: LINKED },
        { to: `merged`, prUrl: LINKED }
      )
    ).toBe(true)
  })

  it(`refuses a DIFFERENT PR from merging the issue (branch-fallback misattribution)`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: LINKED },
        { to: `merged`, prUrl: OTHER }
      )
    ).toBe(false)
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: LINKED },
        { to: `closed`, prUrl: OTHER }
      )
    ).toBe(false)
  })

  it(`is idempotent on merged (webhook + cron double-fire)`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `merged`, prUrl: LINKED },
        { to: `merged`, prUrl: LINKED }
      )
    ).toBe(false)
  })

  it(`allows merging a closed-then-reopened PR (closed → merged)`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `closed`, prUrl: LINKED },
        { to: `merged`, prUrl: LINKED }
      )
    ).toBe(true)
  })

  it(`closes only from open`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: LINKED },
        { to: `closed`, prUrl: LINKED }
      )
    ).toBe(true)
    expect(
      prStateTransitionAllowed(
        { prState: `merged`, prUrl: LINKED },
        { to: `closed`, prUrl: LINKED }
      )
    ).toBe(false)
    expect(
      prStateTransitionAllowed(
        { prState: `closed`, prUrl: LINKED },
        { to: `closed`, prUrl: LINKED }
      )
    ).toBe(false)
    expect(
      prStateTransitionAllowed(
        { prState: null, prUrl: null },
        { to: `closed`, prUrl: OTHER }
      )
    ).toBe(false)
  })

  it(`reopens only from closed (webhook 'reopened' heals the badge)`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `closed`, prUrl: LINKED },
        { to: `open`, prUrl: LINKED }
      )
    ).toBe(true)
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: LINKED },
        { to: `open`, prUrl: LINKED }
      )
    ).toBe(false)
    expect(
      prStateTransitionAllowed(
        { prState: `merged`, prUrl: LINKED },
        { to: `open`, prUrl: LINKED }
      )
    ).toBe(false)
    expect(
      prStateTransitionAllowed(
        { prState: `closed`, prUrl: LINKED },
        { to: `open`, prUrl: OTHER }
      )
    ).toBe(false)
  })

  it(`skips the URL guard when either side has no URL (tRPC mergePr passes the stored URL; legacy rows may lack one)`, () => {
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: null },
        { to: `merged`, prUrl: OTHER }
      )
    ).toBe(true)
    expect(
      prStateTransitionAllowed(
        { prState: `open`, prUrl: LINKED },
        { to: `merged` }
      )
    ).toBe(true)
  })
})
