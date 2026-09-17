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
 * Two refusals, in the order a person would hit them:
 *  - the ambient login: the agent CLI's own config dir, which Exponential
 *    never created and must not delete;
 *  - a machine whose build cannot run the command at all.
 *
 * EXP-944: being signed OUT is no longer one of them. A dead profile is the
 * thing people most want gone, the removal is a profile-dir delete that never
 * touches the account (no `codex logout`, ever), and the server has always
 * taken it — it gates on the ambient id, the caps and the reported profile,
 * never on the credential's state. So a signed-out named login offers "Sign
 * in" AND "Remove account". */
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
