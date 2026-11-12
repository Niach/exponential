// EXP-862: the account ACTIONS — the one set every agent login wears — plus
// the outcome view a finished `agent_login` command renders into.
//
// #EXP-1643: the one surface that mounts them is the device card's account
// panel (`device-logins.tsx`, `DeviceAccountPanel`). The rules below are
// unchanged, because they were always about one login on one machine.
//
// They are ONE rule, hand-mirrored ×4 (desktop `usage_bar.rs` / `machines.rs`,
// iOS `DeviceLogins`, Android `AgentAccountsRows.chipActions`), in this fixed
// order:
//
//   - "Sign in": signed out, or a credential that expired here.
//   - "Persist login" / "Renew persisted login" (EXP-1533, EXP-1626): claude
//     rows that do NOT need a sign-in — the machine runs `claude setup-token`
//     in a fresh staging dir and attaches the one-year token to that
//     profile; "Renew" while the profile already holds one
//     (`tokenExpiresAt`). "Sign in" is always the browser login.
//   - "Sign out" (EXP-1137): signed in (`agent_profile_sign_out`: claude's own `auth logout` inside that
//     profile's config dir, codex's credential file deleted — never `codex
//     logout`, which revokes the account server-wide). The row stays.
//   - "Remove account": always (deletes THIS
//     machine's copy of the login: its profile dir and its index row). Every
//     row is a profile dir; the CLI's own ambient login is never listed.
//
// EXP-1158: no entry picks the login the machine starts on — that is the
// LAST USED one, moved only by a person's start or switch.
//
// The account itself is untouched by every entry, which is exactly what each
// confirm says (`lib/agent-account-remove.ts`).
//
// A login on a teammate's machine or an offline one is a statement, not a
// control — and its health badge is the ONLY signed-out notice on the row.
//
// EXP-765: claude's sign-in link carries `code=true` — the browser page ends
// by showing an authorization CODE the CLI on the machine is still waiting
// for. The code field under the link hands it back as an `agent_login_code`
// command.
import { useState, type ReactNode } from "react"
import type {
  DeviceAgentAccount,
  DeviceAgentHealth,
  DeviceAgentProfileEntry,
} from "@/db/schema"
import {
  conceptIcon,
  Button,
  Input,
  DEVICE_READINESS_COPY,
  Prompt,
  toast,
  type DeviceAccountPanelAction,
} from "@exp/ui"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"
import { parseAgentLoginResult, accountName } from "@/lib/agent-usage"
import {
  canSignOutAccountOn,
  removeAccountConfirmCopy,
  signOutConfirmCopy,
} from "@/lib/agent-account-remove"
import {
  deviceIsOnline,
  deviceIsMine,
  type SteerDevice,
} from "@/lib/steer-devices"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"

const SignInIcon = conceptIcon(`ui-sign-in`)
const CopyIcon = conceptIcon(`ui-copy`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)

/** The chip menu's entries, byte-identical ×4 (Android
 * `AgentAccountsRows.ACTION_*`). */
export const ACTION_SIGN_IN = `Sign in`
/** EXP-1533 / EXP-1626: Persist login on a signed-in claude login (a
 *  `claude setup-token` attached to the profile) — "Persist" while it holds
 *  none, "Renew" while it does (devices-list.json copy, ×4). */
export const ACTION_PERSIST_LOGIN = DEVICES_LIST_COPY.persistLogin
export const ACTION_RENEW_PERSISTED_LOGIN = DEVICES_LIST_COPY.renewPersistedLogin
export const ACTION_SIGN_OUT = `Sign out`
export const ACTION_REMOVE = `Remove account`

/** The command key the dialog tracks a per-agent login under. */
export function agentLoginKey(agent: string): string {
  return `login:${agent}`
}

/** EXP-765: the key the dialog tracks a per-agent `agent_login_code` under. */
export function agentLoginCodeKey(agent: string): string {
  return `login-code:${agent}`
}

const LOGIN_CODE_PREFIX = `login-code:`

/** The agent behind a `login-code:` key, or null for any other key. */
export function agentOfLoginCodeKey(key: string): string | null {
  return key.startsWith(LOGIN_CODE_PREFIX)
    ? key.slice(LOGIN_CODE_PREFIX.length)
    : null
}

