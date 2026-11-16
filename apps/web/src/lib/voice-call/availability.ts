// NEW-70 (voice V6): the telephone's availability as web reads it.
// EXP-1857 (`fixtures/voice-call.json` `callTarget`): a call rings the
// called agent's desktop (EXP-1858: or the member's own server it runs on),
// else falls back to the member's newest desktop
// that speaks. The inputs are the member's voice settings (`voice.get`, the
// running meeting), the agent's Runs on device and the member's SYNCED
// desktop `devices` rows: online by `lastSeenAt` freshness, their doctor row
// `speech`. The server's `voice.startCall` reads the same rows through the
// same `callSpeechOf` and `callTarget`, so the telephone and the refusal
// never disagree.

import {
  callSpeechOf as speechOfState,
  callTarget,
  type CallSpeech,
  type CallTarget,
  type CallTargetInput,
} from "@exp/etalk/voice-call"

import type { DeviceDoctor } from "@/db/schema"
import { deviceRowIsOnline } from "@/lib/steer-devices"

/** A device's doctor row `speech` (group voice) as a caller reads it
 *  (voice-call.json `speechCases`; no row = null). */
export function callSpeechOf(doctor: DeviceDoctor | null | undefined): CallSpeech | null {
  const row = doctor?.items.find((item) => item.key === `speech` && item.group === `voice`)
  return speechOfState(row?.state ?? null)
}

export interface CallSettingsLike {
  /** The member's Assistant's desktop (where a meeting runs). */
  voiceDeviceId: string | null
  /** The member's voice language (the fallback greeting). */
  language?: string
  /** #NEW-57: a meeting runs on the voice device. */
  meetingStartedAt?: string | null
}

export interface CallDeviceRowLike {
  deviceId: string
  userId?: string
  kind?: string
  label: string
  lastSeenAt: Date | string
  doctor?: DeviceDoctor | null
}

/** The target input of a web caller: never a desktop itself (`here` null),
 *  and `busy` is the relay's to say on dial. Only the member's OWN desktops
 *  count, newest first; settings not loaded yet read as no desktop. */
export function webCallInput(
  settings: CallSettingsLike | null,
  agentDevice: string | null,
  rows: readonly CallDeviceRowLike[],
  userId: string | null,
  now: Date
): CallTargetInput {
  const own = (kind: string) =>
    settings
      ? rows
          .filter((row) => row.kind === kind && row.userId === userId)
          .sort((a, b) => new Date(b.lastSeenAt).getTime() - new Date(a.lastSeenAt).getTime())
          .map((row) => ({
            id: row.deviceId,
            name: row.label,
            online: deviceRowIsOnline(row.lastSeenAt, now),
            speech: callSpeechOf(row.doctor),
            meeting: Boolean(settings.meetingStartedAt) && row.deviceId === settings.voiceDeviceId,
          }))
      : []
  // EXP-1858: the member's own servers (a candidate only as the agent's device).
  return { platform: `web`, here: null, agentDevice, desktops: own(`desktop`), servers: own(`server`), busy: false }
}

export function webCallTarget(
  settings: CallSettingsLike | null,
  agentDevice: string | null,
  rows: readonly CallDeviceRowLike[],
  userId: string | null,
  now: Date
): CallTarget {
  return callTarget(webCallInput(settings, agentDevice, rows, userId, now))
}
