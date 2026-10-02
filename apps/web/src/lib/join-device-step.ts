// EXP-1169: what an invited teammate sees right after `teamInvites.accept`.
// The creator's wizard ends on the devices step; a joiner used to get nothing
// and met "No device online" on their first Start. So the SAME step shows
// once after an accept, but only to someone who owns no device: a second-team
// joiner with a machine goes straight in. The natives apply the same rule at
// their accept surfaces.

export type JoinDeviceStep = `wait` | `step` | `enter`

/**
 * `devices` = the caller's OWN devices, null while the synced shape has not
 * landed. A joiner is never trapped on the wait: once `timedOut`, an unknown
 * device list enters the team.
 */
export function joinDeviceStep(
  devices: readonly unknown[] | null,
  timedOut: boolean
): JoinDeviceStep {
  if (devices === null) return timedOut ? `enter` : `wait`
  return devices.length === 0 ? `step` : `enter`
}

/** How long the accept page waits for the devices shape before entering. */
export const JOIN_DEVICE_WAIT_MS = 4_000
