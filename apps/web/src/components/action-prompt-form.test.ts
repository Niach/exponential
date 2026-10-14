import { describe, expect, it, vi } from "vitest"

vi.mock(`@/lib/collections`, () => ({ actionCollection: {} }))
vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

const { draftsToInputs, inputKeyFromLabel } = await import(
  `@/components/action-prompt-form`
)

// EXP-1293: an action's typed picks are edited by LABEL; the stable key the
// runs refer to is kept for an existing input and minted (snake_case, unique,
// ≤32) for a new one — the server's `actionInputsSchema` rules.
describe(`action input drafts`, () => {
  it(`mints snake_case keys from labels`, () => {
    expect(inputKeyFromLabel(`Target board`)).toBe(`target_board`)
    expect(inputKeyFromLabel(`  2nd PR!  `)).toBe(`nd_pr`)
    expect(inputKeyFromLabel(`???`)).toBe(`input`)
    expect(inputKeyFromLabel(`x`.repeat(40))).toHaveLength(32)
  })

  it(`keeps existing keys, de-duplicates new ones and drops blank rows`, () => {
    expect(
      draftsToInputs([
        { key: `repo`, label: `Repository`, type: `repo`, required: true },
        { label: `Repo`, type: `repo`, required: false },
        { label: `   `, type: `issue`, required: false },
        { label: `Issue`, type: `issue`, required: false },
      ])
    ).toEqual([
      { key: `repo`, label: `Repository`, type: `repo`, required: true },
      { key: `repo_2`, label: `Repo`, type: `repo`, required: false },
      { key: `issue`, label: `Issue`, type: `issue`, required: false },
    ])
  })
})
