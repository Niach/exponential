// EXP-862 — "Remove account": the rule behind the chip menu's destructive
// entry, and the sentence the confirm asks.
//
// What the command removes is the MACHINE's copy of a login: the agent CLI's
// config dir for that profile (credentials included) and its index row. The
// ACCOUNT itself is untouched — the device never runs `codex logout`, which
// would revoke it server-wide, and nothing about it leaves the machine. Which
// is exactly what the confirm says, so nobody has to guess.
//
// Hand-mirrored ×4 (desktop `accounts_section.rs` / `machines.rs`, iOS and
// Android `AgentAccountsRows`): same rule, same strings.
import type { DeviceAgentHealth } from "@/db/schema"
import { SYSTEM_PROFILE_ID } from "./agent-usage"
import {
  deviceCanAgentLogin,
  deviceCanRemoveAccount,
  type SteerDevice,
} from "./steer-devices"

/** The chip fields the rule reads — a `DeviceAccountChip` and the account
 * page's `AgentProfileUsageRow` both satisfy it. */
export interface RemovableAccountRow {
  profileId: string
  signedIn: boolean
  health: DeviceAgentHealth
}

/** Byte-identical with the server's refusal (lib/trpc/devices.ts), so a
 * requester that raced a downgrade reads the same sentence twice. */
export const REMOVE_ACCOUNT_OLD_APP = `That machine runs an older Exponential app that cannot remove agent accounts. Update it first.`

/** Why "Remove account" is NOT offered for this login on this machine, or
 * null when it is. The menu shows the entry exactly when this is null; the
 * sentence exists for the tooltip and for a requester that raced the state.
 *
 * Three refusals, in the order a person would hit them:
 *  - the ambient login: the agent CLI's own config dir, which Exponential
 *    never created and must not delete;
 *  - a machine whose build cannot run the command at all;
 *  - a login that is signed out or expired here: its chip's one repair is a
 *    sign-in, so that is all it offers. */
export function removeAccountBlockReason(
  device: Pick<SteerDevice, `caps`>,
  row: RemovableAccountRow
): string | null {
  if (!row.profileId || row.profileId === SYSTEM_PROFILE_ID) {
    return `That is the machine's own agent login, not one Exponential can remove.`
  }
  if (!deviceCanAgentLogin(device) || !deviceCanRemoveAccount(device)) {
    return REMOVE_ACCOUNT_OLD_APP
  }
  if (!row.signedIn || row.health === `needs_relogin`) {
    return `That account is signed out on that machine, so its chip offers a sign-in instead.`
  }
  return null
}

/** Whether the chip menu offers "Remove account" for this login. */
export function canRemoveAccountOn(
  device: Pick<SteerDevice, `caps`>,
  row: RemovableAccountRow
): boolean {
  return removeAccountBlockReason(device, row) === null
}

/** The confirm the destructive entry asks, pinned ×4: it names the login and
 * the machine, and it says in the same breath that the account survives. */
export function removeAccountConfirmCopy(
  accountLabel: string,
  deviceLabel: string
): string {
  return `Delete ${accountLabel} on ${deviceLabel}? The login is removed from this device only; the account itself is untouched.`
}
