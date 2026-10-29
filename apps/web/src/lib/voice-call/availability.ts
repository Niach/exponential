// NEW-70 (voice V6): the telephone's availability as web reads it
// (`fixtures/voice-call.json` `callAvailability`). The inputs are the
// member's voice settings (`voice.get`) and the voice device's SYNCED
// `devices` row: online by `lastSeenAt` freshness, its doctor row `speech`.
// The server's `voice.startCall` reads the same row through the same
// `callSpeechOf`, so the telephone and the refusal never disagree.

import { callAvailability, type CallAvailability, type CallAvailabilityInput, type CallSpeech } from "@exp/etalk/voice-call"

import type { DeviceDoctor } from "@/db/schema"
import { deviceRowIsOnline } from "@/lib/steer-devices"

/** The voice device's doctor row `speech` (group voice) as a caller reads
 *  it: `ok` speaks, `missing` = no usable GPU, anything else (absent, off,
 *  still downloading) = getting its voice ready. */
export function callSpeechOf(doctor: DeviceDoctor | null | undefined): CallSpeech | null {
  const row = doctor?.items.find((item) => item.key === `speech` && item.group === `voice`)
  if (!row) return null
  if (row.state === `ok`) return `ok`
  if (row.state === `missing`) return `noGpu`
  return `preparing`
}

export interface CallSettingsLike {
  voiceDeviceId: string | null
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
 *  whose row is gone reads as none picked, as on the server. */
export function webCallInput(
  settings: CallSettingsLike | null,
  device: CallDeviceRowLike | null,
  now: Date
): CallAvailabilityInput {
  const row = settings?.voiceDeviceId && device?.deviceId === settings.voiceDeviceId ? device : null
  return {
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
