// EXP-1560 (was A-16): the logins a DEVICE CARD holds, as ONE row of brand-mark
// ICONS (`fixtures/devices-list.json`), THE run-mark amber badge on a login
// that needs attention (dead, signed out, at its limit). #EXP-1643: a press
// expands the card with THE account panel (email, plan, its status, the usage
// windows, the actions), remembered per device on this client; a pointer's
// hover is only the tooltip with the email. Team devices are READ-ONLY: the
// same panel without actions. More than `card.maxAccountIcons` logins fold
// behind a ⋯ that reveals them in place.
//
// Hand-mirrored ×4 (desktop `machines.rs`, iOS
// `DeviceLogins.swift`, Android `AgentsScreen`); the ordering, the label and
// the empty strings live in `lib/agent-usage.ts` + `devices-list.json`.
// EXP-1199: the MCP Apps devices view (`packages/mcp-apps` `devices-view.tsx`)
// mirrors Sign in + Add account through `exponential_devices_account_login`.
import { useMemo, useState } from "react"
import {
  AgentRunMark,
  conceptIcon,
  Button,
  DeviceAccountPanel,
} from "@exp/ui"
import {
  DEVICE_CARD,
  DEVICES_LIST_COPY,
  deviceCardAccountIcons,
  openDeviceAccount,
  type DeviceAttentionItem,
} from "@exp/domain-contract/devices-list"
import { cn } from "@/lib/utils"
import {
  deviceLoginRows,
  loginLabel,
  peakPercent,
  persistedLine,
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
import { fullUsageWindows } from "@/components/agent-usage-windows"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  accountChipActionable,
  persistLoginTarget,
  useAccountActions,
} from "@/components/device-agent-account"

const AddIcon = conceptIcon(`ui-add`)
const MoreIcon = conceptIcon(`ui-more`)

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

/** EXP-1560: a login at its limit — a FRESH report with a window at 100%. */
export function loginLimitHit(row: AgentProfileUsageRow, now: Date): boolean {
  return (
    usageState(row) === `ready` &&
    usageAge(row.usage, now) === null &&
    peakPercent(row.usage) >= 100
  )
}

/** The attention a login wears (its `login` item from `deviceListRow`). */
export type LoginAttention = Extract<DeviceAttentionItem, { kind: `login` }>

// #EXP-1643: the open account per device, on this client only.
const OPEN_ACCOUNTS_KEY = `exp:device-account-open`

function readOpenAccounts(): Record<string, string | null> {
  try {
    return JSON.parse(localStorage.getItem(OPEN_ACCOUNTS_KEY) ?? `{}`) as Record<string, string | null>
  } catch {
    return {}
  }
}

/** The card's open login (`openDeviceAccount`) and its toggle, remembered. */
function useOpenAccount(deviceId: string, loginKeys: readonly string[]) {
  const [remembered, setRemembered] = useState<string | null | undefined>(() => readOpenAccounts()[deviceId])
  const open = openDeviceAccount(loginKeys, remembered)
  const toggle = (key: string) => {
    const next = open === key ? null : key
    setRemembered(next)
    try {
      localStorage.setItem(OPEN_ACCOUNTS_KEY, JSON.stringify({ ...readOpenAccounts(), [deviceId]: next }))
    } catch {
      // Storage refused (private mode): it stays open for this visit only.
    }
  }
  return { open, toggle }
}

/** The account icons of ONE device card and the open account's panel.
 *  `readOnly` is the team-devices arm. */
