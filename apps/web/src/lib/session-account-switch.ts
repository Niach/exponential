// EXP-849: switching a SESSION to another agent login, from its usage wall.
//
// A switch = ONE `sessions.send({ sessionId, account })` with no text: the
// server writes the synced `sessions.account` (the login the session should
// run on) and the host device resumes the SAME row on that login between
// turns. CLAUDE ONLY (codex's conversation lives in its login's own rollout
// store) and BETWEEN TURNS only.
//
// The options are THE composer's account list (`flattenAccounts`) narrowed to
// the session's agent. Every sentence is hand-mirrored ×4 (desktop
// `crates/ui/src/account_switch.rs`, Android `domain/SessionAccountSwitch.kt`,
// iOS `Domain/SessionAccountSwitch.swift`); `session-account-switch.test.ts`
// locks them against the native sources.
import type { DeviceAgentHealth } from "@/db/schema"
import {
  flattenAccounts,
  type AccountOption,
  type AccountSource,
} from "@/lib/accounts/account-option"

/** The only agent that can change account between messages (EXP-849 §E). */
export const SWITCHABLE_AGENT = `claude`

/** The rate-limit wall's control. */
export const WALL_SWITCH_LABEL = `Switch account`

/** The primary control on an account row. */
export const SWITCH_LABEL = `Switch to this account`

/** What a switch costs, said once where the switch is offered. */
export const SWITCH_COST_NOTE = `The session continues under the other account. Re-reading the transcript once costs tokens.`

// ── Refusals: one sentence each, shown ON the disabled control ─────────────

export const REASON_AGENT = `Only claude can switch accounts during a session.`
export const REASON_OFFLINE = `The machine is offline.`
export const REASON_BUSY = `The agent is working — switching waits for the turn to finish.`
export const REASON_SIGNED_OUT = `Sign in to this account on that machine first.`
export const REASON_NEEDS_RELOGIN = `This account needs a re-login on that machine.`
export const REASON_ALREADY = `This session is already on this account.`

/** Who may steer a session (Threads v2 visibility ×4): a session on a
 *  team-SHARED device = every member, any other = its starter only. */
export function canSteerSession(
  session: { shared: boolean; userId: string },
  currentUserId: string | null | undefined
): boolean {
  if (session.shared) return true
  return Boolean(currentUserId) && session.userId === currentUserId
}

/** The session's agent: the row's `cli`, else the device's default agent. */
export function sessionSwitchAgent(
  cli: string | null | undefined,
  device: AccountSource | null | undefined
): string | null {
  return cli ?? device?.launchDefaults?.defaultAgent ?? null
}

/** The run-level refusal, or null. The ORDER is the ×4 order: the most
 *  fundamental refusal wins, so a codex session never reads "offline". */
export function switchBlockedReason(input: {
  agent: string | null
  deviceOnline: boolean
  busy: boolean
}): string | null {
  if (input.agent !== SWITCHABLE_AGENT) return REASON_AGENT
  if (!input.deviceOnline) return REASON_OFFLINE
  if (input.busy) return REASON_BUSY
  return null
}

/** One login's refusal, or null = switchable. */
export function switchOptionBlockedReason(
  option: { id: string; health: DeviceAgentHealth },
  currentAccount: string | null | undefined
): string | null {
  if (option.health === `needs_relogin`) return REASON_NEEDS_RELOGIN
  if (option.health === `signed_out`) return REASON_SIGNED_OUT
  if (currentAccount && currentAccount === option.id) return REASON_ALREADY
  return null
}

export interface SessionAccountSwitchOption extends AccountOption {
  /** This session is on it (`sessions.account`). */
  current: boolean
  blockedReason: string | null
}

/** The device's signed-in logins for the session's agent, in the composer's
 *  order, each with its refusal. */
export function sessionAccountSwitchOptions(
  device: AccountSource | null | undefined,
  session: { cli: string | null | undefined; account: string | null | undefined }
): SessionAccountSwitchOption[] {
  if (!device) return []
  const agent = sessionSwitchAgent(session.cli, device)
  if (!agent) return []
  return flattenAccounts(device)
    .filter((option) => option.agent === agent)
    .map((option) => ({
      ...option,
      current: Boolean(session.account) && option.id === session.account,
      blockedReason: switchOptionBlockedReason(option, session.account),
    }))
}
