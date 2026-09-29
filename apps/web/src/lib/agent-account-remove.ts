// EXP-862: "Remove account" — the rule behind the chip menu's destructive
// entry, and the sentence the confirm asks. EXP-1137 adds "Sign out" beside
// it, and lets the machine's own AMBIENT login be removed too.
//
// What the removal removes is the MACHINE's copy of a login: the agent CLI's
// config dir for that profile (credentials included) and its index row. The
// ACCOUNT itself is untouched — the device never runs `codex logout`, which
// would revoke it server-wide, and nothing about it leaves the machine. Which
// is exactly what the confirm says, so nobody has to guess.
//
// EXP-1137: the ambient login (`system`) is the agent CLI's own config dir,
// so there is no dir to delete. "Remove account" there means: sign it out on
// the machine (claude's own `auth logout`; codex's credential file deleted,
// never `codex logout`) and hide the row until that login signs in again.
// "Sign out" alone does the first half and keeps the row, for any login.
// Both ride the machine's `account-sign-out` cap.
//
// Hand-mirrored ×4 (desktop `usage_bar.rs` / `agent_account_actions.rs`, iOS
// and Android `AgentAccountsRows`): same rule, same strings.
import type { DeviceAgentHealth } from "@/db/schema"
import { SYSTEM_PROFILE_ID } from "./agent-usage"
import {
  deviceCanAgentLogin,
  deviceCanRemoveAccount,
  deviceCanSignOutAccount,
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

/** EXP-1137: the server's refusal for a sign-out (or an ambient removal,
 * which signs out first) on a build without the body. Byte-identical ×4. */
export const SIGN_OUT_OLD_APP = `That machine runs an older Exponential app that cannot sign agent accounts out. Update it first.`

/** The ambient login: the CLI's own config dir. */
export function isAmbientProfile(profileId: string): boolean {
  return !profileId || profileId === SYSTEM_PROFILE_ID
}

/** Why "Remove account" is NOT offered for this login on this machine, or
 * null when it is. The menu shows the entry exactly when this is null; the
 * sentence exists for the tooltip and for a requester that raced the state.
 *
 * Two builds, one refusal each:
 *  - the ambient login needs the sign-out body (`account-sign-out`): the
 *    machine signs it out and hides the row;
 *  - a named profile needs the removal body (`account-remove`): the machine
 *    deletes its dir.
 *
 * EXP-944: being signed OUT is not one of them. A dead profile is the thing
 * people most want gone, the removal is a profile-dir delete that never
 * touches the account (no `codex logout`, ever), and the server has always
 * taken it — it gates on the caps and the reported profile, never on the
 * credential's state. So a signed-out login offers "Sign in" AND "Remove
 * account". */
export function removeAccountBlockReason(
  device: Pick<SteerDevice, `caps`>,
  row: RemovableAccountRow
): string | null {
  if (!deviceCanAgentLogin(device)) {
    return isAmbientProfile(row.profileId)
      ? SIGN_OUT_OLD_APP
      : REMOVE_ACCOUNT_OLD_APP
  }
  if (isAmbientProfile(row.profileId)) {
    return deviceCanSignOutAccount(device) ? null : SIGN_OUT_OLD_APP
  }
  return deviceCanRemoveAccount(device) ? null : REMOVE_ACCOUNT_OLD_APP
}

/** Whether the chip menu offers "Remove account" for this login. */
export function canRemoveAccountOn(
  device: Pick<SteerDevice, `caps`>,
  row: RemovableAccountRow
): boolean {
  return removeAccountBlockReason(device, row) === null
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

/** EXP-1137: the ambient login's remove confirm, pinned ×4. `agentLabel` is
 * the agent's display name (`Claude` / `Codex`): the sentence has to say
 * that the CLI in the person's own terminal is signed out along with it. */
export function removeAmbientAccountConfirmCopy(
  accountLabel: string,
  deviceLabel: string,
  agentLabel: string
): string {
  return `Remove ${accountLabel} from ${deviceLabel}? The machine's own ${agentLabel} login is signed out there, including for the ${agentLabel} CLI in the terminal, and hidden here until it signs in again; the account itself is untouched.`
}

/** EXP-1137: the sign-out confirm, pinned ×4. `ambientAgentLabel` names the
 * agent when the login is the machine's own (the terminal CLI signs out
 * too); a named profile keeps its row for a later sign-in. */
export function signOutConfirmCopy(
  accountLabel: string,
  deviceLabel: string,
  ambientAgentLabel?: string | null
): string {
  if (ambientAgentLabel) {
    return `Sign ${accountLabel} out on ${deviceLabel}? That is the machine's own ${ambientAgentLabel} login, so the ${ambientAgentLabel} CLI there is signed out too; the account itself is untouched.`
  }
  return `Sign ${accountLabel} out on ${deviceLabel}? The login stays listed so it can sign in again; the account itself is untouched.`
}
