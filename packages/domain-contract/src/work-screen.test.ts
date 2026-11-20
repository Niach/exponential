import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/work-screen.json" with { type: "json" }
import {
  overlayDisplay,
  place,
  spaceOffered,
  workDisplays,
  workScreenCurrent,
  workScreenGate,
  workScreenOptions,
  workScreenPickedId,
  workScreenPickerKey,
  type WorkScreenCall,
  type WorkScreenPlaceInput,
  type WorkScreenState,
} from "./work-screen.ts"

describe(`work-screen.json`, () => {
  for (const c of fixture.optionCases) {
    it(`options: ${c.name}`, () => {
      expect(workScreenOptions(c.displays, c.stored)).toEqual(c.expect)
    })
  }
  for (const c of fixture.pickerKeyCases) {
    it(`picker key: ${c.value ?? `Auto`}`, () => {
      expect(workScreenPickerKey(c.value)).toBe(c.key)
      expect(workScreenPickedId(c.key)).toBe(c.value)
    })
  }
  for (const c of [...fixture.gateCases, ...fixture.spaceGateCases]) {
    it(`gate: ${c.name}`, () => {
      const state = c.state as WorkScreenState
      expect(workDisplays(state)).toEqual(c.work)
      for (const { call, expect: want } of c.calls) {
        expect(workScreenGate(state, call as WorkScreenCall)).toEqual(want)
      }
    })
  }
  for (const c of fixture.currentCases) {
    it(`current: ${c.name}`, () => {
      expect(workScreenCurrent(c.work, c.primary, c.previous)).toBe(c.expect)
    })
  }
  for (const c of fixture.overlayCases) {
    it(`overlay: ${c.name}`, () => {
      expect(overlayDisplay(c.state)).toBe(c.expect)
    })
  }
})

// #EXP-1514: the agent space. New cases sit under NEW keys (every mirror
// loops over `optionCases`, `pickerKeyCases` and `gateCases` whole).
describe(`work-screen.json: the agent space`, () => {
  it(`the picker's words live in \`space\`: no key joins \`copy\` outside \`refusals\``, () => {
    // Both phones lock this key set (WorkScreenTest.kt, WorkScreenTests.swift).
    expect(Object.keys(fixture.copy).filter((key) => key !== `refusals`)).toEqual([
      `row`,
      `auto`,
      `display`,
      `primary`,
      `notConnected`,
    ])
    expect(fixture.pickerKeys.space).toBe(fixture.space.value)
  })
  for (const c of fixture.spaceOptionCases) {
    it(`options: ${c.name}`, () => {
      expect(workScreenOptions(c.displays, c.stored, c.spaceOffered)).toEqual(c.expect)
    })
  }
  for (const c of fixture.spaceKeyCases) {
    it(`picker key: ${c.value ?? `Auto`}`, () => {
      expect(workScreenPickerKey(c.value)).toBe(c.key)
      expect(workScreenPickedId(c.key)).toBe(c.value)
    })
  }
  for (const c of fixture.placeCases) {
    it(`place: ${c.name}`, () => {
      expect(place(c as WorkScreenPlaceInput)).toBe(c.expect as ReturnType<typeof place>)
    })
  }
  it(`spaceOffered: an agent_space item in state ok (device-doctor.json)`, () => {
    const items = (space: { state: string } | null) => [
      { key: `git`, group: `required`, state: `ok` },
      { key: `computer_use`, group: `computer_use`, state: `ok` },
      ...(space ? [{ key: `agent_space`, group: `computer_use`, parent: `computer_use`, ...space }] : []),
    ]
    expect(spaceOffered({ items: items({ state: `ok` }) })).toBe(true)
    expect(spaceOffered({ items: items({ state: `missing` }) })).toBe(false)
    expect(spaceOffered({ items: items(null) })).toBe(false)
    expect(spaceOffered({ items: [] })).toBe(false)
    expect(spaceOffered(null)).toBe(false)
    expect(spaceOffered(undefined)).toBe(false)
  })
})
