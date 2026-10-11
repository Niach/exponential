import { describe, expect, it } from "vitest"
import icons from "@exp/icons/icons.json"
import fixture from "@exp/domain-contract/fixtures/empty-state.json"
import { EMPTY_STATES, emptyStateCopy } from "@/lib/empty-states"

// EXP-1274: the ×4 copy contract. The natives' suites (EmptyStateCopyTests,
// EmptyStateCopyTest, empty_state_copy.rs) replay the same file.

describe(`empty-state.json`, () => {
  const concepts = new Set(Object.keys(icons.semantic))

  it(`names every glyph by an existing concept`, () => {
    for (const [id, state] of Object.entries(fixture.states)) {
      expect(concepts.has(state.icon), `${id}: ${state.icon}`).toBe(true)
    }
  })

  it(`keeps titles short and never explains in them`, () => {
    for (const [id, state] of Object.entries(fixture.states)) {
      expect(state.title.split(` `).length, id).toBeLessThanOrEqual(5)
      expect(state.title.endsWith(`.`), id).toBe(false)
    }
  })

  it(`gives list states no description and no action`, () => {
    for (const [id, state] of Object.entries(fixture.states)) {
      if (state.size !== `list`) continue
      expect(`description` in state, id).toBe(false)
      expect(`action` in state, id).toBe(false)
    }
  })

  it(`ends every description with a full stop`, () => {
    for (const [id, state] of Object.entries(fixture.states)) {
      if (!(`description` in state)) continue
      expect(state.description.endsWith(`.`), id).toBe(true)
    }
  })

  it(`exposes the states by id`, () => {
    expect(emptyStateCopy(`mcp-servers`)).toEqual({
      size: `list`,
      icon: `settings-mcp`,
      title: `No MCP servers yet`,
    })
    expect(Object.keys(EMPTY_STATES).length).toBe(Object.keys(fixture.states).length)
  })
})
