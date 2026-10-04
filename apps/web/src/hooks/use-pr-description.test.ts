import { beforeEach, describe, expect, it, vi } from "vitest"
import { renderHook, waitFor } from "@testing-library/react"

// EXP-1154: the PR body fetch behind the Results face's fallback (lifted from
// EXP-1139's review-page card).

const mocks = vi.hoisted(() => ({ prDescription: vi.fn() }))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { issues: { prDescription: { query: mocks.prDescription } } },
}))

import { usePrDescription } from "@/hooks/use-pr-description"

const ISSUE = { id: `11111111-1111-4111-8111-111111111111`, prNumber: 880 }

beforeEach(() => {
  vi.clearAllMocks()
  mocks.prDescription.mockResolvedValue({
    repo: `acme/app`,
    prNumber: 880,
    url: `https://github.com/acme/app/pull/880`,
    title: `EXP-792: MCP servers rework`,
    body: null,
    state: `open`,
  })
})

describe(`usePrDescription`, () => {
  it(`loads GitHub's title and body (a null body reads as blank)`, async () => {
    const { result } = renderHook(() => usePrDescription(ISSUE))
    await waitFor(() => expect(result.current.state.kind).toBe(`ready`))
    expect(result.current.state).toEqual({
      kind: `ready`,
      title: `EXP-792: MCP servers rework`,
      body: ``,
      state: `open`,
    })
    expect(mocks.prDescription).toHaveBeenCalledWith({ issueId: ISSUE.id })
  })

  it(`stays idle while disabled or without a PR`, () => {
    const { result } = renderHook(() =>
      usePrDescription(ISSUE, { enabled: false })
    )
    expect(result.current.state.kind).toBe(`idle`)
    renderHook(() => usePrDescription({ id: ISSUE.id, prNumber: null }))
    expect(mocks.prDescription).not.toHaveBeenCalled()
  })

  it(`reports a refusal and an unlinked PR`, async () => {
    mocks.prDescription.mockRejectedValueOnce(new Error(`GitHub is down`))
    const { result } = renderHook(() => usePrDescription(ISSUE))
    await waitFor(() =>
      expect(result.current.state).toEqual({
        kind: `error`,
        message: `GitHub is down`,
      })
    )
    mocks.prDescription.mockResolvedValueOnce({ title: null, body: null, state: null })
    const second = renderHook(() => usePrDescription(ISSUE))
    await waitFor(() =>
      expect(second.result.current.state).toEqual({
        kind: `error`,
        message: `No pull request is linked.`,
      })
    )
  })
})
