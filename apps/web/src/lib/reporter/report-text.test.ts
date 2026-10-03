import { describe, expect, it } from "vitest"
import { escapeReporterText } from "@/lib/reporter-text"
import { stripImageEmbeds, unescapeReporterText } from "./report-text"

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
