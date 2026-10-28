import { describe, expect, it } from "vitest"
import { escapeReporterText } from "@/lib/reporter-text"
import fixture from "@exp/domain-contract/fixtures/reporter-reply.json"
import {
  REPORTER_SUPPORT_LABEL,
  reporterFacingText,
  stripImageEmbeds,
  unescapeReporterText,
} from "./report-text"

describe(`stripImageEmbeds`, () => {
  it(`drops the widget's image paragraphs and keeps the text`, () => {
    expect(
      stripImageEmbeds(
        `Button broken\n\n![Screenshot](/api/attachments/a)\n\n![Image](/api/attachments/b)`
      )
    ).toBe(`Button broken`)
  })

  it(`leaves inline text alone and collapses the gap`, () => {
    expect(
      stripImageEmbeds(`first\n\n![Screenshot](/api/attachments/a)\n\nsecond`)
    ).toBe(`first\n\nsecond`)
    expect(stripImageEmbeds(``)).toBe(``)
  })
})

describe(`unescapeReporterText`, () => {
  it(`round-trips the server escaper`, () => {
    for (const text of [
      `# not a heading`,
      `ping @bob@x.io re #EXP-1 <b>`,
      `- item\n1. one\n= setext`,
      `a \\ backslash and *stars* and _under_`,
      `plain words.`,
    ]) {
      expect(unescapeReporterText(escapeReporterText(text))).toBe(
        text.replace(/^(?:\t| {4,})[ \t]*/gm, ``)
      )
    }
  })
})

// NEW-85: the reporter-reply.json `reporterView` cases (web only: the
// reporter view is server-built, no native client renders it).
describe(`reporterFacingText`, () => {
  it.each(fixture.reporterView.cases.map((c) => [c.name, c] as const))(
    `%s`,
    (_name, c) => {
      expect(reporterFacingText(c.input, c.reporterEmail)).toBe(c.output)
    }
  )

  it(`labels team replies with the fixture's Support label`, () => {
    expect(REPORTER_SUPPORT_LABEL).toBe(`Support`)
  })
})
