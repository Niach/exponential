import { describe, expect, it } from "vitest"
import spec from "@exp/domain-contract/fixtures/device-doctor.json"
import type { DeviceDoctor } from "@/db/schema"

import { lowDiskWarning } from "./device-disk"

// EXP-1375: the start warning follows the fixture's composer warning.
describe(`lowDiskWarning`, () => {
  for (const fixtureCase of spec.cases as unknown as {
    name: string
    doctor: DeviceDoctor
    composerWarning?: string | null
  }[]) {
    it(fixtureCase.name, () => {
      const warning = lowDiskWarning(`mint`, fixtureCase.doctor)
      if (fixtureCase.composerWarning === `disk`) {
        expect(warning).toBe(`mint is low on disk: 6.2 GB free.`)
      } else {
        expect(warning).toBeNull()
      }
    })
  }

  it(`no report, no warning`, () => {
    expect(lowDiskWarning(`mint`, null)).toBeNull()
  })
})
