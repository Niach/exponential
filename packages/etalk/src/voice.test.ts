import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/voice.json" with { type: "json" }
import {
  agentOptions,
  clearOnNone,
  downloadProgress,
  identifierNumberWords,
  languageLabel,
  languageNote,
  numberWords,
  PTT_HIDDEN,
  pttOverlay,
  pttStep,
  captureStart,
  captureStep,
  capturePending,
  shortcutCanonical,
  shortcutLabel,
  shortcutReason,
  shortcutSupport,
  shouldSpeak,
  speechChunks,
  spokenText,
  trayHeader,
  wakeDetail,
  railVoice,
  trayIcon,
  trayMenu,
  utteranceInput,
  voiceDeviceOptions,
  wakePhrasePlaceholder,
  voiceTextInput,
  wakePhraseError,
} from "./voice"

describe(`voice.json`, () => {
  for (const c of fixture.shouldSpeakCases) {
    it(`shouldSpeak: ${c.name}`, () => {
      expect(shouldSpeak(c.message, c.context)).toBe(c.speak)
    })
  }
  for (const c of fixture.spokenTextCases) {
    it(`spokenText: ${c.name}`, () => {
      expect(spokenText(c.body, c.maxChars ?? fixture.maxChars, c.language ?? `en`)).toBe(c.spoken)
    })
  }
  for (const c of fixture.numberWordsCases) {
    it(`numberWords ${c.language ?? `en`} ${c.n}`, () => {
      expect(numberWords(c.n, c.language ?? `en`)).toBe(c.words)
    })
  }
  for (const c of fixture.identifierNumberCases) {
    it(`identifierNumberWords ${c.language ?? `en`} ${c.n}`, () => {
      expect(identifierNumberWords(c.n, c.language ?? `en`)).toBe(c.words)
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
  for (const c of fixture.downloadProgressCases) {
    it(`downloadProgress: ${c.name}`, () => expect(downloadProgress(c.received, c.total)).toBe(c.text))
  }
  it(`spokenWords: an unknown language reads as English`, () => {
    expect(spokenText(`See https://example.com, 50% done.`, fixture.maxChars, `fr`)).toBe(`See link, fifty percent done.`)
  })
  for (const c of fixture.wakePhrasePlaceholderCases) {
    it(`wakePhrasePlaceholder ${c.agent}`, () => expect(wakePhrasePlaceholder(c.agent)).toBe(c.placeholder))
  }
  for (const c of fixture.wakePhraseErrorCases) {
    it(`wakePhraseError ${JSON.stringify(c.in)}`, () => expect(wakePhraseError(c.in)).toBe(c.error))
  }
  for (const c of fixture.voiceTextInputCases) {
    it(`voiceTextInput ${JSON.stringify(c.in)} ≤ ${c.max}`, () => expect(voiceTextInput(c.in, c.max)).toBe(c.out))
  }
  it(`voiceTextInput: the cap is zod's, the server's limits`, () => {
    for (const c of fixture.voiceTextInputCases) expect(voiceTextInput(c.in, c.max).length).toBeLessThanOrEqual(c.max)
  })
  for (const c of fixture.trayMenuCases) {
    it(`trayMenu: ${c.name}`, () => expect(trayMenu(c.settings, c.context)).toEqual(c.menu))
  }
  for (const c of fixture.trayIconCases) {
    it(`trayIcon: ${c.name}`, () => expect(trayIcon(c.context)).toBe(c.icon as `app` | `listening` | `recording` | null))
  }
  for (const c of fixture.wakeDetailCases) {
    it(`wakeDetail: ${c.name}`, () => expect(wakeDetail(c.context as Parameters<typeof wakeDetail>[0])).toBe(c.detail))
  }
  for (const c of fixture.railVoiceCases) {
    it(`railVoice: ${c.name}`, () => expect(railVoice(c.settings, c.context)).toEqual(c.rail))
  }
  for (const c of fixture.shortcutSupportCases) {
    it(`shortcutSupport ${c.os} ${c.session} ${c.shortcut} portal=${c.portal}`, () =>
      expect(shortcutSupport(c.os as `linux`, c.session as `x11` | null, c.shortcut, c.portal)).toBe(c.support as `global`))
  }
  for (const c of fixture.shortcutParseCases) {
    it(`shortcutCanonical ${JSON.stringify(c.in)}`, () => {
      expect(shortcutCanonical(c.in)).toBe(c.value)
      if (c.value !== null) expect(shortcutLabel(c.value)).toBe(c.label as string)
    })
  }
  for (const c of fixture.captureCases) {
    it(`captureStep: ${c.name}`, () => {
      const state = (c.events as Parameters<typeof captureStep>[1][]).reduce(captureStep, captureStart(c.before))
      expect({ status: state.status, value: state.value, pending: capturePending(state), error: state.error }).toEqual(c.result)
    })
  }
  for (const c of fixture.shortcutReasonCases) {
    it(`shortcutReason ${c.reason}`, () => expect(shortcutReason(c.reason as `taken`, c.shortcut)).toBe(c.text))
  }
  for (const c of fixture.pttFlowCases) {
    it(`pttFlow: ${c.name}`, () => {
      const state = (c.events as Parameters<typeof pttStep>[1][]).reduce(pttStep, PTT_HIDDEN)
      expect(pttOverlay(state, c.agent)).toEqual(c.overlay as ReturnType<typeof pttOverlay>)
    })
  }
  for (const c of fixture.utteranceCases) {
    it(`utteranceInput: ${c.name}`, () =>
      expect(utteranceInput(c.heard, { teamId: c.teamId, agentId: c.agentId, conversationId: c.conversationId })).toEqual(c.input))
  }
  for (const c of fixture.speechChunkCases) {
    it(`speechChunks: ${c.name}`, () => expect(speechChunks(c.text, c.streaming)).toEqual(c.chunks))
  }
  it(`the overlay specimens read the overlay copy`, () => {
    const ptt = fixture.pttCopy
    const states = [ptt.overlayListening, ptt.overlayHeard, ptt.overlayThinking, fixture.copy.speaking, ptt.overlayMuted]
    for (const specimen of fixture.overlaySpecimens) expect(states).toContain(specimen.state)
  })
})
