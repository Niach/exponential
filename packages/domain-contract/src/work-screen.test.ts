import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/work-screen.json" with { type: "json" }
import {
  overlayDisplay,
  workDisplays,
  workScreenCurrent,
  workScreenGate,
  workScreenOptions,
  workScreenPickedId,
  workScreenPickerKey,
  type WorkScreenCall,
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
  for (const c of fixture.gateCases) {
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
