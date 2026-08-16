import { describe, expect, it } from "vitest"
import { buildSteerImageMessage, MAX_STEER_IMAGES } from "./steer-image-message"

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
