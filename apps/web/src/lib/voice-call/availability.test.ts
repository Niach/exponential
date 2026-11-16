import { describe, expect, it } from "vitest"
import fixture from "@exp/etalk/fixtures/voice-call.json"
import type { CallSpeech, CallTargetInput } from "@exp/etalk/voice-call"

import type { DeviceDoctor } from "@/db/schema"
import { callSpeechOf, webCallInput, webCallTarget, type CallDeviceRowLike } from "./availability"

// NEW-70 / EXP-1857: the web telephone replays voice-call.json `targetCases`
// through what web actually reads (the member's voice settings, the agent's
// device and the member's synced desktop rows), never a hand-built input.

const NOW = new Date(`2026-10-27T12:00:00Z`)
const STALE = new Date(NOW.getTime() - 10 * 60_000)

/** The doctor's `speech` row a caller's input stands for. */
function doctorFor(speech: CallSpeech | null): DeviceDoctor {
  const state = { ok: `ok`, noGpu: `missing`, preparing: `action`, failed: `error` } as const
  return {
    checkedAt: NOW.toISOString(),
    items: speech === null ? [] : [{ key: `speech`, group: `voice`, state: state[speech] }],
  } as DeviceDoctor
}

function rowsFor(input: CallTargetInput) {
  const meetingOn = input.desktops.find((desktop) => desktop.meeting)
  const settings = { voiceDeviceId: meetingOn?.id ?? null, meetingStartedAt: meetingOn ? NOW.toISOString() : null }
  const rowOf = (kind: string) => (device: CallTargetInput[`desktops`][number], index: number): CallDeviceRowLike => ({
    deviceId: device.id,
    userId: `me`,
    kind,
    label: device.name,
    // Newest first: a few seconds apart, stale when offline.
    lastSeenAt: device.online ? new Date(NOW.getTime() - 1_000 * (index + 1)) : new Date(STALE.getTime() - index),
    doctor: doctorFor(device.speech),
  })
  // EXP-1858: the member's own servers are rows of kind `server`.
  const rows: CallDeviceRowLike[] = [...input.desktops.map(rowOf(`desktop`)), ...(input.servers ?? []).map(rowOf(`server`))]
  // Another member's desktop and server, and a spare server of mine that
  // runs no called agent, never count.
  rows.push({ deviceId: `d-theirs`, userId: `them`, kind: `desktop`, label: `theirs`, lastSeenAt: NOW, doctor: doctorFor(`ok`) })
  rows.push({ deviceId: `d-server`, userId: `them`, kind: `server`, label: `theirs`, lastSeenAt: NOW, doctor: doctorFor(`ok`) })
  rows.push({ deviceId: `d-spare`, userId: `me`, kind: `server`, label: `spare`, lastSeenAt: NOW, doctor: doctorFor(`ok`) })
  return { settings, rows }
}

// Web is never a desktop itself, and `busy` is the relay's to say on dial.
const WEB_CASES = fixture.targetCases.filter((c) => c.input.platform !== `desktop` && !c.input.busy)

describe(`web call target (voice-call.json targetCases)`, () => {
  it.each(WEB_CASES.map((c) => [c.name, c] as const))(`%s`, (_name, c) => {
    const input = c.input as CallTargetInput
    const { settings, rows } = rowsFor(input)
    const read = webCallInput(settings, input.agentDevice, rows, `me`, NOW)
    expect(read.desktops.map((desktop) => desktop.id)).toEqual(input.desktops.map((desktop) => desktop.id))
    expect(webCallTarget(settings, input.agentDevice, rows, `me`, NOW)).toEqual(c.expected)
  })

  it(`settings not loaded yet keep the telephone off`, () => {
    expect(webCallTarget(null, `d-mint`, [], `me`, NOW).enabled).toBe(false)
  })

  it.each(fixture.speechCases.map((c) => [c.name, c] as const))(`speechCases: %s`, (_name, c) => {
    const doctor = { checkedAt: ``, items: c.state === null ? [] : [{ key: `speech`, group: `voice`, state: c.state }] }
    expect(callSpeechOf(doctor as DeviceDoctor)).toBe(c.expected)
  })

  it(`the doctor row: only the voice group's speech row counts`, () => {
    expect(callSpeechOf(null)).toBeNull()
    expect(callSpeechOf(doctorFor(null))).toBeNull()
    // Another group's `speech` row is not the voice's.
    expect(callSpeechOf({ checkedAt: ``, items: [{ key: `speech`, group: `required`, state: `ok` }] } as DeviceDoctor)).toBeNull()
  })
})
