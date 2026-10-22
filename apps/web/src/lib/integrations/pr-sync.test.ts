import { describe, expect, it } from "vitest"
import {
  mergedHeadDeletable,
  parseIssueIdentifierFromBranch,
  planPrAutomationTransition,
  subIssueReached,
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

describe(`planPrAutomationTransition`, () => {
  const builtinInReview = {
    id: `s-in-review`,
    builtinKey: `in_review`,
    category: `started` as const,
  }
  const builtinDone = {
    id: `s-done`,
    builtinKey: `done`,
    category: `completed` as const,
  }
  const customStarted = {
    id: `s-custom-started`,
    builtinKey: null,
    category: `started` as const,
  }
  const customCompleted = {
    id: `s-custom-completed`,
    builtinKey: null,
    category: `completed` as const,
  }

  it(`moves an eligible issue to the builtin default`, () => {
    expect(
      planPrAutomationTransition({
        event: `opened`,
        automationEnabled: true,
        current: { status: `in_progress`, statusId: `s-in-progress` },
        target: builtinInReview,
      })
    ).toEqual({ status: `in_review`, statusId: `s-in-review` })
    expect(
      planPrAutomationTransition({
        event: `merged`,
        automationEnabled: true,
        current: { status: `in_review`, statusId: `s-in-review` },
        target: builtinDone,
      })
    ).toEqual({ status: `done`, statusId: `s-done` })
  })

  it(`resumed takes an In Review issue back to In Progress, nothing else`, () => {
    const builtinInProgress = {
      id: `s-in-progress`,
      builtinKey: `in_progress`,
      category: `started` as const,
    }
    expect(
      planPrAutomationTransition({
        event: `resumed`,
        automationEnabled: true,
        current: { status: `in_review`, statusId: `s-in-review` },
        target: builtinInProgress,
      })
    ).toEqual({ status: `in_progress`, statusId: `s-in-progress` })
    for (const status of [`backlog`, `in_progress`, `done`, `cancelled`, `duplicate`]) {
      expect(
        planPrAutomationTransition({
          event: `resumed`,
          automationEnabled: true,
          current: { status, statusId: null },
          target: builtinInProgress,
        })
      ).toBeNull()
    }
    expect(
      planPrAutomationTransition({
        event: `resumed`,
        automationEnabled: false,
        current: { status: `in_review`, statusId: `s-in-review` },
        target: builtinInProgress,
      })
    ).toBeNull()
    // No builtin row: the bare anchor, status_id resolved by the trigger.
    expect(
      planPrAutomationTransition({
        event: `resumed`,
        automationEnabled: true,
        current: { status: `in_review`, statusId: null },
        target: null,
      })
    ).toEqual({ status: `in_progress` })
  })

  it(`does nothing when the team disabled the automation`, () => {
    expect(
      planPrAutomationTransition({
        event: `merged`,
        automationEnabled: false,
        current: { status: `in_review`, statusId: `s-in-review` },
        target: builtinDone,
      })
    ).toBeNull()
  })

  it(`anchors a custom target via its category`, () => {
    expect(
      planPrAutomationTransition({
        event: `opened`,
        automationEnabled: true,
        current: { status: `backlog`, statusId: `s-backlog` },
        target: customStarted,
      })
    ).toEqual({ status: `in_progress`, statusId: `s-custom-started` })
    expect(
      planPrAutomationTransition({
        event: `merged`,
        automationEnabled: true,
        current: { status: `in_review`, statusId: `s-in-review` },
        target: customCompleted,
      })
    ).toEqual({ status: `done`, statusId: `s-custom-completed` })
  })

  it(`never overrides explicit human resolutions (cancelled/duplicate anchors)`, () => {
    for (const status of [`cancelled`, `duplicate`, `done`]) {
      expect(
        planPrAutomationTransition({
          event: `merged`,
          automationEnabled: true,
          current: { status, statusId: null },
          target: builtinDone,
        })
      ).toBeNull()
    }
    // An issue already parked in review is not re-parked by a webhook replay.
    expect(
      planPrAutomationTransition({
        event: `opened`,
        automationEnabled: true,
        current: { status: `in_review`, statusId: `s-in-review` },
        target: builtinInReview,
      })
    ).toBeNull()
  })

  it(`skips when the issue already sits in the exact target row`, () => {
    // A custom started target anchors in_progress — inside the opened
    // from-set — so only the statusId equality keeps webhook replays from
    // re-firing status_changed events.
    expect(
      planPrAutomationTransition({
        event: `opened`,
        automationEnabled: true,
        current: { status: `in_progress`, statusId: `s-custom-started` },
        target: customStarted,
      })
    ).toBeNull()
  })

  it(`still moves between two rows sharing an anchor`, () => {
    expect(
      planPrAutomationTransition({
        event: `opened`,
        automationEnabled: true,
        current: { status: `in_progress`, statusId: `s-other-started` },
        target: customStarted,
      })
    ).toEqual({ status: `in_progress`, statusId: `s-custom-started` })
  })

  it(`degrades to the bare anchor enum when no target row exists (unseeded team)`, () => {
    expect(
      planPrAutomationTransition({
        event: `merged`,
        automationEnabled: true,
        current: { status: `in_progress`, statusId: null },
        target: null,
      })
    ).toEqual({ status: `done` })
  })
})

// EXP-1334: a thread link starts its issues; parents wait for sub-issues.
describe(`the started automation + the issue tree`, () => {
  const builtinInProgress = { id: `s-in-progress`, builtinKey: `in_progress`, category: `started` as const }

  it(`starts only a not-yet-started issue (backlog + unstarted customs anchor to backlog)`, () => {
    const plan = (status: string, statusId: string | null = null) =>
      planPrAutomationTransition({
        event: `started`,
        automationEnabled: true,
        current: { status, statusId },
        target: builtinInProgress,
      })
    expect(plan(`backlog`)).toEqual({ status: `in_progress`, statusId: `s-in-progress` })
    expect(plan(`in_progress`, `s-custom`)).toBeNull()
    expect(plan(`in_review`)).toBeNull()
    expect(plan(`done`)).toBeNull()
    expect(plan(`cancelled`)).toBeNull()
  })

  it(`falls back to the bare in_progress anchor without a builtin row`, () => {
    expect(
      planPrAutomationTransition({
        event: `started`,
        automationEnabled: true,
        current: { status: `backlog`, statusId: null },
        target: null,
      })
    ).toEqual({ status: `in_progress` })
  })

  it(`a sub-issue holds its parent back until it reaches the event's stage`, () => {
    const at = (status: string, statusId: string | null = null) => ({ status, statusId })
    expect(subIssueReached(`opened`, at(`in_progress`), `s-review`)).toBe(false)
    expect(subIssueReached(`opened`, at(`in_review`), `s-review`)).toBe(true)
    expect(subIssueReached(`opened`, at(`done`), null)).toBe(true)
    // A custom PR-open target anchors to in_progress: the exact row counts.
    expect(subIssueReached(`opened`, at(`in_progress`, `s-review`), `s-review`)).toBe(true)
    expect(subIssueReached(`merged`, at(`in_review`), `s-done`)).toBe(false)
    expect(subIssueReached(`merged`, at(`done`), `s-done`)).toBe(true)
    // A resolution never blocks.
    expect(subIssueReached(`merged`, at(`cancelled`), null)).toBe(true)
    expect(subIssueReached(`merged`, at(`duplicate`), null)).toBe(true)
  })
})

describe(`mergedHeadDeletable (NEW-5)`, () => {
  const base = {
    head: `exp/EXP-42`,
    anyBranch: false,
    isDefaultBranch: false,
    openChildren: 0,
    liveOnSameHead: false,
  }

  it(`a merge seen outside Exponential deletes only exp/ heads`, () => {
    expect(mergedHeadDeletable(base)).toBe(true)
    expect(mergedHeadDeletable({ ...base, head: `feature/login` })).toBe(false)
  })

  it(`our own merge deletes any head`, () => {
    expect(mergedHeadDeletable({ ...base, head: `feature/login`, anyBranch: true })).toBe(true)
  })

  it(`never a default branch, an empty head, or a head something still rests on`, () => {
    expect(mergedHeadDeletable({ ...base, anyBranch: true, isDefaultBranch: true })).toBe(false)
    expect(mergedHeadDeletable({ ...base, head: `  ` })).toBe(false)
    expect(mergedHeadDeletable({ ...base, openChildren: 1 })).toBe(false)
    expect(mergedHeadDeletable({ ...base, liveOnSameHead: true })).toBe(false)
  })
})
