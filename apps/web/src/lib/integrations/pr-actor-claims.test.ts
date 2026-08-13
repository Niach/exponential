import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  _clearPrActorClaims,
  claimPrMerge,
  claimPrOpen,
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
