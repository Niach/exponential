// NEW-70 (voice V6): the telephone's availability as web reads it
// (`fixtures/voice-call.json` `callAvailability`). The inputs are the
// member's voice settings (`voice.get`) and the voice device's SYNCED
// `devices` row: online by `lastSeenAt` freshness, its doctor row `speech`.
// The server's `voice.startCall` reads the same row through the same
// `callSpeechOf`, so the telephone and the refusal never disagree.

import {
  callAvailability,
  callSpeechOf as speechOfState,
  type CallAvailability,
  type CallAvailabilityInput,
  type CallSpeech,
} from "@exp/etalk/voice-call"

import type { DeviceDoctor } from "@/db/schema"
import { deviceRowIsOnline } from "@/lib/steer-devices"

/** The voice device's doctor row `speech` (group voice) as a caller reads
 *  it (voice-call.json `speechCases`; no row = null). */
export function callSpeechOf(doctor: DeviceDoctor | null | undefined): CallSpeech | null {
  const row = doctor?.items.find((item) => item.key === `speech` && item.group === `voice`)
  return speechOfState(row?.state ?? null)
}

export interface CallSettingsLike {
  voiceDeviceId: string | null
  /** NEW-99: the member's voice Assistant (null = none yet). */
  agentId: string | null
  /** The member's voice language (the call's greeting). */
  language?: string
  /** #NEW-57: a meeting runs on the voice device (the telephone is off). */
  meetingStartedAt?: string | null
}

export interface CallDeviceRowLike {
  deviceId: string
  label: string
  lastSeenAt: Date | string
  doctor?: DeviceDoctor | null
}

/** The availability input of a web caller: never the voice device itself
 *  (`here` false), and `busy` is the relay's to say on dial. A voice device
 *  whose row is gone reads as none picked, as on the server; settings not
 *  loaded yet read as no Assistant. */
export function webCallInput(
  settings: CallSettingsLike | null,
  device: CallDeviceRowLike | null,
  now: Date
): CallAvailabilityInput {
  const row = settings?.voiceDeviceId && device?.deviceId === settings.voiceDeviceId ? device : null
  return {
    agent: Boolean(settings?.agentId),
    platform: `web`,
    device: row ? row.label : null,
    online: row ? deviceRowIsOnline(row.lastSeenAt, now) : false,
    here: false,
    speech: row ? callSpeechOf(row.doctor) : null,
    busy: false,
    meeting: Boolean(settings?.meetingStartedAt),
  }
}

export function webCallAvailability(
  settings: CallSettingsLike | null,
  device: CallDeviceRowLike | null,
  now: Date
): CallAvailability {
  return callAvailability(webCallInput(settings, device, now))
}
