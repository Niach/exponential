// Member spaces (fixtures/member-space.json): the pure rules, replayed by
// member-space.test.ts and by desktop, iOS and Android.
import fixture from "../fixtures/member-space.json"

export const MEMBER_SPACE = fixture
export const MEMBER_SPACE_COPY = fixture.copy
export type SpaceEndCause = `unshared` | `optedOut` | `memberLeft` | `ownerLeft` | `deviceRemoved` | `picked` | `installRemoved` | `accountDeleted` | `removed`

/** Who may move one team in or out of a device's opt-in. */
export function mayOptIn(input: {
  caller: string
  allowed: boolean
  kind: string
  shared: boolean
  doctor: string | null
}): boolean {
  if (!input.allowed) return input.caller === `deviceOwner` || input.caller === `teamOwner`
  return input.caller === `deviceOwner` && input.kind === `server` && input.shared && input.doctor === `ok`
}

/** The teams a space works in: opted in, the host's owner and the member both in them. */
export function spaceTeams(
  assistantTeamIds: readonly string[],
  hostOwnerTeamIds: readonly string[],
  memberTeamIds: readonly string[]
): string[] {
  return memberTeamIds.filter((id) => assistantTeamIds.includes(id) && hostOwnerTeamIds.includes(id))
}

export const spaceEnds = (spaceTeamId: string, teams: readonly string[]): boolean => !teams.includes(spaceTeamId)

/** A space runs only the trees of its Assistant conversation. */
export const maySpaceRun = (tree: string, rootDeviceIsSpace: boolean): boolean => tree === `assistant` && rootDeviceIsSpace

/** Whether a pinned install may take part in a root's keys. */
export const pinCovers = (pin: string | null, rootIsOwnAssistant: boolean): boolean =>
  pin === `install` || (pin === `space` && rootIsOwnAssistant)

/** What an install remembers about a root, beside its pins. */
export type RootMark = `assistant` | `never` | `unseen`
/** What its synced rows say of it now (`unknown` = the row, or its agent's, is not synced). */
export type RootSight = `assistant` | `other` | `unknown`

/** `rootMarkCases`: whether a root counts as the member's own Assistant
 *  conversation for a space's pin, sticky; `keyed` = its store already held a
 *  key record of the root, `record` = it records one now. */
export function rootMark(
  mark: RootMark | null,
  input: { see: RootSight; keyed: boolean; record?: boolean }
): { mark: RootMark | null; counts: boolean } {
  if (mark === `never` || input.see === `other`) return { mark: `never`, counts: false }
  if (input.see === `assistant`) {
    return mark === null && input.keyed ? { mark: `never`, counts: false } : { mark: `assistant`, counts: true }
  }
  if (mark === null && input.record && !input.keyed) return { mark: `unseen`, counts: false }
  return { mark, counts: false }
}

/** `deviceCases`: what a request speaking through a space may run on. */
export const spaceMayUseDevice = (device: string, ownTree: boolean): boolean =>
  device === `space` ? ownTree : device === `otherShared`

export interface SpaceDevice {
  rowId: string
  deviceId: string
  userId: string
  label: string
  online: boolean
  assistantTeamIds: readonly string[]
  spaceHostId: string | null
}

/** Runs on's team sections: other people's opted-in hosts, per team. */
export function spaceSections(
  devices: readonly SpaceDevice[],
  teams: readonly { id: string; name: string }[],
  boundId: string | null,
  me: string
) {
  const boundHost = devices.find((device) => device.deviceId === boundId)?.spaceHostId ?? null
  return teams.flatMap((team) => {
    const rows = devices
      .filter((device) => device.userId !== me && !device.spaceHostId && device.assistantTeamIds.includes(team.id))
      .sort((a, b) => (a.label < b.label ? -1 : a.label > b.label ? 1 : 0))
      .map((device) => {
        const selected = boundHost !== null && device.rowId === boundHost
        return { id: device.deviceId, label: device.label, disabled: !device.online && !selected, selected }
      })
    return rows.length > 0 ? [{ teamId: team.id, title: team.name, rows }] : []
  })
}
