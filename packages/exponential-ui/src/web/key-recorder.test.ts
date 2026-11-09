import { describe, expect, test } from "bun:test"
import { keyRecorderCode, keyRecorderKey, keyRecorderKeyPayload, keyRecorderMousePayload } from "./key-recorder"

const none = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }

describe(`#EXP-1629 KeyRecorder DOM capture`, () => {
  test(`the canonical names come from the physical key`, () => {
    expect(keyRecorderKey(`KeyK`)).toBe(`k`)
    expect(keyRecorderKey(`Digit1`)).toBe(`1`)
    expect(keyRecorderKey(`F13`)).toBe(`f13`)
    expect(keyRecorderKey(`ArrowUp`)).toBe(`up`)
    expect(keyRecorderKey(`Backslash`)).toBe(`\\`)
    expect(keyRecorderKey(`IntlRo`)).toBeUndefined()
    for (const key of [`k`, `1`, `f24`, `space`, `escape`, `pagedown`, `\``, `\\`]) expect(keyRecorderKey(keyRecorderCode(key)!)).toBe(key)
  })
  test(`a shifted character still reads its key`, () => {
    expect(keyRecorderKeyPayload({ ...none, shiftKey: true, type: `keydown`, code: `Digit1` })).toEqual({ type: `down`, key: `1`, ctrl: false, alt: false, shift: true, meta: false })
  })
  test(`a modifier alone is a modifiers change; a repeat sends nothing`, () => {
    expect(keyRecorderKeyPayload({ ...none, ctrlKey: true, type: `keydown`, code: `ControlLeft` })).toEqual({ type: `modifiers`, ctrl: true, alt: false, shift: false, meta: false })
    expect(keyRecorderKeyPayload({ ...none, type: `keydown`, code: `KeyK`, repeat: true })).toBeUndefined()
  })
  test(`mouse buttons 3-5 only`, () => {
    expect(keyRecorderMousePayload({ ...none, button: 0 })).toBeUndefined()
    expect(keyRecorderMousePayload({ ...none, button: 2 })).toBeUndefined()
    expect(keyRecorderMousePayload({ ...none, button: 1 })?.button).toBe(3)
    expect(keyRecorderMousePayload({ ...none, button: 3 })?.button).toBe(4)
    expect(keyRecorderMousePayload({ ...none, button: 4 })?.button).toBe(5)
  })
})
