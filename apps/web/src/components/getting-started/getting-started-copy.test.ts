// EXP-1270: Getting started's copy is the fixture's. The natives port the
// same fixture (their lanes replay its `cases` and read its strings), so the
// gate that used to grep three native copy tables now pins the web table to
// the fixture, and the fixture's strings to what Swift, Kotlin and Rust can
// quote as literals.
import { describe, expect, it } from "vitest"
import spec from "@exp/domain-contract/fixtures/getting-started.json"
import {
  GETTING_STARTED_DOCS,
  GETTING_STARTED_STEP_TITLES,
  GETTING_STARTED_TITLE,
} from "@/components/getting-started/getting-started-copy"

const STRINGS = [
  spec.title,
  spec.progress,
  spec.docs.label,
  ...spec.steps.map((step) => step.title),
]

describe(`getting-started copy`, () => {
  it(`is the fixture's: the title, the Docs button, one title per card`, () => {
    expect(GETTING_STARTED_TITLE).toBe(spec.title)
    expect(GETTING_STARTED_DOCS).toEqual(spec.docs)
    expect(GETTING_STARTED_STEP_TITLES).toEqual(
      Object.fromEntries(spec.steps.map((step) => [step.key, step.title]))
    )
  })

  it(`ships eight cards in the agreed order`, () => {
    expect(spec.steps.map((step) => step.key)).toEqual([
      `board`,
      `github`,
      `device`,
      `run`,
      `invite`,
      `server`,
      `mcp`,
      `widget`,
    ])
  })

  it(`keeps every string quotable in Swift, Kotlin and Rust source`, () => {
    for (const value of STRINGS) {
      expect(value).not.toContain(`"`)
      expect(value).not.toContain(`\\`)
      // eslint-disable-next-line no-control-regex
      expect(/^[\x20-\x7e]+$/.test(value) ? value : `non-ASCII: ${value}`).toBe(
        value
      )
    }
  })

  it(`hides only the two web-managed steps from the phones`, () => {
    expect(
      spec.steps.filter((step) => !step.phones).map((step) => step.key)
    ).toEqual([`mcp`, `widget`])
  })
})
