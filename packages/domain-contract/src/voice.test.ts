import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/voice.json" with { type: "json" }
import {
  agentOptions,
  clearOnNone,
  identifierNumberWords,
  languageLabel,
  languageNote,
  numberWords,
  shouldSpeak,
  spokenText,
  trayHeader,
  trayMenu,
  voiceDeviceOptions,
  wakePhrasePlaceholder,
} from "./voice"

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
  for (const c of fixture.agentOptionsCases) {
    it(`agentOptions: ${c.name}`, () => {
      expect(agentOptions(c.agents)).toEqual(c.options)
    })
  }
  for (const c of fixture.voiceDeviceOptionsCases) {
    it(`voiceDeviceOptions: ${c.name}`, () => {
      expect(voiceDeviceOptions(c.devices)).toEqual(c.options)
    })
  }
  for (const c of fixture.clearOnNoneCases) {
    it(`clearOnNone: ${c.name}`, () => {
      expect(clearOnNone(c.before as never, c.change as never)).toEqual(c.after)
    })
  }
  for (const c of fixture.trayHeaderCases) {
    it(`trayHeader ${c.device}`, () => expect(trayHeader(c.device)).toBe(c.header))
  }
  for (const c of fixture.languageNoteCases) {
    it(`languageNote: ${c.name}`, () => expect(languageNote(c.language, c.device)).toBe(c.note))
  }
  for (const c of fixture.languageLabelCases) {
    it(`languageLabel: ${c.name}`, () => expect(languageLabel(c.language, c.device)).toBe(c.label))
  }
  for (const c of fixture.wakePhrasePlaceholderCases) {
    it(`wakePhrasePlaceholder ${c.agent}`, () => expect(wakePhrasePlaceholder(c.agent)).toBe(c.placeholder))
  }
  for (const c of fixture.trayMenuCases) {
    it(`trayMenu: ${c.name}`, () => expect(trayMenu(c.settings, c.context)).toEqual(c.menu))
  }
})
