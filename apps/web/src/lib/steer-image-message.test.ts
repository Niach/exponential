import { describe, expect, it } from "vitest"
import {
  buildSteerImageMessage,
  buildSteerMessage,
  escapeSteerFileName,
  MAX_STEER_FILES,
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
      files: [],
      markers: [1],
    })
  })

  it(`reads embeds sent without text`, () => {
    expect(parseSteerMessage(buildSteerImageMessage(``, [A]))).toEqual({
      text: ``,
      attachmentIds: [A],
      files: [],
      markers: [],
    })
  })

  it(`leaves a plain message untouched`, () => {
    expect(parseSteerMessage(`just words`)).toEqual({
      text: `just words`,
      attachmentIds: [],
      files: [],
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

// Wave D: files beside the images. Byte-identical cases in
// image_message.rs (desktop), SteerImageMessageTests.swift (iOS) and
// SteerImageMessageTest.kt (Android): change all four together.
const C = `33333333-3333-4333-8333-333333333333`

describe(`buildSteerMessage`, () => {
  it(`puts one file link per line after the image embeds`, () => {
    expect(
      buildSteerMessage(`fix the header`, [A], [{ id: B, name: `notes.pdf` }])
    ).toBe(
      `fix the header\n\n![image](/api/attachments/${A})\n[notes.pdf](/api/attachments/${B})`
    )
  })

  it(`separates files from the prose by a blank line without images`, () => {
    expect(
      buildSteerMessage(`see the log`, [], [
        { id: B, name: `build.log` },
        { id: C, name: `trace.zip` },
      ])
    ).toBe(
      `see the log\n\n[build.log](/api/attachments/${B})\n[trace.zip](/api/attachments/${C})`
    )
  })

  it(`sends files alone when the text is whitespace`, () => {
    expect(buildSteerMessage(`  `, [], [{ id: B, name: `notes.pdf` }])).toBe(
      `[notes.pdf](/api/attachments/${B})`
    )
  })

  it(`escapes a closing bracket and a backslash in the name`, () => {
    expect(escapeSteerFileName(`a]b\\c.txt`)).toBe(`a\\]b\\\\c.txt`)
    expect(buildSteerMessage(``, [], [{ id: C, name: `a]b\\c.txt` }])).toBe(
      `[a\\]b\\\\c.txt](/api/attachments/${C})`
    )
  })

  it(`equals the image builder without files`, () => {
    expect(buildSteerMessage(`fix`, [A, B], [])).toBe(
      buildSteerImageMessage(`fix`, [A, B])
    )
    expect(buildSteerMessage(``, [], [])).toBe(``)
  })

  it(`caps at four files`, () => {
    expect(MAX_STEER_FILES).toBe(4)
  })
})

describe(`parseSteerMessage with files`, () => {
  it(`peels the file lines before the image embeds`, () => {
    expect(
      parseSteerMessage(
        `fix [Image #1]\n\n![image](/api/attachments/${A})\n[notes.pdf](/api/attachments/${B})`
      )
    ).toEqual({
      text: `fix [Image #1]`,
      attachmentIds: [A],
      files: [{ id: B, name: `notes.pdf` }],
      markers: [1],
    })
  })

  it(`reads files with no images and no text`, () => {
    expect(
      parseSteerMessage(
        `[build.log](/api/attachments/${B})\n[trace.zip](/api/attachments/${C})`
      )
    ).toEqual({
      text: ``,
      attachmentIds: [],
      files: [
        { id: B, name: `build.log` },
        { id: C, name: `trace.zip` },
      ],
      markers: [],
    })
  })

  it(`unescapes the name`, () => {
    expect(
      parseSteerMessage(`[a\\]b\\\\c.txt](/api/attachments/${C})`).files
    ).toEqual([{ id: C, name: `a]b\\c.txt` }])
  })

  it(`never reads an image embed as a file`, () => {
    expect(
      parseSteerMessage(`![image](/api/attachments/${A})`).files
    ).toEqual([])
  })

  it(`round-trips the builder`, () => {
    const files = [
      { id: B, name: `a]b\\c.txt` },
      { id: C, name: `report (final).pdf` },
    ]
    expect(
      parseSteerMessage(buildSteerMessage(`look [Image #1]`, [A], files))
    ).toEqual({
      text: `look [Image #1]`,
      attachmentIds: [A],
      files,
      markers: [1],
    })
  })
})
