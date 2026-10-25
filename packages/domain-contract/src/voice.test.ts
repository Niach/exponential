import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/voice.json" with { type: "json" }
import { assistantOptions, identifierNumberWords, numberWords, shouldSpeak, spokenText } from "./voice"

describe(`voice.json`, () => {
  for (const c of fixture.shouldSpeakCases) {
    it(`shouldSpeak: ${c.name}`, () => {
      expect(shouldSpeak(c.message, c.context)).toBe(c.speak)
    })
  }
  for (const c of fixture.spokenTextCases) {
    it(`spokenText: ${c.name}`, () => {
      expect(spokenText(c.body, c.maxChars ?? fixture.maxChars)).toBe(c.spoken)
    })
  }
  for (const c of fixture.numberWordsCases) {
    it(`numberWords ${c.n}`, () => {
      expect(numberWords(c.n)).toBe(c.words)
    })
  }
  for (const c of fixture.identifierNumberCases) {
    it(`identifierNumberWords ${c.n}`, () => {
      expect(identifierNumberWords(c.n)).toBe(c.words)
    })
  }
  for (const c of fixture.assistantOptionsCases) {
    it(`assistantOptions: ${c.name}`, () => {
      expect(assistantOptions(c.agents)).toEqual(c.options)
    })
  }
})
