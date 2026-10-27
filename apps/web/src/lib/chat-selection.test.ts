import { describe, expect, test } from "vitest"
import { chatSelectionClick, chatSelectionEnter, chatSelectionPrune } from "./chat-selection"

const ORDER = [`a`, `b`, `c`, `d`]
const click = (
  selection: Parameters<typeof chatSelectionClick>[0],
  id: string,
  mods: { toggle?: boolean; range?: boolean } = {}
) => chatSelectionClick(selection, { id, order: ORDER, toggle: mods.toggle ?? false, range: mods.range ?? false })

describe(`chat selection (#EXP-1388)`, () => {
  test(`a plain click opens while not selecting; cmd/ctrl and shift start the selection`, () => {
    expect(click(null, `b`)).toEqual({ selection: null, open: true })
    expect(click(null, `b`, { toggle: true })).toEqual({ selection: { ids: [`b`], anchor: `b` }, open: false })
    expect(click(null, `b`, { range: true })).toEqual({ selection: { ids: [`b`], anchor: `b` }, open: false })
  })

  test(`while selecting a click toggles, never opens; the last one out leaves`, () => {
    const one = { ids: [`b`], anchor: `b` }
    expect(click(one, `d`)).toEqual({ selection: { ids: [`b`, `d`], anchor: `d` }, open: false })
    expect(click(one, `b`)).toEqual({ selection: null, open: false })
    expect(click({ ids: [`b`, `d`], anchor: `d` }, `b`, { toggle: true }).selection).toEqual({ ids: [`d`], anchor: `b` })
  })

  test(`shift adds the range from the last clicked row, keeping selection order`, () => {
    expect(click({ ids: [`d`, `a`], anchor: `a` }, `c`, { range: true }).selection).toEqual({
      ids: [`d`, `a`, `b`, `c`],
      anchor: `c`,
    })
  })

  test(`enter adds a row once (a long-press, the menu's Select)`, () => {
    expect(chatSelectionEnter(null, `c`)).toEqual({ ids: [`c`], anchor: `c` })
    expect(chatSelectionEnter({ ids: [`c`], anchor: `c` }, `c`)).toEqual({ ids: [`c`], anchor: `c` })
    expect(chatSelectionEnter({ ids: [`c`], anchor: `c` }, `a`)).toEqual({ ids: [`c`, `a`], anchor: `a` })
  })

  test(`prune keeps only rows still listed; none left = not selecting`, () => {
    const selection = { ids: [`a`, `c`], anchor: `c` }
    expect(chatSelectionPrune(selection, ORDER)).toBe(selection)
    expect(chatSelectionPrune(selection, [`a`, `b`])).toEqual({ ids: [`a`], anchor: null })
    expect(chatSelectionPrune(selection, [`b`])).toBeNull()
    expect(chatSelectionPrune(null, ORDER)).toBeNull()
  })
})
