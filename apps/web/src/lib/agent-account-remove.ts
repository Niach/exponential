// EXP-862: "Remove account" — the rule behind the chip menu's destructive
// entry, and the sentence the confirm asks. EXP-1137 adds "Sign out" beside
// it.
//
// What the removal removes is the MACHINE's copy of a login: the agent CLI's
// config dir for that profile (credentials included) and its index row. The
// ACCOUNT itself is untouched — the device never runs `codex logout`, which
// would revoke it server-wide, and nothing about it leaves the machine. Which
// is exactly what the confirm says, so nobody has to guess.
//
// "Sign out" (EXP-1137, cap `account-sign-out`) signs one login out on the
// machine (claude's own `auth logout` inside its config dir; codex's
// credential file deleted, never `codex logout`) and keeps the row. Every row
// is a profile dir the machine created: the agent CLI's own (ambient) login is
// never listed, so it is never removed or signed out from here.
//
// Hand-mirrored ×4 (desktop `usage_bar.rs` / `agent_account_actions.rs`, iOS
// and Android `AgentAccountsRows`): same rule, same strings.
import type { DeviceAgentHealth } from "@/db/schema"
import {
  deviceCanAgentLogin,
  deviceCanRemoveAccount,
  deviceCanSignOutAccount,
  type SteerDevice,
} from "./steer-devices"

/** The chip fields the sign-out rule reads — a `DeviceAccountChip` and the
 * account page's `AgentProfileUsageRow` both satisfy it. */
export interface RemovableAccountRow {
  signedIn: boolean
  health: DeviceAgentHealth
}

/** Byte-identical with the server's refusal (lib/trpc/devices.ts), so a
 * requester that raced a downgrade reads the same sentence twice. */
export const REMOVE_ACCOUNT_OLD_APP = `That machine runs an older Exponential app that cannot remove agent accounts. Update it first.`

/** EXP-1137: the server's refusal for a sign-out on a build without the
 * body. Byte-identical ×4. */
export const SIGN_OUT_OLD_APP = `That machine runs an older Exponential app that cannot sign agent accounts out. Update it first.`

/** Why "Remove account" is NOT offered for this login on this machine, or
 * null when it is. The menu shows the entry exactly when this is null; the
 * sentence exists for the tooltip and for a requester that raced the state:
 * the machine needs the removal body (`account-remove`) to delete the dir.
 *
 * EXP-944: being signed OUT is not one of them. A dead profile is the thing
 * people most want gone, the removal is a profile-dir delete that never
 * touches the account (no `codex logout`, ever), and the server has always
 * taken it — it gates on the caps and the reported profile, never on the
 * credential's state. So a signed-out login offers "Sign in" AND "Remove
 * account". */
export function removeAccountBlockReason(
  device: Pick<SteerDevice, `caps`>
): string | null {
  if (!deviceCanAgentLogin(device) || !deviceCanRemoveAccount(device)) {
    return REMOVE_ACCOUNT_OLD_APP
  }
  return null
}

/** Whether the chip menu offers "Remove account" for this login. */
export function canRemoveAccountOn(
  device: Pick<SteerDevice, `caps`>
): boolean {
  return removeAccountBlockReason(device) === null
}

/** EXP-1137: why "Sign out" is NOT offered for this login on this machine,
 * or null when it is: there is nothing to sign out of, or the build cannot
 * run the command. A revoked credential (`needs_relogin`) still signs out —
 * that is how the dead credential leaves the machine. */
export function signOutBlockReason(
  device: Pick<SteerDevice, `caps`>,
  row: RemovableAccountRow
): string | null {
  if (!row.signedIn) return `That login is already signed out there.`
  if (!deviceCanAgentLogin(device) || !deviceCanSignOutAccount(device)) {
    return SIGN_OUT_OLD_APP
  }
  return null
}

/** Whether the chip menu offers "Sign out" for this login. */
export function canSignOutAccountOn(
  device: Pick<SteerDevice, `caps`>,
  row: RemovableAccountRow
): boolean {
  return signOutBlockReason(device, row) === null
}

/** The confirm the destructive entry asks, pinned ×4: it names the login and
 * the machine, and it says in the same breath that the account survives. */
export function removeAccountConfirmCopy(
  accountLabel: string,
  deviceLabel: string
): string {
  return `Delete ${accountLabel} on ${deviceLabel}? The login is removed from this device only; the account itself is untouched.`
}

/** EXP-1137: the sign-out confirm, pinned ×4: the login keeps its row for a
 * later sign-in. */
export function signOutConfirmCopy(
  accountLabel: string,
  deviceLabel: string
): string {
  return `Sign ${accountLabel} out on ${deviceLabel}? The login stays listed so it can sign in again; the account itself is untouched.`
}