/** One login on ONE machine — `AgentProfileUsageRow` (`deviceLoginRows`)
 * satisfies it. */
export interface AccountChipRow {
  agent: string
  profileId: string
  email?: string | null
  plan?: string | null
  signedIn: boolean
  /** The machine's LAST USED login for that agent (`profiles[].active`). */
  active: boolean
  health: DeviceAgentHealth
  /** EXP-1533: when the profile's long-lived claude login expires (ISO);
   *  present while it holds one. */
  tokenExpiresAt?: string | null
}

/** EXP-1626: the sign-in Persist login / Renew persisted login queue for a
 *  login: `claude setup-token` for THAT profile, both in place. */
export function persistLoginTarget(
  row: Pick<AccountChipRow, `profileId`>
): { profileId: string; longLived: true } {
  return { profileId: row.profileId, longLived: true }
}

/** EXP-1533 / EXP-1626: the persist entry this row offers, or null (claude
 *  only, and only on a login that does not need a sign-in: persisting is an
 *  option on an EXISTING signed-in login). */
export function persistLoginAction(row: AccountChipRow): string | null {
  if (row.agent !== `claude` || chipSignsIn(row)) return null
  return row.tokenExpiresAt ? ACTION_RENEW_PERSISTED_LOGIN : ACTION_PERSIST_LOGIN
}

/** The chip's one repair is a sign-in: it has no working credential here. */
export function chipSignsIn(row: AccountChipRow): boolean {
  return !row.signedIn || row.health === `needs_relogin`
}

/** The entries this chip's menu shows, in order. EXP-944: a signed-out login
 * keeps "Sign in" as its first (and only repairing) entry, but it no longer
 * ENDS there — a dead named profile can be removed too, and codex logins,
 * which are signed out far more often than claude's, were left with a menu of
 * one. EXP-1137: a signed-in login offers "Sign out". EXP-1533: a claude
 * login offers Persist login right after "Sign in". */
export function accountChipActions(row: AccountChipRow): string[] {
  const out: string[] = []
  if (chipSignsIn(row)) out.push(ACTION_SIGN_IN)
  const persist = persistLoginAction(row)
  if (persist) out.push(persist)
  if (canSignOutAccountOn(row)) out.push(ACTION_SIGN_OUT)
  out.push(ACTION_REMOVE)
  return out
}

/** Whether the chip is a CONTROL at all: one of the caller's own machines,
 * listening ("Remove account" is always on offer). Every action rides the
 * owner→device queue, so an offline machine would hold it until it wakes,
 * which reads as a dead click. */
export function accountChipActionable(device: SteerDevice): boolean {
  return deviceIsMine(device) && deviceIsOnline(device)
}

/** The login a confirm names (`accountName`): its address. */
export function accountChipLabel(row: AccountChipRow): string {
  return accountName(row)
}

/** profile id → the `lastLoginAt` the machine reported for it, captured when
 * a sign-in (or import) is queued. */
export type AgentLoginBaseline = Record<string, string | null>

type LoginReporting = Pick<DeviceAgentAccount, `profiles`> | null | undefined

function reportedProfiles(account: LoginReporting): DeviceAgentProfileEntry[] {
  return (account?.profiles ?? []).filter(
    (profile): profile is DeviceAgentProfileEntry => Boolean(profile?.id)
  )
}

export function agentLoginBaseline(account: LoginReporting): AgentLoginBaseline {
  return Object.fromEntries(
    reportedProfiles(account).map((profile) => [
      profile.id,
      profile.lastLoginAt ?? null,
    ])
  )
}

/** Where a sign-in landed: the profile and whether it was a DUPLICATE. */
export interface AgentLoginLanding {
  profileId: string
  /** The landed login's name (`accountName`). */
  email: string
  /** The machine already held that email and refreshed it: the login was
   *  "Add account" (no intended profile) or meant for ANOTHER profile. */
  duplicate: boolean
}

/** Has a queued sign-in landed on the machine yet? The device lands every
 * login on the profile whose EMAIL it signed in as (refreshing it, or adding
 * a profile) and stamps that profile's `lastLoginAt`. So landed = a row whose
 * `lastLoginAt` is set and differs from the `baseline` captured at queue time
 * (or a new id that carries one). Never "signed in": a revoked credential
 * keeps that flag, and the login may land on another profile than the one
 * clicked. Hand-mirrored ×4. */
