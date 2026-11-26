// #EXP-2161 (domain-contract member-space.json): the member SPACE a request
// speaks through. A space key (or an agent key minted through one) runs its
// whole request inside `runInSpace`; the membership reads
// (lib/auth/membership.ts) and the session reads narrow themselves from it,
// so a query added later is narrowed too. No imports of the database here:
// lib/auth/membership.ts reads this.
import { AsyncLocalStorage } from "node:async_hooks"
import { TRPCError } from "@trpc/server"

export interface SpaceGrant {
  /** The space's devices row. */
  spaceId: string
  /** Its machine id (`space-<uuid>`, what sessions name). */
  deviceId: string
  /** The member. */
  userId: string
  label: string
  spaceTeamId: string
  hostRowId: string
  hostUserId: string
  hostDeviceId: string
  /** `spaceTeams` NOW, in the member's order. */
  teamIds: string[]
  personalTeamId: string | null
  /** The member's team set through this space: personal + `teamIds`. */
  reach: string[]
  assistantId: string | null
  createdAt: Date
}

const storage = new AsyncLocalStorage<SpaceGrant>()

export const runInSpace = <T>(grant: SpaceGrant, run: () => T): T => storage.run(grant, run)

/** The space this request speaks through, if any. */
export const currentSpace = (): SpaceGrant | null => storage.getStore() ?? null

/** The space when the request speaks through one of `userId`. */
export function spaceOf(userId: string): SpaceGrant | null {
  const space = currentSpace()
  return space && space.userId === userId ? space : null
}

const ownMachines = new AsyncLocalStorage<string>()

/** #EXP-2161 (TEAMS): `run` speaks for a chat of `userId` a space started:
 *  it keeps the device rule (`sessions.via_space`). */
export const runViaSpace = <T>(userId: string, run: () => T): T => ownMachines.run(userId, run)

/** The member whose own machines this request may never run anything on. */
export const deviceRestrictedUser = (): string | null => currentSpace()?.userId ?? ownMachines.getStore() ?? null

export const SPACE_DEVICE_MESSAGE = `A member's space runs nothing on the member's own machines`

/** `deviceCases`: the machine `ownerId` runs may be used by this request. */
export function assertSpaceDevice(device: { ownerId: string; deviceId: string | null }): void {
  const member = deviceRestrictedUser()
  if (!member || device.ownerId !== member) return
  if (device.deviceId && device.deviceId === currentSpace()?.deviceId) return
  throw new TRPCError({ code: `FORBIDDEN`, message: SPACE_DEVICE_MESSAGE })
}

/** `teamIds` of `userId`, as this request may use them. */
export function narrowTeamIds(userId: string, teamIds: string[]): string[] {
  const space = spaceOf(userId)
  return space ? teamIds.filter((id) => space.reach.includes(id)) : teamIds
}

export const SPACE_KEY_MESSAGE = `This key belongs to a member's space: it reaches only that space`

/** Refuses a request that speaks through a space (settings, devices, keys, installs). */
export function assertOutsideSpace(): void {
  if (currentSpace()) throw new TRPCError({ code: `FORBIDDEN`, message: SPACE_KEY_MESSAGE })
}
