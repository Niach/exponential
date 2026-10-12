import { describe, expect, it } from "vitest"
import spec from "@exp/domain-contract/fixtures/getting-started.json"
import icons from "@exp/icons/icons.json"
import {
  GETTING_STARTED_STEPS,
  deriveGettingStarted,
  type GettingStartedPermissions,
  type GettingStartedPlatform,
  type GettingStartedSignals,
} from "./getting-started-model"

// EXP-1270: the Getting started model replays getting-started.json — the same
// case names the desktop, iOS and Android suites run, compared WHOLE.

describe(`deriveGettingStarted`, () => {
  for (const testCase of spec.cases) {
    it(testCase.name, () => {
      expect(
        deriveGettingStarted(
          testCase.signals as GettingStartedSignals,
          testCase.permissions as GettingStartedPermissions,
          testCase.platform as GettingStartedPlatform
        )
      ).toEqual(testCase.expected)
    })
  }

  it(`names every step's glyph and the Docs button by an existing concept`, () => {
    const concepts = new Set(Object.keys(icons.semantic))
    for (const step of GETTING_STARTED_STEPS) {
      expect(concepts.has(step.icon), step.icon).toBe(true)
    }
    expect(concepts.has(spec.docs.icon)).toBe(true)
  })

  it(`every doneWhen names real signals`, () => {
    const signals = Object.keys(spec.cases[0]!.signals)
    for (const step of GETTING_STARTED_STEPS) {
      for (const name of step.doneWhen.split(`||`)) {
        expect(signals).toContain(name.trim())
      }
    }
  })
})
