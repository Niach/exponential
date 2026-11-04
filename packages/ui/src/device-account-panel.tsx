import type { ReactNode } from "react"
import { Button } from "./button"
import { conceptIcon } from "./icons.generated"
import { Pill } from "./pill"
import { UsageWindows, type UsageWindow } from "./usage-windows"

// #EXP-1643 (fixtures/devices-list.json THE ACCOUNT PANEL): what a device card
// grows when an account icon is pressed: email + the icon-only Sign out and
// Remove, plan + the one action pill, the persisted and status lines, the usage
// windows. Hand-mirrored ×4 (desktop `machines.rs`, iOS `DeviceLogins`, Android
// `AgentsScreen`). Exponential-specific, not eUI.

const SignInIcon = conceptIcon(`ui-sign-in`)
const SignOutIcon = conceptIcon(`ui-sign-out`)
const RemoveIcon = conceptIcon(`ui-delete`)
const PersistIcon = conceptIcon(`ui-clock`)

export interface DeviceAccountPanelAction {
  label: string
  onSelect: () => void
}

export function DeviceAccountPanel({
  name,
  plan,
  persisted,
  status,
  windows,
  stale = false,
  signIn,
  persist,
  signOut,
  remove,
  busy = false,
  children,
}: {
  /** The email, else the label. */
  name: string
  plan?: string | null
  persisted?: string | null
  /** The attention item's status line. */
  status?: string | null
  windows: readonly UsageWindow[]
  stale?: boolean
  signIn?: DeviceAccountPanelAction
  persist?: DeviceAccountPanelAction
  signOut?: DeviceAccountPanelAction
  remove?: DeviceAccountPanelAction
  busy?: boolean
  /** The confirms the host owns. */
  children?: ReactNode
}) {
  const pill = signIn ?? persist
  const PillIcon = signIn ? SignInIcon : PersistIcon
  return (
    <div data-slot="device-account-panel" className="flex min-w-0 flex-col gap-3 border-t border-border pt-3">
      <div className="flex min-w-0 flex-col gap-0.5">
        <div className="flex min-h-7 min-w-0 items-center gap-0.5">
          <span className="min-w-0 flex-1 truncate text-sm font-medium">{name}</span>
          {signOut && (
            <Button
              variant="ghost"
              size="icon-sm"
              className="size-7 shrink-0 text-muted-foreground hover:text-destructive"
              disabled={busy}
              aria-label={signOut.label}
              title={signOut.label}
              data-testid="device-account-sign-out"
              onClick={signOut.onSelect}
            >
              <SignOutIcon className="size-4" />
            </Button>
          )}
          {remove && (
            <Button
              variant="ghost"
              size="icon-sm"
              className="-mr-1 size-7 shrink-0 text-muted-foreground hover:text-destructive"
              disabled={busy}
              aria-label={remove.label}
              title={remove.label}
              data-testid="device-account-remove"
              onClick={remove.onSelect}
            >
              <RemoveIcon className="size-4" />
            </Button>
          )}
        </div>
        {(plan || pill) && (
          <div className="flex min-h-7 min-w-0 items-center justify-between gap-2">
            <span className="min-w-0 truncate text-xs text-muted-foreground">{plan}</span>
            {pill && (
              <Pill
                mode="action"
                className="shrink-0"
                disabled={busy}
                data-testid="device-account-action"
                onClick={pill.onSelect}
              >
                <PillIcon className="size-3.5" />
                {pill.label}
              </Pill>
            )}
          </div>
        )}
        {persisted && (
          <span data-slot="login-persisted" className="text-xs text-muted-foreground">
            {persisted}
          </span>
        )}
        {status && <span className="text-xs text-yellow-400">{status}</span>}
      </div>
      <UsageWindows windows={windows} density="full" stale={stale} />
      {children}
    </div>
  )
}
