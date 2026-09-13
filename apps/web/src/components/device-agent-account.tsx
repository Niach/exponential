// EXP-862: the account CHIP — the one control every agent login wears, on a
// device row ("My devices") and on an account row (Accounts) alike — plus the
// outcome view a finished `agent_login` command renders into.
//
// The chip menu is THREE states and nothing else, hand-mirrored ×4 (desktop
// `accounts_section.rs` / `machines.rs`, iOS `DeviceAccountChips`, Android
// `AgentAccountsRows.chipActions`):
//
//   - signed out, or a credential that expired here: "Sign in", alone. A dead
//     login cannot be switched to, and removing it repairs nothing.
//   - healthy, and NOT the machine's current login: "Set as default"
//     (`agent_profile_use` — no login flow, no logout, no credential copied)
//     and "Remove account".
//   - healthy, and already the machine's login: "Remove account".
//
// "Remove account" deletes THIS machine's copy of the login (its profile dir
// and its index row); the account itself is untouched, which is exactly what
// the confirm says (`removeAccountConfirmCopy`). Nothing here ever runs a
// logout: `codex logout` revokes the account server-wide.
//
// A chip with no entry at all (a teammate's machine, an offline one, a build
// that takes none of the commands) is a statement, not a control — and its
// health badge is the ONLY signed-out notice on the row.
//
// EXP-765: claude's sign-in link carries `code=true` — the browser page ends
// by showing an authorization CODE the CLI on the machine is still waiting
// for. The code field under the link hands it back as an `agent_login_code`
// command (its own cap: a build that only runs the login would report the
// command unsupported, so the field stays hidden without it).
import { useState, type ReactNode } from "react"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import type {
  DeviceAgentAccount,
  DeviceAgentHealth,
  DeviceAgentProfileEntry,
} from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import {
  agentHealth,
  parseAgentLoginResult,
  SYSTEM_PROFILE_ID,
} from "@/lib/agent-usage"
import {
  canRemoveAccountOn,
  removeAccountConfirmCopy,
} from "@/lib/agent-account-remove"
import {
  deviceCanAgentLogin,
  deviceCanSwitchAccount,
  deviceIsOnline,
  deviceIsMine,
  type SteerDevice,
} from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { agentLabel } from "@/components/agent-picker"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

const SignInIcon = conceptIcon(`ui-sign-in`)
const SwapIcon = conceptIcon(`ui-swap`)
const RemoveIcon = conceptIcon(`ui-delete`)
const CopyIcon = conceptIcon(`ui-copy`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)

/** The chip menu's three entries, byte-identical ×4 (Android
 * `AgentAccountsRows.ACTION_*`). */
export const ACTION_SIGN_IN = `Sign in`
export const ACTION_SET_DEFAULT = `Set as default`
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

/** One login on ONE machine — a device row's `DeviceAccountChip` and the
 * accounts page's `AgentProfileUsageRow` both satisfy it. */
export interface AccountChipRow {
  agent: string
  profileId: string
  /** The profile's label (`Default` for the ambient login). */
  profileLabel: string
  email?: string | null
  signedIn: boolean
  /** The machine's ACTIVE login for that agent. */
  active: boolean
  health: DeviceAgentHealth
}

/** The chip's one repair is a sign-in: it has no working credential here. */
export function chipSignsIn(row: AccountChipRow): boolean {
  return !row.signedIn || row.health === `needs_relogin`
}

/** A healthy login this machine is not using becomes its login — one queued
 * `agent_profile_use`, gated on the machine's `account-switch` cap (the
 * server refuses the command without it). */
export function chipSetsDefault(
  row: AccountChipRow,
  canSwitchAccount: boolean
): boolean {
  return !chipSignsIn(row) && !row.active && canSwitchAccount
}

/** The entries this chip's menu shows, in order. */
export function accountChipActions(
  device: Pick<SteerDevice, `caps`>,
  row: AccountChipRow
): string[] {
  if (chipSignsIn(row)) return [ACTION_SIGN_IN]
  const out: string[] = []
  if (chipSetsDefault(row, deviceCanSwitchAccount(device))) {
    out.push(ACTION_SET_DEFAULT)
  }
  if (canRemoveAccountOn(device, row)) out.push(ACTION_REMOVE)
  return out
}

/** Whether the chip is a CONTROL at all: one of the caller's own machines,
 * listening, able to take the commands — and with an entry to offer. Every
 * action rides the owner→device queue, so an offline machine would hold it
 * until it wakes, which reads as a dead click. */
export function accountChipActionable(
  device: SteerDevice,
  row: AccountChipRow
): boolean {
  if (!deviceIsMine(device) || !deviceIsOnline(device)) return false
  if (!deviceCanAgentLogin(device)) return false
  return accountChipActions(device, row).length > 0
}

/** The login a confirm names: its address, else the profile's own label. */
export function accountChipLabel(row: AccountChipRow): string {
  return row.email || row.profileLabel
}

/** EXP-862: has the login a sign-in was FOR landed on the device yet? The
 * login dialog closes on that transition, and "signed in" alone answers the
 * wrong question twice over:
 *
 *  - a credential the probe found revoked keeps `signedIn === true` the whole
 *    time (the CLI still claims a login, only the probe knows better), so a
 *    re-login would open already "signed in" and never close;
 *  - an "Add account" run names a profile the device has not created yet, so
 *    reading the account's own flag reports the AMBIENT login's state — true
 *    on any device that already holds one.
 *
 * Landed = signed in AND not revoked, on the profile that was targeted: by
 * id, or by the label the device was asked to create it under (the id is the
 * device's to mint, `agent_profiles::create`). */
