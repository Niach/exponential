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
// "Sign out" (EXP-1137) signs one login out on the
// machine (claude's own `auth logout` inside its config dir; codex's
// credential file deleted, never `codex logout`) and keeps the row. Every row
// is a profile dir the machine created: the agent CLI's own (ambient) login is
// never listed, so it is never removed or signed out from here.
//
// Hand-mirrored ×4 (desktop `usage_bar.rs` / `agent_account_actions.rs`, iOS
// and Android `AgentAccountsRows`): same rule, same strings.
import type { DeviceAgentHealth } from "@/db/schema"

/** The chip fields the sign-out rule reads — a `DeviceAccountChip` and the
 * account page's `AgentProfileUsageRow` both satisfy it. */
export interface RemovableAccountRow {
  signedIn: boolean
  health: DeviceAgentHealth
}

// "Remove account" is offered on EVERY login: no build cap gates it, every
// daemon at the client floor runs it. EXP-944: being signed OUT does not
// block it either. A dead profile is the thing people most want gone, the
// removal is a profile-dir delete that never touches the account (no `codex
// logout`, ever), and the server gates on the reported profile, never on the
// credential's state. So a signed-out login offers "Sign in" AND "Remove
// account".

/** EXP-1137: whether the chip menu offers "Sign out" for this login: it is
 * signed in. A revoked credential (`needs_relogin`) still signs out — that is
 * how the dead credential leaves the machine. */
export function canSignOutAccountOn(row: RemovableAccountRow): boolean {
  return row.signedIn
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
