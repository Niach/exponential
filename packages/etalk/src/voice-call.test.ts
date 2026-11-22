// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/voice-call.json" with { type: "json" }
import {
  callDuration,
  callOpening,
  callSessionPick,
  callSessionText,
  CALL_IDLE,
  CALL_SEAL_IDLE,
  callSealStep,
  callSealWireCode,
  type CallSealEvent,
  type CallSealRole,
  callAnswerMs,
  callAvailability,
  callSpeechOf,
  callRingMs,
  callPathDead,
  callRingToneOn,
  callStep,
  callTap,
  callTarget,
  callTimer,
  callView,
  type CallPlatform,
  type CallAvailabilityInput,
  type CallTargetInput,
  type CallEvent,
  CALLER_IDLE,
  callerFrame,
  callerStep,
  type CallerEffect,
  type CallerInput,
  type CallerState,
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

// EXP-1991: no reason is copy nothing can show. Each one is the reason
// some `targetCases` row answers (`{device}` = any name).
test(`every reason is one callTarget answers`, () => {
  const shown = fixture.targetCases.map((entry) => entry.expected.reason).filter((reason) => reason !== null)
  const answered = (template: string) => {
    const [before, after] = template.split(`{device}`)
    return shown.some((reason) =>
      after === undefined
        ? reason === template
        : reason.length > before!.length + after.length && reason.startsWith(before!) && reason.endsWith(after)
    )
  }
  expect(Object.entries(fixture.copy.reasons).filter(([, template]) => !answered(template)).map(([key]) => key)).toEqual([])
})

describe(`EXP-1878: callTap`, () => {
  for (const entry of fixture.tapCases) {
    test(entry.name, () => {
      expect(callTap(entry.input.platform as CallPlatform, entry.input.availability)).toEqual(entry.expected)
    })
  }
})

describe(`EXP-1857: callTarget`, () => {
  for (const entry of fixture.targetCases) {
    test(entry.name, () => {
      expect(callTarget(entry.input as CallTargetInput)).toEqual(entry.expected)
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

describe(`livenessCases`, () => {
  for (const entry of fixture.livenessCases) {
    test(entry.name, () => {
      expect(callPathDead(entry.input.alive, entry.input.silentMs)).toBe(entry.expected.dead)
    })
  }
})

describe(`EXP-1941: callSealStep (seal.modeCases)`, () => {
  for (const entry of fixture.seal.modeCases) {
    test(entry.name, () => {
      let state = CALL_SEAL_IDLE
      const actions = (entry.events as CallSealEvent[]).map((event) => {
        const stepped = callSealStep(entry.role as CallSealRole, state, event)
        state = stepped.state
        return stepped.actions
      })
      expect(actions).toEqual(entry.actions)
      expect(state.phase).toBe(entry.phase as typeof state.phase)
    })
  }

  test(`maxOpenFailures frames in a row that do not open end a sealed call, on both ends`, () => {
    const { maxOpenFailures } = fixture.seal
    for (const role of [`caller`, `host`] as const) {
      let state = { phase: `sealed` as const, failures: 0 }
      for (let n = 1; n < maxOpenFailures; n++) state = callSealStep(role, state, { t: `frame`, opens: false }).state as typeof state
      expect(state).toEqual({ phase: `sealed`, failures: maxOpenFailures - 1 })
      const last = callSealStep(role, state, { t: `frame`, opens: false })
      expect(last.state.phase).toBe(`over`)
      expect(last.actions).toEqual(role === `caller` ? [`end:seal`, `fail:dropped`, `abandon`] : [`end:seal`, `abandon`])
    }
  })

  test(`every own-device code has copy today and reads dropped from the wire`, () => {
    for (const code of fixture.seal.ownDeviceCodes) {
      expect(Object.keys(fixture.copy.failed)).toContain(code)
      expect(callSealWireCode(code)).toBe(`dropped`)
    }
    expect(callSealWireCode(`busy`)).toBe(`busy`)
  })
})

// #EXP-1993 / #EXP-2017: every `caller.cases` row, as iOS
// (`VoiceCallCallerTests`) and Android (`VoiceCallCallerTest`) replay it:
// per event the effects in order, then the final stage and phase.
describe(`#EXP-2017 callerStep (caller.cases)`, () => {
  test(`every row is replayed`, () => {
    expect(fixture.caller.cases.length).toBeGreaterThanOrEqual(31)
  })
  for (const entry of fixture.caller.cases) {
    test(entry.name, () => {
      expect(entry.events.length).toBe(entry.effects.length)
      let state = CALLER_IDLE
      const effects = (entry.events as CallerInput[]).map((event) => {
        const stepped = callerStep(state, event)
        state = stepped.state
        return stepped.effects
      })
      expect(effects).toEqual(entry.effects as CallerEffect[][])
      const row = entry as { stage: string; phase: string; device?: string; fallback?: boolean; muted?: boolean; speaker?: boolean }
      expect(state.stage).toBe(row.stage as typeof state.stage)
      expect(state.call.phase).toBe(row.phase as typeof state.call.phase)
      if (row.device !== undefined) expect(state.call.device).toBe(row.device)
      if (row.fallback !== undefined) expect(state.call.fallback).toBe(row.fallback)
      if (row.muted !== undefined) expect(state.call.muted).toBe(row.muted)
      if (row.speaker !== undefined) expect(state.call.speaker).toBe(row.speaker)
    })
  }

  test(`what goes on the socket is the effect's frame as JSON; a reason-less call_end carries no reason key`, () => {
    const end = callerStep({ ...CALLER_IDLE, stage: `paired`, call: { ...CALL_IDLE, phase: `listening`, device: `mint` } }, { t: `end` })
    expect(end.effects[0]).toEqual({ do: `send`, frame: { t: `call_end` } })
    expect(JSON.stringify((end.effects[0] as { frame: unknown }).frame)).toBe(`{"t":"call_end"}`)
  })

  test(`a frame that is no JSON object, or a host event this build does not know, is nothing`, () => {
    const paired: CallerState = { ...CALLER_IDLE, stage: `paired`, call: { ...CALL_IDLE, phase: `listening`, device: `mint` } }
    for (const frame of [null, `call_end`, [], { t: `call_event` }, { t: `call_event`, event: { t: `said` } }, { t: `call_event`, event: { t: `newThing` } }]) {
      expect(callerStep(paired, { t: `frame`, frame })).toEqual({ state: paired, effects: [] })
    }
    expect(callerFrame({ t: `call_event`, event: { t: `failed` } })).toEqual({ t: `event`, event: { t: `failed`, code: `` } })
    expect(callerStep(paired, { t: `frame`, frame: { t: `call_event`, event: { t: `failed` } } }).effects).toEqual([
      { do: `release` },
      { do: `toast`, text: fixture.copy.failed.dropped },
    ])
  })
})

describe(`EXP-1961 callSession`, () => {
  for (const entry of fixture.callSession.pickCases) {
    test(`pick: ${entry.name}`, () => {
      expect(callSessionPick(entry.prev, entry.nowMs)).toBe(entry.pick as never)
    })
  }
  for (const entry of fixture.callSession.openingCases) {
    test(`opening: ${entry.name}`, () => {
      expect(callOpening(entry.readyAtMs, entry.openingAtMs, entry.nowMs)).toBe(entry.opening as never)
    })
  }
  for (const entry of fixture.callSession.durationCases) {
    test(`duration: ${entry.name}`, () => {
      expect(callDuration(entry.durationMs, entry.segmentStartedAtMs, entry.endedAtMs)).toBe(entry.expected)
    })
  }
  test(`the request names the agent's language, English when unset`, () => {
    expect(callSessionText(`in {language}`, `de`)).toBe(`in German`)
    expect(callSessionText(`in {language}`, null)).toBe(`in English`)
  })
})
