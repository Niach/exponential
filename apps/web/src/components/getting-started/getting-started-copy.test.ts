// EXP-1270: Getting started's copy is the fixture's. The natives port the
// same fixture (their lanes replay its `cases` and read its strings), so the
// gate that used to grep three native copy tables now pins the web table to
// the fixture, and the fixture's strings to what Swift, Kotlin and Rust can
// quote as literals.
import { describe, expect, it } from "vitest"
import spec from "@exp/domain-contract/fixtures/getting-started.json"
import {
  GETTING_STARTED_DOCS,
  GETTING_STARTED_MAX_WIDTH,
  GETTING_STARTED_NEXT_LABEL,
  GETTING_STARTED_STEP_CTA,
  GETTING_STARTED_STEP_TITLES,
  GETTING_STARTED_STEP_WHY,
  GETTING_STARTED_TITLE,
} from "@/components/getting-started/getting-started-copy"

const STRINGS = [
  spec.title,
  spec.progress,
  spec.docs.label,
  spec.nextLabel,
  ...spec.steps.flatMap((step) => [step.title, step.why, step.cta]),
]

const byStep = (field: `title` | `why` | `cta`) =>
  Object.fromEntries(spec.steps.map((step) => [step.key, step[field]]))

describe(`getting-started copy`, () => {
  it(`is the fixture's: the title, Docs, Next, per step a title, a why and a cta`, () => {
    expect(GETTING_STARTED_TITLE).toBe(spec.title)
    expect(GETTING_STARTED_DOCS).toEqual(spec.docs)
    expect(GETTING_STARTED_NEXT_LABEL).toBe(spec.nextLabel)
    expect(GETTING_STARTED_STEP_TITLES).toEqual(byStep(`title`))
    expect(GETTING_STARTED_STEP_WHY).toEqual(byStep(`why`))
    expect(GETTING_STARTED_STEP_CTA).toEqual(byStep(`cta`))
  })

  it(`takes the fixture's geometry`, () => {
    expect(GETTING_STARTED_MAX_WIDTH).toBe(spec.maxWidth)
    expect(spec.row.heightPhone).toBeGreaterThanOrEqual(spec.row.height)
  })

  it(`ships eight steps in the agreed order`, () => {
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

  it(`gives every step ONE sentence of why and a button label`, () => {
    for (const step of spec.steps) {
      expect(step.why.trim()).toBe(step.why)
      expect(step.why.endsWith(`.`)).toBe(true)
      expect(step.cta.length).toBeGreaterThan(0)
    }
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
