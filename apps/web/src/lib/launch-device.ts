// EXP-836: WHICH machine a launch surface has selected. One pure precedence
// rule instead of the effect that used to settle it:
//
//   1. an EXPLICIT request (a play button's `?device=`, `LaunchSeed.deviceId`)
//      the moment that machine is a candidate — it outranks everything, and
//      keeps outranking it while the devices shape is still hydrating, so a
//      late-arriving row still wins over the default;
//   2. the person's own pick in the select;
//   3. their default machine (EXP-622 `isDefault`);
//   4. the first candidate.
//
// A pick CLEARS the request (`useLaunchOptions.setDeviceId`), which is what
// makes the request one-shot: it never reasserts itself over a human choice,
// and it is never persisted as the default either — only `devices.setDefault`
// writes that flag.

// #FEED-204 (apps.json EDITOR DEVICE): the device picked in an app editor's
// composer, per app on this browser. A pick only; never the default machine.
const editorDeviceKey = (appId: string) => `exp.apps.editorDevice.${appId}`

export function loadEditorDevice(appId: string): string | null {
  if (typeof localStorage === `undefined`) return null
  try {
    return localStorage.getItem(editorDeviceKey(appId))
  } catch {
    return null
  }
}

export function saveEditorDevice(appId: string, deviceId: string): void {
  if (typeof localStorage === `undefined`) return
  try {
    localStorage.setItem(editorDeviceKey(appId), deviceId)
  } catch {
    // A full or blocked storage only forgets the pick.
  }
}

export interface LaunchDeviceCandidate {
  deviceId: string
  isDefault?: boolean
}

/** The default machine among `devices`, or null when none is flagged. */
export function defaultCandidateId(
  devices: readonly LaunchDeviceCandidate[]
): string | null {
  return devices.find((device) => device.isDefault === true)?.deviceId ?? null
}

export function resolveLaunchDeviceId(
  devices: readonly LaunchDeviceCandidate[],
  {
    requested,
    picked,
  }: { requested?: string | null; picked?: string | null } = {}
): string | null {
  const known = (id: string | null | undefined) =>
    Boolean(id) && devices.some((device) => device.deviceId === id)
  if (known(requested)) return requested!
  if (known(picked)) return picked!
  return defaultCandidateId(devices) ?? devices[0]?.deviceId ?? null
}
