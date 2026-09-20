import { describe, expect, it } from "vitest"
import { shapeParser } from "@/lib/shape-parser"

describe(`shapeParser`, () => {
  it(`parses int8 columns to a number, not a BigInt (EXP-955)`, () => {
    // attachments.size_bytes is an int8; Electric's default would hand back
    // 22345n, which the size formatter renders as "0 B".
    const parsed = shapeParser.int8(`22345`)
    expect(parsed).toBe(22345)
    expect(typeof parsed).toBe(`number`)
    expect(Number.isFinite(parsed)).toBe(true)
  })

  it(`still parses timestamps to Date`, () => {
    expect(shapeParser.timestamptz(`2026-09-20T10:00:00Z`)).toBeInstanceOf(Date)
    expect(shapeParser.timestamp(`2026-09-20 10:00:00`)).toBeInstanceOf(Date)
  })
})
