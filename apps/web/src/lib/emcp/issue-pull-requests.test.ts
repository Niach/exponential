import { describe, expect, it, vi } from "vitest"

vi.mock(`@/db/connection`, () => ({ db: {} }))

import { issuePullRequestStatus, issuePullRequestWire, type IssuePullRequestRow } from "./issue-pull-requests"

const row = (over: Partial<IssuePullRequestRow> = {}): IssuePullRequestRow => ({
  id: `pr-1`,
  number: 12,
  url: `https://github.com/acme/app/pull/12`,
  state: `open`,
  checksState: `pending`,
  mergeState: `checks`,
  mergeWait: `checks`,
  mergePosition: null,
  mergeHold: null,
  ...over,
})

describe(`an issue's pull requests (#NEW-23)`, () => {
  it(`words a PR with the header pill's labels`, () => {
    expect(issuePullRequestStatus(row())).toBe(`Checks running`)
    expect(issuePullRequestStatus(row({ mergeWait: `session` }))).toBe(`Session working`)
    expect(issuePullRequestStatus(row({ state: `draft`, mergeState: `draft`, mergeWait: `draft` }))).toBe(`Draft`)
    expect(issuePullRequestStatus(row({ mergeState: `queued`, mergeWait: null, mergePosition: 2 }))).toBe(`Queued · 2nd`)
    expect(issuePullRequestStatus(row({ mergeState: `testing`, mergeWait: null }), 3)).toBe(`Testing with 3`)
    expect(issuePullRequestStatus(row({ mergeState: `testing`, mergeWait: null }))).toBe(`Testing`)
    expect(issuePullRequestStatus(row({ mergeState: `failing`, mergeWait: null }))).toBe(`Checks failing`)
    expect(issuePullRequestStatus(row({ mergeState: `held`, mergeHold: `person` }))).toBe(`On hold`)
  })

  it(`reads merged and closed from the PR's own state`, () => {
    // A PR merged outside the train keeps its last merge_state.
    expect(issuePullRequestStatus(row({ state: `merged`, mergeState: `checks` }))).toBe(`Merged`)
    expect(issuePullRequestStatus(row({ state: `closed`, mergeState: `checks` }))).toBe(`Closed`)
  })

  it(`answers the wire row`, () => {
    expect(issuePullRequestWire(row({ mergeState: `queued`, mergeWait: null, mergePosition: 1, checksState: `success` }))).toEqual({
      pullRequestId: `pr-1`,
      number: 12,
      url: `https://github.com/acme/app/pull/12`,
      state: `open`,
      status: `Queued · 1st`,
      checks: `success`,
    })
  })
})
