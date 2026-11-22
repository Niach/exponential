import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/work-screen.json"
import { workScreenPickedId, workScreenPickerKey } from "@exp/domain-contract/work-screen"
import { workScreenPickerItems } from "@/lib/work-screen-picker"

// #EXP-1514: the device settings picker replays work-screen.json through
// the device's doctor: `spaceOffered` = its `agent_space` row in state `ok`
// (device-doctor.json), every other agent_space state lists no space.
const doctor = (displays: (typeof fixture.spaceOptionCases)[number][`displays`], offered: boolean) => ({
  displays,
  items: [
    { key: `computer_use`, state: `ok` },
    { key: `agent_space`, state: offered ? `ok` : `missing` },
  ],
})

describe(`workScreenPickerItems: work-screen.json`, () => {
  for (const c of fixture.spaceOptionCases) {
    it(`spaceOptionCases: ${c.name}`, () => {
      expect(workScreenPickerItems(doctor(c.displays, c.spaceOffered), c.stored)).toEqual(
        c.expect.map((option) => ({ value: workScreenPickerKey(option.value), label: option.label }))
      )
    })
  }
  for (const c of fixture.optionCases) {
    it(`optionCases (no agent_space row): ${c.name}`, () => {
      expect(workScreenPickerItems({ displays: c.displays, items: [] }, c.stored)).toEqual(
        c.expect.map((option) => ({ value: workScreenPickerKey(option.value), label: option.label }))
      )
    })
  }
  for (const c of fixture.spaceKeyCases) {
    it(`spaceKeyCases: ${c.value ?? `Auto`}`, () => {
      expect(workScreenPickerKey(c.value)).toBe(c.key)
      expect(workScreenPickedId(c.key)).toBe(c.value)
    })
  }
  it(`a picked item stores its option's value (Agent space only = the space value)`, () => {
    const items = workScreenPickerItems(doctor(fixture.spaceOptionCases[0].displays, true), null)
    expect(items.map((item) => workScreenPickedId(item.value))).toEqual(
      fixture.spaceOptionCases[0].expect.map((option) => option.value)
    )
    expect(workScreenPickedId(items[1].value)).toBe(fixture.space.value)
  })
  it(`no doctor: Auto, and a stored space stays listed`, () => {
    expect(workScreenPickerItems(null, null)).toEqual([{ value: fixture.pickerKeys.auto, label: fixture.copy.auto }])
    expect(workScreenPickerItems(undefined, fixture.space.value)).toEqual([
      { value: fixture.pickerKeys.auto, label: fixture.copy.auto },
      { value: fixture.pickerKeys.space, label: fixture.space.only },
    ])
  })
})
