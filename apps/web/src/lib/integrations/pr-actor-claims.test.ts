import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  _clearPrActorClaims,
  claimPrMerge,
  claimPrOpen,
  noteAgentIssueActivity,
  peekAgentIssueActors,
  releasePrMergeClaim,
  releasePrOpenClaim,
  takePrMergeClaim,
  takePrOpenClaim,
} from "@/lib/integrations/pr-actor-claims"

// Locks the EXP-494 claim contract: an in-app PR open/merge records its
// initiator before the GitHub call, the webhook consumes it exactly once, and
// everything degrades to a null (today's fallback behavior) — expiry, release
// on a failed GitHub call, or a plain miss.

describe(`pr-actor-claims`, () => {
  beforeEach(() => {
    _clearPrActorClaims()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it(`round-trips a claim and consumes it on take`, () => {
    claimPrOpen(`acme/app`, `exp/EXP-9`, { userId: `u1`, viaAgent: true })
    expect(takePrOpenClaim(`acme/app`, `exp/EXP-9`)).toEqual({
      userId: `u1`,
      viaAgent: true,
    })
    // Take = consume: a redelivery gets nothing.
    expect(takePrOpenClaim(`acme/app`, `exp/EXP-9`)).toBeNull()
  })

  it(`treats repo names case-insensitively but branches case-sensitively`, () => {
    claimPrOpen(`Acme/App`, `exp/EXP-9`, { userId: `u1`, viaAgent: false })
    expect(takePrOpenClaim(`acme/app`, `exp/exp-9`)).toBeNull()
    expect(takePrOpenClaim(`acme/app`, `exp/EXP-9`)).toEqual({
      userId: `u1`,
      viaAgent: false,
    })
  })

  it(`keeps open and merge claims isolated`, () => {
    claimPrMerge(`acme/app`, 12, { userId: `u1`, viaAgent: false })
    expect(takePrOpenClaim(`acme/app`, `12`)).toBeNull()
    expect(takePrMergeClaim(`acme/app`, 12)).toEqual({
      userId: `u1`,
      viaAgent: false,
    })
  })

  it(`expires claims after the TTL`, () => {
    vi.useFakeTimers()
    claimPrMerge(`acme/app`, 7, { userId: `u1`, viaAgent: true })
    vi.advanceTimersByTime(10 * 60 * 1000 + 1)
    expect(takePrMergeClaim(`acme/app`, 7)).toBeNull()
  })

  it(`release drops a claim written for a failed GitHub call`, () => {
    claimPrOpen(`acme/app`, `exp/EXP-9`, { userId: `u1`, viaAgent: true })
    releasePrOpenClaim(`acme/app`, `exp/EXP-9`)
    expect(takePrOpenClaim(`acme/app`, `exp/EXP-9`)).toBeNull()

    claimPrMerge(`acme/app`, 7, { userId: `u1`, viaAgent: false })
    releasePrMergeClaim(`acme/app`, 7)
    expect(takePrMergeClaim(`acme/app`, 7)).toBeNull()
  })

  it(`re-claiming the same key overwrites the actor`, () => {
    claimPrMerge(`acme/app`, 7, { userId: `u1`, viaAgent: false })
    claimPrMerge(`acme/app`, 7, { userId: `u2`, viaAgent: true })
    expect(takePrMergeClaim(`acme/app`, 7)).toEqual({
      userId: `u2`,
      viaAgent: true,
    })
  })

  it(`evicts the oldest claim past the cap`, () => {
    for (let i = 0; i < 1000; i++) {
      claimPrMerge(`acme/app`, i, { userId: `u1`, viaAgent: false })
    }
    claimPrMerge(`acme/app`, 1000, { userId: `u1`, viaAgent: false })
    expect(takePrMergeClaim(`acme/app`, 0)).toBeNull()
    expect(takePrMergeClaim(`acme/app`, 1)).not.toBeNull()
    expect(takePrMergeClaim(`acme/app`, 1000)).not.toBeNull()
  })
})

// EXP-617: the issue-keyed agent-activity record. Different lifetime rules
// from the PR claims above on purpose — a set per issue, and PEEKED rather
// than consumed, because one record has to cover the `opened` webhook, the
// tool's own fan-out and any later merge.
describe(`agent issue activity`, () => {
  beforeEach(() => {
    _clearPrActorClaims()
  })
  afterEach(() => {
    vi.useRealTimers()
  })

  it(`peeks without consuming`, () => {
    noteAgentIssueActivity(`issue-1`, `u1`)
    expect(peekAgentIssueActors(`issue-1`)).toEqual([`u1`])
    // Unlike takePr*Claim, reading it again still returns it.
    expect(peekAgentIssueActors(`issue-1`)).toEqual([`u1`])
  })

  it(`accumulates several actors per issue and keeps issues apart`, () => {
    noteAgentIssueActivity(`issue-1`, `u1`)
    noteAgentIssueActivity(`issue-1`, `u2`)
    noteAgentIssueActivity(`issue-2`, `u3`)
    expect(peekAgentIssueActors(`issue-1`).sort()).toEqual([`u1`, `u2`])
    expect(peekAgentIssueActors(`issue-2`)).toEqual([`u3`])
    expect(peekAgentIssueActors(`issue-3`)).toEqual([])
  })

  // The real EXP-617 incident had 42 minutes between the agent filing the
  // issue and the PR being opened, so anything under an hour is useless here.
  it(`still covers the issue an hour later, and expires by four`, () => {
    vi.useFakeTimers()
    noteAgentIssueActivity(`issue-1`, `u1`)
    vi.advanceTimersByTime(60 * 60 * 1000)
    expect(peekAgentIssueActors(`issue-1`)).toEqual([`u1`])
    vi.advanceTimersByTime(3 * 60 * 60 * 1000 + 60 * 1000)
    // Still bounded: the window is the mitigation for suppressing a teammate
    // who merely touched the issue, so it must not outlive itself.
    expect(peekAgentIssueActors(`issue-1`)).toEqual([])
  })

  it(`re-noting refreshes the actor's expiry`, () => {
    vi.useFakeTimers()
    noteAgentIssueActivity(`issue-1`, `u1`)
    vi.advanceTimersByTime(3 * 60 * 60 * 1000)
    noteAgentIssueActivity(`issue-1`, `u1`)
    vi.advanceTimersByTime(3 * 60 * 60 * 1000)
    expect(peekAgentIssueActors(`issue-1`)).toEqual([`u1`])
  })

  it(`evicts the oldest issue past the cap`, () => {
    for (let i = 0; i < 1000; i++) {
      noteAgentIssueActivity(`issue-${i}`, `u1`)
    }
    noteAgentIssueActivity(`issue-1000`, `u1`)
    expect(peekAgentIssueActors(`issue-0`)).toEqual([])
    expect(peekAgentIssueActors(`issue-1000`)).toEqual([`u1`])
  })
})