export function agentLoginLanded(
  account:
    | Pick<DeviceAgentAccount, `signedIn` | `health` | `profiles`>
    | null
    | undefined,
  target: { profileId?: string; newProfileLabel?: string }
): boolean {
  if (!account) return false
  const usable = (
    entry: Pick<DeviceAgentProfileEntry, `signedIn` | `health`>
  ): boolean =>
    entry.signedIn === true && agentHealth(entry) !== `needs_relogin`
  const profiles = (account.profiles ?? []).filter(
    (profile): profile is DeviceAgentProfileEntry => Boolean(profile?.id)
  )
  const label = target.newProfileLabel?.trim()
  if (label) {
    return profiles.some(
      (profile) => (profile.label ?? ``).trim() === label && usable(profile)
    )
  }
  const profileId = target.profileId
  if (profileId && profileId !== SYSTEM_PROFILE_ID) {
    return profiles.some(
      (profile) => profile.id === profileId && usable(profile)
    )
  }
  // The ambient login: a device that reports profiles carries it as the
  // `system` row, and its top-level fields are the ACTIVE profile's.
  const ambient = profiles.find((profile) => profile.id === SYSTEM_PROFILE_ID)
  return usable(ambient ?? account)
}

/** THE chip menu. The trigger is the caller's pill (the two surfaces draw
 * different chips); everything behind it — the queued commands, the destructive
 * confirm, the failure toast — lives here so the rule cannot drift between the
 * device rows and the account rows. */
export function AccountChipMenu({
  device,
  row,
  accountLabel,
  onSignIn,
  trigger,
}: {
  /** The device that HOLDS this login (one of the caller's own). */
  device: SteerDevice
  row: AccountChipRow
  /** What the remove confirm calls this login. Defaults to the login's own
   *  address/label; a device row passes its chip's text (`Claude Code ·
   *  dev@acme.test`), so the sentence names exactly what was clicked. */
  accountLabel?: string
  /** Open the sign-in for THIS login (`AgentLoginDialogHost` lives at the
   *  route level; the caller owns the request so this module stays off the
   *  dialog's import graph). */
  onSignIn: () => void
  trigger: ReactNode
}) {
  const [busy, setBusy] = useState(false)
  const [confirmRemove, setConfirmRemove] = useState(false)
  const actions = accountChipActions(device, row)
  const deviceLabel = device.deviceLabel || device.deviceId

  const queue = async (
    kind: `agent_profile_use` | `agent_profile_remove`,
    success: string,
    failure: string
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
        { context: { skipErrorToast: true } }
      )
      // Both commands ride the owner→device queue and change NOTHING here
      // until the device's next heartbeat, so without this the click reads
      // as dead. Same sentence as the desktop's notification
      // (`accounts_section.rs`).
      toast.success(success)
    } catch (error) {
      toast.error(failure, {
        description: trpcErrorMessage(
          error,
          `The command could not be queued on the device.`
        ),
      })
    } finally {
      setBusy(false)
    }
  }

  // The login dialog is hosted elsewhere in the tree — hand off a tick after
  // the menu closes, or the close's focus return swallows it.
  const signIn = () => setTimeout(onSignIn, 0)

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
        <DropdownMenuContent align="start">
          {actions.includes(ACTION_SIGN_IN) && (
            <DropdownMenuItem onSelect={signIn}>
              <SignInIcon />
              {ACTION_SIGN_IN}
            </DropdownMenuItem>
          )}
          {actions.includes(ACTION_SET_DEFAULT) && (
            <DropdownMenuItem
              disabled={busy}
              onSelect={() =>
                void queue(
                  `agent_profile_use`,
                  `${deviceLabel} will run ${agentLabel(row.agent)} as this account.`,
                  `Couldn't switch the account on that device`
                )
              }
            >
              <SwapIcon />
              {ACTION_SET_DEFAULT}
            </DropdownMenuItem>
          )}
          {actions.includes(ACTION_REMOVE) && (
            <DropdownMenuItem
              variant="destructive"
              disabled={busy}
              onSelect={() => setConfirmRemove(true)}
            >
              <RemoveIcon />
              {ACTION_REMOVE}
            </DropdownMenuItem>
          )}
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Destructive, so it asks — and the sentence says in the same breath
          that only this device's copy of the login goes. */}
      <AlertDialog
        open={confirmRemove}
        onOpenChange={(open) => {
          if (!open && !busy) setConfirmRemove(false)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{ACTION_REMOVE}</AlertDialogTitle>
            <AlertDialogDescription>
              {removeAccountConfirmCopy(
                accountLabel ?? accountChipLabel(row),
                deviceLabel
              )}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={(event) => {
                event.preventDefault()
                setConfirmRemove(false)
                void queue(
                  `agent_profile_remove`,
                  `${deviceLabel} will remove this login.`,
                  `Couldn't remove the account on that device`
                )
              }}
            >
              {busy && <LoaderCircle className="animate-spin" />}
              Remove
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  )
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
  if (progress?.phase === `failed`) {
    return (
      <p className="text-xs text-destructive">
        {progress.message ?? `The device reported a failure.`}
      </p>
    )
  }
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
