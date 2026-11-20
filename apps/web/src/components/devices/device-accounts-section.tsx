import { useCallback, useState } from "react"
import { DEVICES_LIST_COPY, deviceListRow } from "@exp/domain-contract/devices-list"
import {
  agentLabel,
  AgentRunMark,
  conceptIcon,
  GlassSectionHeader,
  ListRow,
  Pill,
  SETTINGS_LIST_CLASS,
} from "@exp/ui"
import { deviceIsMine, deviceIsOnline, deviceUpdateAvailable, type SteerDevice } from "@/lib/steer-devices"
import { loginLabel, usageAge, usageState, type AgentProfileUsageRow } from "@/lib/agent-usage"
import { addableAgents } from "@/lib/agent-account-add"
import { AddAccountDialog } from "@/components/add-account-dialog"
import { AgentUsageWindows } from "@/components/agent-usage-windows"
import { accountChipActionable } from "@/components/device-agent-account"
import { LoginPanel, loginLimitHit, useDeviceLogins, type LoginAttention } from "@/components/device-logins"
import { useAgentUsageRefresh } from "@/hooks/use-agent-usage-refresh"

// #EXP-2027 (sidebar-tree.json DEVICES): the device page's Accounts list, on
// the viewer's own machine: one row per login (its brand mark with THE
// attention badge, the email, `<agent> · <plan>`, the mini usage windows),
// a press opens THE account panel under it (devices-list.json, the card's
// panel: status, every window, the actions); `Add account` = the existing
// add-account flow, offered where a sign-in can be queued.

const AddIcon = conceptIcon(`ui-add`)

export const DEVICE_ACCOUNTS_LABEL = `Accounts`

/** A login row's second line: `Claude Code · Max` (no plan = the agent). */
export function accountSubtitle(row: Pick<AgentProfileUsageRow, `agent` | `plan` | `email`>): string {
  const agent = agentLabel(row.agent)
  return row.email && row.plan ? `${agent} · ${row.plan}` : agent
}

export function DeviceAccountsSection({
  device,
  now,
  latestVersions,
}: {
  device: SteerDevice
  now: Date
  latestVersions: { desktop: string | null; cli: string | null } | null
}) {
  const logins = useDeviceLogins(device)
  const [adding, setAdding] = useState(false)
  const [openKey, setOpenKey] = useState<string | null>(null)
  // EXP-909: the 30 s usage refresh the device list ran, for this machine.
  const canRefresh = useCallback((row: AgentProfileUsageRow) => row.online, [])
  useAgentUsageRefresh(logins, canRefresh, now)

  const online = deviceIsOnline(device)
  const mine = deviceIsMine(device)
  const latest = (device.kind === `server` ? latestVersions?.cli : latestVersions?.desktop) ?? null
  const { attention } = deviceListRow({
    online,
    mine,
    server: device.kind === `server`,
    registered: device.registered === true,
    outdated: deviceUpdateAvailable(device.version, latest),
    latest,
    updateQueued: Boolean(device.updateRequested && device.updateBlocked),
    accountsReported: device.agentAccounts !== undefined,
    logins: logins.map((login) => ({
      agent: login.agent,
      profileId: login.profileId,
      label: loginLabel(login),
      health: login.health,
      limitHit: loginLimitHit(login, now),
      tokenExpiresAt: login.tokenExpiresAt,
    })),
    now: now.toISOString(),
  })
  const itemFor = (row: AgentProfileUsageRow) =>
    attention.find(
      (item): item is LoginAttention =>
        item.kind === `login` && item.agent === row.agent && item.profileId === row.profileId
    ) ?? null
  const canAdd = mine && online && addableAgents(device).length > 0

  return (
    <section data-testid="device-accounts">
      <GlassSectionHeader
        label={DEVICE_ACCOUNTS_LABEL}
        trailing={
          canAdd ? (
            <Pill mode="action" onClick={() => setAdding(true)} data-testid={`add-account-${device.deviceId}`}>
              <AddIcon className="size-3" />
              {DEVICES_LIST_COPY.addAccount}
            </Pill>
          ) : undefined
        }
      />
      <div className={`${SETTINGS_LIST_CLASS} pt-1`}>
        {device.agentAccounts === undefined || logins.length === 0 ? (
          <div className="px-3 py-2 text-sm text-muted-foreground">
            {device.agentAccounts === undefined ? DEVICES_LIST_COPY.checking : DEVICES_LIST_COPY.noAccount}
          </div>
        ) : (
          logins.map((row) => (
            <AccountRow
              key={row.key}
              device={device}
              row={row}
              item={itemFor(row)}
              now={now}
              open={openKey === row.key}
              onToggle={() => setOpenKey((key) => (key === row.key ? null : row.key))}
            />
          ))
        )}
      </div>
      <AddAccountDialog open={adding} onOpenChange={setAdding} device={device} />
    </section>
  )
}

function AccountRow({
  device,
  row,
  item,
  now,
  open,
  onToggle,
}: {
  device: SteerDevice
  row: AgentProfileUsageRow
  item: LoginAttention | null
  now: Date
  open: boolean
  onToggle: () => void
}) {
  const ready = usageState(row) === `ready`
  const stale = ready && row.usage ? usageAge(row.usage, now) !== null : false
  return (
    <div className="flex flex-col">
      <ListRow
        interactive
        active={open}
        onClick={onToggle}
        aria-expanded={open}
        className="gap-3 px-3 py-2"
        data-testid={`device-account-${row.key}`}
        data-attention={item ? item.status : undefined}
      >
        <AgentRunMark
          agent={row.agent}
          state={item ? `needs_input` : undefined}
          ringClassName="ring-background"
          className="size-4 shrink-0 [&>svg]:size-4"
        />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm">{row.email || loginLabel(row)}</div>
          <div className="truncate text-xs text-muted-foreground">
            {item ? `${accountSubtitle(row)} · ${DEVICES_LIST_COPY.status[item.status]}` : accountSubtitle(row)}
          </div>
        </div>
        {ready && row.usage && (
          <AgentUsageWindows usage={row.usage} now={now} density="mini" stale={stale} className="shrink-0" />
        )}
      </ListRow>
      {open && (
        <div className="px-3 pb-3">
          <LoginPanel device={device} row={row} item={item} now={now} actionable={accountChipActionable(device)} />
        </div>
      )}
    </div>
  )
}
