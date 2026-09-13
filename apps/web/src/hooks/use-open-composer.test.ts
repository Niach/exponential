import { describe, expect, it, vi } from "vitest"

vi.mock(`@tanstack/react-router`, () => ({
  useNavigate: () => vi.fn(),
  useParams: () => ({}),
  useLocation: () => ``,
  useSearch: () => null,
}))

import { composerSearch } from "@/hooks/use-open-composer"

// EXP-870: the composer URL for a seed and an origin.
describe(`composerSearch`, () => {
  it(`carries the origin's token beside the seed`, () => {
    expect(
      composerSearch({ actionId: `a1` }, { kind: `board`, boardSlug: `web` })
    ).toEqual({ action: `a1`, from: `board:web` })
  })

  // A pinned action is context-free: no `from`, so the Agent page opens
  // full-width and the run it starts brings no list along.
  it(`omits from for a context-free (pinned) start`, () => {
    expect(composerSearch({ actionId: `a1` }, null)).toEqual({ action: `a1` })
  })

  it(`keeps every seeded field`, () => {
    expect(
      composerSearch({ issueIds: [`i1`, `i2`], text: `go` }, { kind: `inbox` })
    ).toEqual({ issues: `i1,i2`, text: `go`, from: `inbox` })
  })
})
