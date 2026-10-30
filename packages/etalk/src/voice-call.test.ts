// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/voice-call.json" with { type: "json" }
import {
  CALL_IDLE,
  callAnswerMs,
  callAvailability,
  callSpeechOf,
  callRingMs,
  callRingToneOn,
  callStep,
  callTimer,
  callView,
  type CallPlatform,
  type CallAvailabilityInput,
  type CallEvent,
} from "./voice-call"

describe(`callSpeechOf`, () => {
  for (const entry of fixture.speechCases) {
    test(entry.name, () => {
      expect(callSpeechOf(entry.state)).toBe(entry.expected as ReturnType<typeof callSpeechOf>)
    })
  }
})

describe(`callAvailability`, () => {
  for (const entry of fixture.availabilityCases) {
    test(entry.name, () => {
      expect(callAvailability(entry.input as CallAvailabilityInput)).toEqual(entry.expected)
    })
  }
})

describe(`callStep + callView`, () => {
  for (const entry of fixture.flowCases) {
    test(entry.name, () => {
      const state = (entry.events as CallEvent[]).reduce(callStep, CALL_IDLE)
      expect(callView(state, entry.platform as CallPlatform)).toEqual(entry.view as never)
      expect(state.toast).toBe((entry as { toast?: string }).toast ?? null)
    })
  }
})

describe(`callTimer`, () => {
  for (const entry of fixture.timerCases) {
    test(`${entry.ms} ms`, () => {
      expect(callTimer(entry.ms)).toBe(entry.text)
    })
  }
})

describe(`EXP-1423: the ring`, () => {
  for (const entry of fixture.ringCases) {
    test(`callRingMs(${entry.random})`, () => {
      expect(callRingMs(entry.random)).toBe(entry.ringMs)
    })
  }
  for (const entry of fixture.answerCases) {
    test(entry.name, () => {
      expect(callAnswerMs(entry.random, entry.readyMs)).toBe(entry.answeredMs)
    })
  }
  for (const entry of fixture.ringToneCases) {
    test(`tone at ${entry.ms} ms`, () => {
      expect(callRingToneOn(entry.ms)).toBe(entry.on)
    })
  }
})
