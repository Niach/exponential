import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/meeting.json" with { type: "json" }
import {
  appOptions,
  channels,
  consentBody,
  desktopAudioOptions,
  FLUSH_START,
  flushStep,
  meetingMessage,
  who,
  type DesktopAudio,
  type DesktopAudioCapability,
  type FlushEvent,
} from "./meeting.ts"

describe(`meeting.json`, () => {
  for (const c of fixture.desktopAudioOptionCases) {
    it(`desktop audio: ${c.name}`, () => {
      expect(desktopAudioOptions(c.capability as DesktopAudioCapability)).toEqual(c.expect as DesktopAudio[])
    })
  }
  for (const c of fixture.appOptionCases) {
    it(`apps: ${c.name}`, () => {
      expect(appOptions(c.live, c.stored)).toEqual(c.expect)
    })
  }
  for (const c of fixture.whoCases) {
    it(`who: ${c.name}`, () => {
      expect(who(c.desktopAudio as DesktopAudio, c.apps, c.capability as DesktopAudioCapability)).toBe(c.expect)
    })
  }
  for (const c of fixture.consentCases) {
    it(`consent: ${c.name}`, () => {
      expect(consentBody(c.who, c.agent, c.days)).toBe(c.expect)
    })
  }
  for (const c of fixture.channelCases) {
    it(`channels: ${c.name}`, () => {
      expect(
        channels(c.desktopAudio as DesktopAudio, c.apps, c.capability as DesktopAudioCapability, c.playing)
      ).toEqual(c.expect)
    })
  }
  for (const c of fixture.flushCases) {
    it(`flush: ${c.name}`, () => {
      let state = FLUSH_START
      for (const step of c.steps) {
        const next = flushStep(state, step.event as FlushEvent, step.at)
        expect(next.send).toBe(step.send)
        state = next.state
      }
    })
  }
  for (const c of fixture.messageCases) {
    it(`message: ${c.name}`, () => {
      expect(meetingMessage(c.segments)).toBe(c.expect)
    })
  }
})
