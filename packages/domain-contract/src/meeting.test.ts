import { describe, expect, it } from "bun:test"

import fixture from "../fixtures/meeting.json" with { type: "json" }
import {
  appOptions,
  autoStopped,
  channels,
  consentBody,
  desktopAudioOptions,
  FLUSH_START,
  flushStep,
  meetingMessage,
  meetingExpired,
  meetingMessages,
  MEETING_FLUSH,
  who,
  type DesktopAudio,
  type DesktopAudioCapability,
  type FlushEvent,
} from "./meeting.ts"

describe(`meeting.json`, () => {
  for (const c of fixture.autoStopCases) {
    it(`auto-stop: ${c.name}`, () => expect(meetingExpired(c.startedAt, c.nowMs)).toBe(c.expect))
  }
  for (const c of fixture.autoStoppedCases) {
    it(`auto-stopped ${c.afterMs}`, () => expect(autoStopped(c.afterMs)).toBe(c.expect))
  }
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
  for (const c of fixture.splitCases) {
    it(`split: ${c.name}`, () => {
      const got = meetingMessages(c.segments, c.maxChars)
      expect(got).toEqual(c.expect)
      for (const message of got) expect(message.length).toBeLessThanOrEqual(c.maxChars)
    })
  }
  it(`split: the default is the server's sessions.send text limit`, () => {
    expect(MEETING_FLUSH.maxMessageChars).toBe(16384)
    const long = Array.from({ length: 400 }, (_, i) => ({ channel: `you`, label: `You`, text: `word `.repeat(20), startMs: 1793181723000 + i }))
    const got = meetingMessages(long)
    expect(got.length).toBeGreaterThan(1)
    for (const message of got) expect(message.length).toBeLessThanOrEqual(16384)
  })
})
