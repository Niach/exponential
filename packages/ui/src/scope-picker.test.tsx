import { fireEvent, render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import {
  EMPTY_SCOPE_SELECTION,
  ScopePicker,
  effectiveScopeSelection,
  hasScopeSelection,
  scopeCaption,
  type ScopePickerTeam,
  type ScopeSelection,
} from "./scope-picker"

// FEED-76: the consent screen and the Create-API-key dialog share this
// control, so its behaviour is locked here once.

const TREE: ScopePickerTeam[] = [
  {
    id: `t-acme`,
    name: `Acme`,
    boards: [
      { id: `b-web`, name: `Web`, prefix: `WEB` },
      { id: `b-mob`, name: `Mobile`, prefix: `MOB` },
    ],
  },
  { id: `t-lab`, name: `Lab`, boards: [] },
]

const scoped = (partial: Partial<ScopeSelection>): ScopeSelection => ({
  allTeams: false,
  teamIds: [],
  boardIds: [],
  ...partial,
})

describe(`ScopePicker`, () => {
  it(`starts on Everything and hides the tree until it is switched off`, () => {
    const onChange = vi.fn()
    const { container, getByRole } = render(
      <ScopePicker tree={TREE} value={EMPTY_SCOPE_SELECTION} onChange={onChange} />
    )
    expect(container.querySelectorAll(`[data-team]`)).toHaveLength(0)
    fireEvent.click(getByRole(`switch`))
    expect(onChange).toHaveBeenCalledWith({ allTeams: false, teamIds: [], boardIds: [] })
  })

  it(`lists every team with its boards indented and the empty-team note`, () => {
    const { container, getByText } = render(
      <ScopePicker tree={TREE} value={scoped({})} onChange={vi.fn()} />
    )
    expect(container.querySelectorAll(`[data-team]`)).toHaveLength(2)
    expect(getByText(`WEB`)).toBeTruthy()
    expect(getByText(`No boards yet.`)).toBeTruthy()
  })

  it(`ticking a team adds it; its boards read checked and disabled`, () => {
    const onChange = vi.fn()
    const { rerender, container } = render(
      <ScopePicker tree={TREE} value={scoped({})} onChange={onChange} />
    )
    fireEvent.click(container.querySelector(`#scope-team-t-acme`)!)
    expect(onChange).toHaveBeenCalledWith(scoped({ teamIds: [`t-acme`] }))
    rerender(
      <ScopePicker tree={TREE} value={scoped({ teamIds: [`t-acme`] })} onChange={onChange} />
    )
    const board = container.querySelector(`#scope-board-b-web`)!
    expect(board.getAttribute(`data-state`)).toBe(`checked`)
    expect(board.hasAttribute(`disabled`)).toBe(true)
  })

  it(`ticking and unticking a board edits boardIds only`, () => {
    const onChange = vi.fn()
    const { container, rerender } = render(
      <ScopePicker tree={TREE} value={scoped({})} onChange={onChange} />
    )
    fireEvent.click(container.querySelector(`#scope-board-b-mob`)!)
    expect(onChange).toHaveBeenLastCalledWith(scoped({ boardIds: [`b-mob`] }))
    rerender(
      <ScopePicker tree={TREE} value={scoped({ boardIds: [`b-mob`] })} onChange={onChange} />
    )
    fireEvent.click(container.querySelector(`#scope-board-b-mob`)!)
    expect(onChange).toHaveBeenLastCalledWith(scoped({ boardIds: [] }))
  })

  it(`says so when the member has no team`, () => {
    const { getByText } = render(
      <ScopePicker tree={[]} value={scoped({})} onChange={vi.fn()} />
    )
    expect(getByText(`You aren't a member of any team yet.`)).toBeTruthy()
  })
})

describe(`effectiveScopeSelection`, () => {
  it(`drops boards a selected whole team already covers`, () => {
    expect(
      effectiveScopeSelection(TREE, scoped({ teamIds: [`t-acme`], boardIds: [`b-web`] }))
    ).toEqual(scoped({ teamIds: [`t-acme`] }))
  })

  it(`keeps boards outside the selected teams`, () => {
    expect(
      effectiveScopeSelection(TREE, scoped({ teamIds: [`t-lab`], boardIds: [`b-web`] }))
    ).toEqual(scoped({ teamIds: [`t-lab`], boardIds: [`b-web`] }))
  })

  it(`Everything carries no ids at all`, () => {
    expect(
      effectiveScopeSelection(TREE, { allTeams: true, teamIds: [`t-acme`], boardIds: [`b-web`] })
    ).toEqual(EMPTY_SCOPE_SELECTION)
  })
})

describe(`hasScopeSelection / scopeCaption`, () => {
  it(`needs Everything or at least one team/board`, () => {
    expect(hasScopeSelection(EMPTY_SCOPE_SELECTION)).toBe(true)
    expect(hasScopeSelection(scoped({}))).toBe(false)
    expect(hasScopeSelection(scoped({ teamIds: [`t-lab`] }))).toBe(true)
    expect(hasScopeSelection(scoped({ boardIds: [`b-web`] }))).toBe(true)
  })

  it(`captions an unscoped key, a scoped one and an emptied one`, () => {
    expect(scopeCaption(null)).toBe(`All teams`)
    expect(
      scopeCaption({
        teams: [{ name: `Acme` }],
        boards: [{ name: `Web`, prefix: `WEB` }],
      })
    ).toBe(`Scoped to Acme, Web (WEB)`)
    expect(scopeCaption({ teams: [], boards: [] })).toBe(`Scoped to nothing reachable`)
  })
})
