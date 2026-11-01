import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/devices-list.json" with { type: "json" }
import { deviceCardAccountIcons, deviceCardColumns, deviceListRow, type DeviceListInput } from "./devices-list"

describe(`deviceListRow`, () => {
  for (const testCase of fixture.cases) {
    test(testCase.name, () => {
      expect(deviceListRow(testCase.input as DeviceListInput)).toEqual(
        testCase.expect as ReturnType<typeof deviceListRow>
      )
    })
  }
})

describe(`deviceCardAccountIcons`, () => {
  for (const testCase of fixture.accountIcons) {
    test(testCase.name, () => {
      expect(deviceCardAccountIcons(testCase.input.count, testCase.input.expanded)).toEqual(testCase.expect)
    })
  }
})

test(`deviceCardColumns: a portrait phone gets 1, landscape 2+`, () => {
  expect(deviceCardColumns(358)).toBe(1)
  expect(deviceCardColumns(1100)).toBe(4)
})