export function agentLoginLanding(
  account: LoginReporting,
  baseline: AgentLoginBaseline,
  intendedProfileId?: string
): AgentLoginLanding | null {
  for (const profile of reportedProfiles(account)) {
    if (!profile.lastLoginAt) continue
    const existed = profile.id in baseline
    if (existed && baseline[profile.id] === profile.lastLoginAt) continue
    return {
      profileId: profile.id,
      email: accountName(profile),
      duplicate:
        existed && (!intendedProfileId || intendedProfileId !== profile.id),
    }
  }
  return null
}

/** `{email} was already added. Refreshed it.` — the warning toast a duplicate
 * landing raises (device-doctor.json `copy.alreadyAdded`, ×4). */
export function alreadyAddedCopy(email: string): string {
  return DEVICE_READINESS_COPY.alreadyAdded.replace(`{email}`, () => email)
}

/** THE account actions behind the panel (#EXP-1643): the queued commands, the
 * destructive confirms, the failure toast. They live here so the rule cannot
 * drift from the copy the other three clients hold. */
export function useAccountActions({
  device,
  row,
  accountLabel,
  onSignIn,
  onPersistLogin,
}: {
  /** The device that HOLDS this login (one of the caller's own). */
  device: SteerDevice
  row: AccountChipRow
  /** What the confirms call this login (the text the row rendered). */
  accountLabel?: string
  /** Open the sign-in for THIS login (always the browser login, EXP-1626). */
  onSignIn: () => void
  /** EXP-1626: Persist login / Renew persisted login for THIS profile
   *  (`persistLoginTarget`). */
  onPersistLogin: () => void
}): {
  busy: boolean
  signIn?: DeviceAccountPanelAction
  persist?: DeviceAccountPanelAction
  signOut?: DeviceAccountPanelAction
  remove?: DeviceAccountPanelAction
  confirms: ReactNode
} {
  const [busy, setBusy] = useState(false)
  const [confirmRemove, setConfirmRemove] = useState(false)
  const [confirmSignOut, setConfirmSignOut] = useState(false)
  const actions = accountChipActions(row)
  const deviceLabel = device.deviceLabel || device.deviceId
  const loginLabel = accountLabel ?? accountChipLabel(row)

  const queue = async (
    kind: `agent_profile_remove` | `agent_profile_sign_out`,
    success: string,
    failure: { verb: string; thing: string }
  ) => {
    if (busy) return
    setBusy(true)
    try {
      await trpc.devices.createCommand.mutate(
        {
          deviceId: device.deviceId,
          kind,
          agent: row.agent as never,
          profileId: row.profileId,
        },
        // The surface says what failed in its own words; the global link
        // would add a second toast on top of it.
        SKIP_ERROR_TOAST
      )
      // Both commands ride the owner→device queue and change NOTHING here
      // until the device's next heartbeat, so without this the click reads
      // as dead. Same sentence as the desktop's notification.
      toast.success(success)
    } catch (error) {
      // FEED-79: destructive, so no Retry.
      failedAction(failure.verb, failure.thing, error)
    } finally {
      setBusy(false)
    }
  }

  const persist = persistLoginAction(row)

  return {
    busy,
    signIn: actions.includes(ACTION_SIGN_IN) ? { label: ACTION_SIGN_IN, onSelect: onSignIn } : undefined,
    persist: persist && actions.includes(persist) ? { label: persist, onSelect: onPersistLogin } : undefined,
    signOut: actions.includes(ACTION_SIGN_OUT)
      ? { label: ACTION_SIGN_OUT, onSelect: () => setConfirmSignOut(true) }
      : undefined,
    remove: actions.includes(ACTION_REMOVE)
      ? { label: ACTION_REMOVE, onSelect: () => setConfirmRemove(true) }
      : undefined,
    confirms: (
      <>
        {/* EXP-1137: destructive too — the login needs a sign-in again. */}
        <Prompt
          open={confirmSignOut}
          onOpenChange={setConfirmSignOut}
          busy={busy}
          title={`${ACTION_SIGN_OUT}?`}
          body={signOutConfirmCopy(loginLabel, deviceLabel)}
          actions={[
            { label: `Cancel`, role: `cancel` },
            {
              label: `Sign out`,
              role: `destructive`,
              busy,
              onSelect: () => {
                setConfirmSignOut(false)
                void queue(
                  `agent_profile_sign_out`,
                  `${deviceLabel} will sign this login out.`,
                  { verb: `sign out`, thing: `the login` }
                )
              },
            },
          ]}
        />

        {/* Destructive, so it asks — and the sentence says in the same breath
            that only this device's copy of the login goes. */}
        <Prompt
          open={confirmRemove}
          onOpenChange={setConfirmRemove}
          busy={busy}
          title={`${ACTION_REMOVE}?`}
          body={removeAccountConfirmCopy(loginLabel, deviceLabel, row.agent)}
          actions={[
            { label: `Cancel`, role: `cancel` },
            {
              label: `Remove`,
              role: `destructive`,
              busy,
              onSelect: () => {
                setConfirmRemove(false)
                void queue(
                  `agent_profile_remove`,
                  `${deviceLabel} will remove this login.`,
                  { verb: `remove`, thing: `the login` }
                )
              },
            },
          ]}
        />
      </>
    ),
  }
}

