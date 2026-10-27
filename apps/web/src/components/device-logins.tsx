// A-16: the logins a DEVICE holds, as ONE line of chips beside its name
// (`fixtures/devices-list.json`): brand mark + label (+ ` · plan`). A pointer
// previews the login's limits on hover (the account picker's preview,
// UsageWindows `hover`); a click — and a tap on touch — opens the account
// menu with the same bars at its top. Team devices are READ-ONLY: their chips
// preview and never open a menu. Health is NOT written here: the row's
// attention icon carries it.
//
// The ⋯ menu is the `AccountChipMenu` (Sign in / Sign out / Remove account,
// same gating). Hand-mirrored ×4 (desktop `machines.rs`, iOS
// `DeviceLogins.swift`, Android `AgentsScreen`); the ordering, the label and
// the empty strings live in `lib/agent-usage.ts` + `devices-list.json`.
// EXP-1199: the MCP Apps devices view (`packages/mcp-apps` `devices-view.tsx`)
// mirrors Sign in + Add account through `exponential_devices_account_login`.
import { useMemo, useState } from "react"
import {
  AgentMark,
  conceptIcon,
  Button,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Pill,
  UsageWindows,
  useIsMobile,
} from "@exp/ui"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"
import {
  deviceLoginRows,
  loginLabel,
  sortDeviceLogins,
  usageAge,
  usageState,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceIsMine,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { addableAgents } from "@/lib/agent-account-add"
import { AddAccountDialog } from "@/components/add-account-dialog"
import { miniUsageWindows } from "@/components/agent-usage-windows"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  AccountChipMenu,
  accountChipActionable,
} from "@/components/device-agent-account"

const AddIcon = conceptIcon(`ui-add`)

/** The device's logins in the row's order (`sortDeviceLogins`). */
export function useDeviceLogins(device: SteerDevice): AgentProfileUsageRow[] {
  return useMemo(
    () =>
      sortDeviceLogins(
        deviceLoginRows(
          {
            deviceId: device.deviceId,
            deviceLabel: device.deviceLabel,
            agentAccounts: device.agentAccounts,
            agentUsage: device.agentUsage,
          },
          { mine: deviceIsMine(device), online: deviceIsOnline(device) }
        )
      ),
    [device]
  )
}

/** The chips of ONE device row. `readOnly` is the team-devices arm. */
export function DeviceLoginChips({
  device,
  rows,
  now,
  readOnly = false,
}: {
  device: SteerDevice
  rows: readonly AgentProfileUsageRow[]
  now: Date
  readOnly?: boolean
}) {
  // EXP-845 REVERSED (EXP-909): "Add account" only where a sign-in can be
  // queued — my machine, online, with an agent installed to sign in to.
  const canAdd =
    !readOnly &&
    deviceIsMine(device) &&
    deviceIsOnline(device) &&
    addableAgents(device).length > 0
  return (
    <div className="flex min-w-0 flex-wrap items-center gap-1.5">
      {device.agentAccounts === undefined ? (
        // Never reported yet — not "no login".
        <span className="text-xs text-muted-foreground">{DEVICES_LIST_COPY.checking}</span>
      ) : rows.length === 0 ? (
        <span className="text-xs text-muted-foreground">{DEVICES_LIST_COPY.noAccount}</span>
      ) : (
        rows.map((row) => (
          <DeviceLoginChip
            key={row.key}
            device={device}
            row={row}
            now={now}
            readOnly={readOnly}
          />
        ))
      )}
      {canAdd && <AddAccountChip device={device} />}
    </div>
  )
}

/** The login's limits, the account picker's preview density. */
function LoginLimits({ row, now }: { row: AgentProfileUsageRow; now: Date }) {
  if (usageState(row) !== `ready`) return null
  return (
    <UsageWindows
      windows={miniUsageWindows(row.usage)}
      density="hover"
      stale={usageAge(row.usage, now) !== null}
      className="w-40"
    />
  )
}

function DeviceLoginChip({
  device,
  row,
  now,
  readOnly,
}: {
  device: SteerDevice
  row: AgentProfileUsageRow
  now: Date
  readOnly: boolean
}) {
  const touch = useIsMobile()
  const label = loginLabel(row)
  const actionable = !readOnly && accountChipActionable(device)
  const limits = usageState(row) === `ready`
  const chip = (
    <Pill
      mode={actionable ? `action` : `readonly`}
      leading={<AgentMark agent={row.agent} className="size-3.5" />}
      aria-label={actionable ? `Account menu for ${label} on ${device.deviceLabel || device.deviceId}` : undefined}
      data-testid={`device-login-chip-${row.key}`}
      className="max-w-full min-w-0"
      onClick={(event) => event.stopPropagation()}
    >
      <span className="min-w-0 truncate" title={label}>
        {label}
        {/* The plan only alongside an email — it IS the label otherwise. */}
        {row.email && row.plan && (
          <span className="text-muted-foreground/60">{` · ${row.plan}`}</span>
        )}
      </span>
    </Pill>
  )
  const menu = actionable ? (
    <AccountChipMenu
      device={device}
      row={row}
      accountLabel={label}
      onSignIn={() =>
        requestAgentLogin({ device, agent: row.agent, profileId: row.profileId })
      }
      header={limits ? <LoginLimits row={row} now={now} /> : undefined}
      trigger={chip}
    />
  ) : (
    chip
  )
  if (touch || !limits) return menu
  return (
    <HoverCard openDelay={150} closeDelay={80}>
      <HoverCardTrigger asChild>
        <span className="inline-flex min-w-0">{menu}</span>
      </HoverCardTrigger>
      <HoverCardContent
        side="bottom"
        align="start"
        sideOffset={6}
        data-slot="account-limits-preview"
        className="w-auto p-2"
      >
        <LoginLimits row={row} now={now} />
      </HoverCardContent>
    </HoverCard>
  )
}

/** The device-bound "Add account" ghost `+` — the sign-in runs HERE. */
function AddAccountChip({ device }: { device: SteerDevice }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        variant="ghost"
        size="icon-sm"
        className="size-6 text-muted-foreground"
        aria-label={DEVICES_LIST_COPY.addAccount}
        title={DEVICES_LIST_COPY.addAccount}
        onClick={(event) => {
          event.stopPropagation()
          setOpen(true)
        }}
        data-testid={`add-account-${device.deviceId}`}
      >
        <AddIcon className="size-3.5" />
      </Button>
      <AddAccountDialog open={open} onOpenChange={setOpen} device={device} />
    </>
  )
}
