import { describe, expect, it } from "vitest"
import {
  buildSteerImageMessage,
  imageMarker,
  insertImageMarker,
  MAX_STEER_IMAGES,
  parseSteerMessage,
  renumberImageMarkers,
} from "./steer-image-message"

// Fixtures are mirrored byte-for-byte in SteerImageMessageTests.swift (iOS)
// and SteerImageMessageTest.kt (Android) — change all three together.
const A = `11111111-1111-4111-8111-111111111111`
const B = `22222222-2222-4222-8222-222222222222`

describe(`buildSteerImageMessage`, () => {
  it(`appends embeds after a blank line, one per line`, () => {
    expect(buildSteerImageMessage(`fix the header`, [A, B])).toBe(
      `fix the header\n\n![image](/api/attachments/${A})\n![image](/api/attachments/${B})`
    )
  })

  it(`sends embeds alone when the text is whitespace`, () => {
    expect(buildSteerImageMessage(`  \n `, [A])).toBe(
      `![image](/api/attachments/${A})`
    )
  })

  it(`returns trimmed text unchanged without images`, () => {
    expect(buildSteerImageMessage(`  hello  `, [])).toBe(`hello`)
  })

  it(`returns the empty string for no text and no images`, () => {
    expect(buildSteerImageMessage(``, [])).toBe(``)
  })

  it(`caps at four images`, () => {
    expect(MAX_STEER_IMAGES).toBe(4)
  })
})

// EXP-698 — the positional `[Image #N]` markers. Same fixture rule: the
// natives mirror these cases.
describe(`insertImageMarker`, () => {
  it(`spaces the marker off the text it lands against`, () => {
    expect(insertImageMarker(`crop`, 4, 1)).toEqual({
      text: `crop [Image #1]`,
      caret: 15,
    })
  })

  it(`inserts mid-text with one space on each side`, () => {
    const { text, caret } = insertImageMarker(`crop this`, 5, 2)
    expect(text).toBe(`crop [Image #2] this`)
    // Behind the trailing space, ready for more typing.
    expect(caret).toBe(16)
  })

  it(`adds no space where one is already there`, () => {
    expect(insertImageMarker(`crop `, 5, 1).text).toBe(`crop [Image #1]`)
    expect(insertImageMarker(` this`, 0, 1).text).toBe(`[Image #1] this`)
  })

  it(`stands alone in an empty draft`, () => {
    expect(insertImageMarker(``, 0, 1)).toEqual({
      text: `[Image #1]`,
      caret: 10,
    })
  })

  it(`clamps an out-of-range caret to the end`, () => {
    expect(insertImageMarker(`crop`, 99, 1).text).toBe(`crop [Image #1]`)
  })
})

describe(`renumberImageMarkers`, () => {
  it(`drops the removed marker and slides the higher ones down`, () => {
    expect(
      renumberImageMarkers(`crop [Image #1] and [Image #2] and [Image #3]`, 2)
    ).toBe(`crop [Image #1] and and [Image #2]`)
  })

  it(`tidies the gap the dropped marker left`, () => {
    expect(renumberImageMarkers(`crop [Image #1] please`, 1)).toBe(
      `crop please`
    )
    expect(renumberImageMarkers(`crop [Image #1]`, 1)).toBe(`crop`)
    expect(renumberImageMarkers(`[Image #1] crop`, 1)).toBe(`crop`)
  })

  it(`leaves lower markers and untouched lines alone`, () => {
    expect(
      renumberImageMarkers(`[Image #1]  keep\ncrop [Image #3]`, 2)
    ).toBe(`[Image #1]  keep\ncrop [Image #2]`)
  })

  it(`removes every occurrence of the same marker`, () => {
    expect(renumberImageMarkers(`a [Image #2] b [Image #2] c`, 2)).toBe(
      `a b c`
    )
  })
})

describe(`parseSteerMessage`, () => {
  it(`splits the prose from the trailing embeds`, () => {
    expect(
      parseSteerMessage(buildSteerImageMessage(`fix [Image #1]`, [A, B]))
    ).toEqual({
      text: `fix [Image #1]`,
      attachmentIds: [A, B],
      markers: [1],
    })
  })

  it(`reads embeds sent without text`, () => {
    expect(parseSteerMessage(buildSteerImageMessage(``, [A]))).toEqual({
      text: ``,
      attachmentIds: [A],
      markers: [],
    })
  })

  it(`leaves a plain message untouched`, () => {
    expect(parseSteerMessage(`just words`)).toEqual({
      text: `just words`,
      attachmentIds: [],
      markers: [],
    })
  })

  it(`reports markers in text order, deduped`, () => {
    expect(
      parseSteerMessage(`[Image #2] then [Image #1] then [Image #2]`).markers
    ).toEqual([2, 1])
  })

  it(`builds the marker the pattern matches`, () => {
    expect(imageMarker(3)).toBe(`[Image #3]`)
    expect(parseSteerMessage(imageMarker(3)).markers).toEqual([3])
  })
})