/** The device's own sentence when a login command FAILED, else null. The
 *  dialog raises it as the failed action's toast (FEED-79). */
export function agentLoginFailure(result: string | null | undefined): string | null {
  if (!result) return null
  const progress = parseAgentLoginResult(result)
  if (progress?.phase !== `failed`) return null
  return progress.message ?? `The device reported a failure.`
}

/** What a finished login command hands back: the CLI's own sign-in URL (open
 * it anywhere) plus, for Codex's device-code flow, the code to type on the
 * machine. A link WITHOUT a code is claude's: the browser hands one back
 * instead, and (EXP-765) the field below the link returns it to the machine.
 * Anything unparsable renders as the raw text the device sent. */
export function AgentLoginOutcome({
  result,
  codePending,
  onEnterCode,
}: {
  result: string
  codePending: boolean
  onEnterCode: (code: string) => void
}) {
  const [draft, setDraft] = useState(``)
  const progress = parseAgentLoginResult(result)
  // A failed login is the host's toast (FEED-79, `agentLoginFailure`).
  if (progress?.phase === `failed`) return null
  if (!progress?.url) {
    return <p className="text-xs text-muted-foreground">{result}</p>
  }
  const code = progress.code
  const wantsCodeBack = !code
  const submit = () => {
    const trimmed = draft.trim()
    if (!trimmed || codePending) return
    onEnterCode(trimmed)
    setDraft(``)
  }
  return (
    <div className="space-y-1">
      <a
        href={progress.url}
        target="_blank"
        rel="noreferrer"
        className="flex items-center gap-1 text-xs text-primary underline underline-offset-2"
      >
        <ExternalLinkIcon className="size-3 shrink-0" />
        <span className="min-w-0 truncate">{progress.url}</span>
      </a>
      {code && (
        <div className="flex items-center gap-1.5">
          <span className="font-mono text-xs">{code}</span>
          <Button
            variant="ghost"
            className="h-5 w-5 p-0 text-muted-foreground"
            aria-label="Copy code"
            title="Copy code"
            onClick={() => {
              void navigator.clipboard?.writeText(code)
            }}
          >
            <CopyIcon className="size-3" />
          </Button>
        </div>
      )}
      {wantsCodeBack && (
        <div className="flex items-center gap-1.5">
          <Input
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === `Enter`) {
                event.preventDefault()
                submit()
              }
            }}
            placeholder="Code from the browser"
            aria-label="Code from the browser"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            className="h-7 font-mono text-xs"
          />
          <Button
            variant="glass"
            size="sm"
            className="shrink-0"
            disabled={!draft.trim() || codePending}
            onClick={submit}
          >
            <SignInIcon className="size-3" />
            Enter code
          </Button>
        </div>
      )}
      <p className="text-[11px] text-muted-foreground">
        {code
          ? `Open the link on any device and enter the code on the machine.`
          : wantsCodeBack
            ? `Open the link on any device, then paste the code it shows here.`
            : `Open the link on any device.`}
      </p>
    </div>
  )
}
