import { describe, expect, it } from "vitest"
import fixture from "@exp/etalk/fixtures/voice-call.json"
import type { CallAvailabilityInput } from "@exp/etalk/voice-call"

import type { DeviceDoctor } from "@/db/schema"
import { callSpeechOf, webCallAvailability, webCallInput, type CallDeviceRowLike } from "./availability"

// NEW-70: the web telephone replays voice-call.json `availabilityCases`
// through what web actually reads (the member's voice settings and the voice
// device's synced row), never through a hand-built input.

const NOW = new Date(`2026-10-27T12:00:00Z`)
const FRESH = new Date(NOW.getTime() - 10_000)
const STALE = new Date(NOW.getTime() - 10 * 60_000)

/** The doctor's `speech` row a caller's input stands for. */
function doctorFor(speech: CallAvailabilityInput[`speech`]): DeviceDoctor {
  const state = { ok: `ok`, noGpu: `missing`, preparing: `action` } as const
  return {
    checkedAt: NOW.toISOString(),
    items: speech === null ? [] : [{ key: `speech`, group: `voice`, state: state[speech] }],
  } as DeviceDoctor
}

function rowsFor(input: CallAvailabilityInput) {
  const settings = {
    voiceDeviceId: input.device === null ? null : `dev-1`,
    meetingStartedAt: input.meeting ? NOW.toISOString() : null,
  }
  const device: CallDeviceRowLike | null =
    input.device === null
      ? null
      : { deviceId: `dev-1`, label: input.device, lastSeenAt: input.online ? FRESH : STALE, doctor: doctorFor(input.speech) }
  return { settings, device }
}

// Web never IS the voice device, and `busy` is the relay's to say on dial.
const WEB_CASES = fixture.availabilityCases.filter((c) => c.input.platform !== `desktop` && !c.input.busy && !c.input.here)

describe(`web call availability (voice-call.json availabilityCases)`, () => {
  it.each(WEB_CASES.map((c) => [c.name, c] as const))(`%s`, (_name, c) => {
    const input = c.input as CallAvailabilityInput
    const { settings, device } = rowsFor(input)
    // A phone case reads the same on web: only `desktop` has `notHere`.
    expect(webCallInput(settings, device, NOW)).toEqual({ meeting: false, ...input, platform: `web` })
    expect(webCallAvailability(settings, device, NOW)).toEqual(c.expected)
  })

  it(`covers every reason a web caller can see`, () => {
    for (const key of [`noVoiceDevice`, `offline`, `meeting`, `preparing`, `noGpu`] as const) {
      const hit = WEB_CASES.some((c) => c.expected.reason === fixture.copy.reasons[key].replace(`{device}`, c.input.device ?? ``))
      expect(hit, key).toBe(true)
    }
    expect(WEB_CASES.some((c) => c.expected.enabled)).toBe(true)
  })

  it(`a voice device whose row is gone reads as none picked`, () => {
    const settings = { voiceDeviceId: `dev-1` }
    expect(webCallAvailability(settings, null, NOW).reason).toBe(fixture.copy.reasons.noVoiceDevice)
    const other = { deviceId: `dev-2`, label: `mint`, lastSeenAt: FRESH, doctor: doctorFor(`ok`) }
    expect(webCallAvailability(settings, other, NOW).reason).toBe(fixture.copy.reasons.noVoiceDevice)
  })

  it(`settings not loaded yet keep the telephone off`, () => {
    expect(webCallAvailability(null, null, NOW).enabled).toBe(false)
  })

  it(`the doctor row: ok → ok, missing → noGpu, absent or anything else → preparing`, () => {
    expect(callSpeechOf(null)).toBeNull()
    expect(callSpeechOf(doctorFor(null))).toBeNull()
    expect(callSpeechOf(doctorFor(`ok`))).toBe(`ok`)
    expect(callSpeechOf(doctorFor(`noGpu`))).toBe(`noGpu`)
    expect(callSpeechOf(doctorFor(`preparing`))).toBe(`preparing`)
    // Another group's `speech` row is not the voice's.
    expect(callSpeechOf({ checkedAt: ``, items: [{ key: `speech`, group: `required`, state: `ok` }] } as DeviceDoctor)).toBeNull()
  })
})