export function DeviceAccountIcons({
  device,
  rows,
  attention,
  now,
  readOnly = false,
}: {
  device: SteerDevice
  rows: readonly AgentProfileUsageRow[]
  attention: readonly DeviceAttentionItem[]
  now: Date
  readOnly?: boolean
}) {
  const [revealed, setRevealed] = useState(false)
  const { open, toggle } = useOpenAccount(device.deviceId, rows.map((row) => row.key))
  // A login folded behind the ⋯ that is open reveals the fold.
  const openIndex = rows.findIndex((row) => row.key === open)
  const expanded = revealed || openIndex >= deviceCardAccountIcons(rows.length, false).visible
  // EXP-845 REVERSED (EXP-909): "Add account" only where a sign-in can be
  // queued — my machine, online, with an agent installed to sign in to.
  const canAdd =
    !readOnly &&
    deviceIsMine(device) &&
    deviceIsOnline(device) &&
    addableAgents(device).length > 0
  const icons = deviceCardAccountIcons(rows.length, expanded)
  const itemFor = (row: AgentProfileUsageRow) =>
    attention.find(
      (item): item is LoginAttention =>
        item.kind === `login` && item.agent === row.agent && item.profileId === row.profileId
    ) ?? null
  const openRow = openIndex >= 0 ? rows[openIndex]! : null
  return (
    <div className="flex min-w-0 flex-col" style={{ gap: DEVICE_CARD.panelGap }}>
      <div className="flex min-w-0 flex-wrap items-center gap-1">
        {device.agentAccounts === undefined ? (
          // Never reported yet — not "no login".
          <span className="text-xs text-muted-foreground">{DEVICES_LIST_COPY.checking}</span>
        ) : rows.length === 0 ? (
          <span className="text-xs text-muted-foreground">{DEVICES_LIST_COPY.noAccount}</span>
        ) : (
          rows.slice(0, icons.visible).map((row) => (
            <DeviceAccountIcon
              key={row.key}
              device={device}
              row={row}
              item={itemFor(row)}
              open={row.key === open}
              onToggle={() => toggle(row.key)}
            />
          ))
        )}
        {icons.more && (
          <Button
            variant="ghost"
            size="icon-sm"
            className="size-7 text-muted-foreground"
            aria-label={DEVICES_LIST_COPY.moreAccounts}
            title={DEVICES_LIST_COPY.moreAccounts}
            onClick={() => setRevealed(true)}
          >
            <MoreIcon className="size-4" />
          </Button>
        )}
        {canAdd && <AddAccountChip device={device} />}
      </div>
      {openRow && (
        <LoginPanel
          key={openRow.key}
          device={device}
          row={openRow}
          item={itemFor(openRow)}
          now={now}
          actionable={!readOnly && accountChipActionable(device)}
        />
      )}
    </div>
  )
}

/** #EXP-1643: the open login's panel. */
function LoginPanel({
  device,
  row,
  item,
  now,
  actionable,
}: {
  device: SteerDevice
  row: AgentProfileUsageRow
  item: LoginAttention | null
  now: Date
  actionable: boolean
}) {
  const ready = usageState(row) === `ready`
  const actions = useAccountActions({
    device,
    row,
    accountLabel: loginLabel(row),
    onSignIn: () => requestAgentLogin({ device, agent: row.agent, profileId: row.profileId }),
    // EXP-1626: `claude setup-token` attached to THIS profile (its browser
    // login is never touched).
    onPersistLogin: () => requestAgentLogin({ device, agent: row.agent, ...persistLoginTarget(row) }),
  })
  return (
    <DeviceAccountPanel
      name={row.email || loginLabel(row)}
      plan={row.email ? row.plan : null}
      persisted={persistedLine(row)}
      status={item ? DEVICES_LIST_COPY.status[item.status] : null}
      windows={ready && row.usage ? fullUsageWindows(row.usage, now) : []}
      stale={ready && row.usage ? usageAge(row.usage, now) !== null : false}
      busy={actions.busy}
      {...(actionable
        ? { signIn: actions.signIn, persist: actions.persist, signOut: actions.signOut, remove: actions.remove }
        : {})}
    >
      {actionable && actions.confirms}
    </DeviceAccountPanel>
  )
}

function DeviceAccountIcon({
  device,
  row,
  item,
  open,
  onToggle,
}: {
  device: SteerDevice
  row: AgentProfileUsageRow
  item: LoginAttention | null
  open: boolean
  onToggle: () => void
}) {
  const label = loginLabel(row)
  return (
    <Button
      variant="ghost"
      size="icon-sm"
      className={cn(`size-7`, open && `bg-glass-active hover:bg-glass-active`)}
      aria-label={`${label} on ${device.deviceLabel || device.deviceId}`}
      aria-expanded={open}
      title={row.email || label}
      data-testid={`device-login-chip-${row.key}`}
      data-attention={item ? item.status : undefined}
      onClick={onToggle}
    >
      <AgentRunMark
        agent={row.agent}
        state={item ? `needs_input` : undefined}
        ringClassName="ring-glass-card-opaque"
        className="size-4 [&>svg]:size-4"
      />
    </Button>
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
        className="size-7 text-muted-foreground"
        aria-label={DEVICES_LIST_COPY.addAccount}
        title={DEVICES_LIST_COPY.addAccount}
        onClick={(event) => {
          event.stopPropagation()
          setOpen(true)
        }}
        data-testid={`add-account-${device.deviceId}`}
      >
        <AddIcon className="size-4" />
      </Button>
      <AddAccountDialog open={open} onOpenChange={setOpen} device={device} />
    </>
  )
}
