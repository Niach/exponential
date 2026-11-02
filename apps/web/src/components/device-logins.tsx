// EXP-1560 (was A-16): the logins a DEVICE CARD holds, as ONE row of brand-mark
// ICONS (`fixtures/devices-list.json`): no email on the card, THE run-mark
// amber badge on a login that needs attention (dead, signed out, at its
// limit). A pointer previews the login on hover (email, plan, its status,
// the usage windows, Sign in when it has one); a click — and a tap on touch —
// opens the account menu headed by the same preview. Team devices are
// READ-ONLY: their icons preview and never open a menu. More than
// `card.maxAccountIcons` logins fold behind a ⋯ that reveals them in place.
//
// The menu is the `AccountChipMenu` (Sign in / Sign out / Remove account,
// same gating). Hand-mirrored ×4 (desktop `machines.rs`, iOS
// `DeviceLogins.swift`, Android `AgentsScreen`); the ordering, the label and
// the empty strings live in `lib/agent-usage.ts` + `devices-list.json`.
// EXP-1199: the MCP Apps devices view (`packages/mcp-apps` `devices-view.tsx`)
// mirrors Sign in + Add account through `exponential_devices_account_login`.
import { useMemo, useState } from "react"
import {
  AgentRunMark,
  conceptIcon,
  Button,
  HoverCard,
  HoverCardContent,
  HoverCardTrigger,
  Pill,
  Popover,
  PopoverContent,
  PopoverTrigger,
  UsageWindows,
  useIsMobile,
} from "@exp/ui"
import {
  DEVICES_LIST_COPY,
  deviceCardAccountIcons,
  type DeviceAttentionItem,
} from "@exp/domain-contract/devices-list"
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
  AccountChipMenu,
  accountChipActionable,
  persistLoginTarget,
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

/** The account icons of ONE device card. `readOnly` is the team-devices arm. */
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
  const [expanded, setExpanded] = useState(false)
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
  return (
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
            now={now}
            readOnly={readOnly}
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
          onClick={() => setExpanded(true)}
        >
          <MoreIcon className="size-4" />
        </Button>
      )}
      {canAdd && <AddAccountChip device={device} />}
    </div>
  )
}

/** The hover card pill a login item's action reads (devices-list.json):
 *  `sign_in` (the browser login), or EXP-1626's renewal of a persisted
 *  claude login (the persist sign-in for that profile). */
export function loginActionLabel(item: LoginAttention): string {
  return item.action === `keep_signed_in`
    ? DEVICES_LIST_COPY.renewPersistedLogin
    : DEVICES_LIST_COPY.signIn
}

/** The login's preview: email (else the label), the plan, the persisted
 *  line (EXP-1626 `persistedLine`), its status, the usage windows — the
 *  hover card's body and the account menu's header. */
function LoginPreview({
  row,
  item,
  now,
  onSignIn,
}: {
  row: AgentProfileUsageRow
  item: LoginAttention | null
  now: Date
  /** The hover card's trailing Sign in pill (only when the item has one). */
  onSignIn?: () => void
}) {
  const ready = usageState(row) === `ready`
  const persisted = persistedLine(row)
  return (
    <div data-slot="login-preview" className="flex w-60 flex-col gap-2">
      <div className="flex min-w-0 flex-col">
        <span className="truncate text-sm font-medium">{row.email || loginLabel(row)}</span>
        {row.email && row.plan && (
          <span className="truncate text-xs text-muted-foreground">{row.plan}</span>
        )}
        {persisted && (
          <span data-slot="login-persisted" className="text-xs text-muted-foreground">
            {persisted}
          </span>
        )}
        {item && (
          <span className="text-xs text-amber-500">{DEVICES_LIST_COPY.status[item.status]}</span>
        )}
      </div>
      {ready && row.usage && (
        <UsageWindows
          windows={fullUsageWindows(row.usage, now)}
          density="full"
          stale={usageAge(row.usage, now) !== null}
        />
      )}
      {onSignIn && item?.action && (
        <Pill mode="action" className="self-start" onClick={onSignIn}>
          {loginActionLabel(item)}
        </Pill>
      )}
    </div>
  )
}

function DeviceAccountIcon({
  device,
  row,
  item,
  now,
  readOnly,
}: {
  device: SteerDevice
  row: AgentProfileUsageRow
  item: LoginAttention | null
  now: Date
  readOnly: boolean
}) {
  const touch = useIsMobile()
  const label = loginLabel(row)
  const actionable = !readOnly && accountChipActionable(device)
  const signIn = () =>
    requestAgentLogin({ device, agent: row.agent, profileId: row.profileId })
  // EXP-1626: Persist login / Renew persisted login: `claude setup-token`
  // attached to THIS profile (its browser login is never touched).
  const persistLogin = () =>
    requestAgentLogin({ device, agent: row.agent, ...persistLoginTarget(row) })
  const icon = (
    <Button
      variant="ghost"
      size="icon-sm"
      className="size-7"
      aria-label={`${label} on ${device.deviceLabel || device.deviceId}`}
      data-testid={`device-login-chip-${row.key}`}
      data-attention={item ? item.status : undefined}
      onClick={(event) => event.stopPropagation()}
    >
      <AgentRunMark
        agent={row.agent}
        state={item ? `needs_input` : undefined}
        ringClassName="ring-glass-card-opaque"
        className="size-4 [&>svg]:size-4"
      />
    </Button>
  )
  const preview = <LoginPreview row={row} item={item} now={now} />
  if (actionable) {
    const menu = (
      <AccountChipMenu
        device={device}
        row={row}
        accountLabel={label}
        onSignIn={signIn}
        onPersistLogin={persistLogin}
        header={preview}
        trigger={icon}
      />
    )
    if (touch) return menu
    return (
      <HoverCard openDelay={150} closeDelay={80}>
        <HoverCardTrigger asChild>
          <span className="inline-flex">{menu}</span>
        </HoverCardTrigger>
        <HoverCardContent
          side="bottom"
          align="start"
          sideOffset={6}
          data-slot="account-limits-preview"
          className="w-auto p-3"
        >
          <LoginPreview
            row={row}
            item={item}
            now={now}
            onSignIn={() =>
              item?.action === `keep_signed_in` ? persistLogin() : signIn()
            }
          />
        </HoverCardContent>
      </HoverCard>
    )
  }
  // Read-only: the preview on hover (pointer), and on a tap (touch).
  if (touch) {
    return (
      <Popover>
        <PopoverTrigger asChild>{icon}</PopoverTrigger>
        <PopoverContent side="bottom" align="start" sideOffset={6} className="w-auto p-3">
          {preview}
        </PopoverContent>
      </Popover>
    )
  }
  return (
    <HoverCard openDelay={150} closeDelay={80}>
      <HoverCardTrigger asChild>{icon}</HoverCardTrigger>
      <HoverCardContent
        side="bottom"
        align="start"
        sideOffset={6}
        data-slot="account-limits-preview"
        className="w-auto p-3"
      >
        {preview}
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
