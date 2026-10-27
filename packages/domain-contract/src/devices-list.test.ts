import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/devices-list.json" with { type: "json" }
import { deviceListRow, type DeviceListInput } from "./devices-list"

describe(`deviceListRow`, () => {
  for (const testCase of fixture.cases) {
    test(testCase.name, () => {
      expect(deviceListRow(testCase.input as DeviceListInput)).toEqual(
        testCase.expect as ReturnType<typeof deviceListRow>
      )
    })
  }
})
