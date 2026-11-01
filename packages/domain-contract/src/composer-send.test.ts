import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/composer-send.json" with { type: "json" }
import { composerAfterSend, composerTextAfterSend, type ComposerSendInput } from "./composer-send"

describe(`composerAfterSend`, () => {
  test(`the fixture has cases`, () => {
    expect(fixture.cases.length).toBeGreaterThan(0)
  })

  for (const entry of fixture.cases) {
    test(entry.name, () => {
      const input = entry.input as ComposerSendInput
      expect(composerAfterSend(input)).toEqual(entry.expected)
      if (input.outcome === `sent`) {
        expect(composerTextAfterSend(input.sent.text, input.current.text)).toBe(entry.expected.text)
      }
    })
  }
})
