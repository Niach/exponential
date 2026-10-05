import { useState } from "react"
import {
  AgentMark,
  EmptyState,
  GlassSectionHeader,
  ListRow,
  LiveDot,
  Meter,
  cn,
  conceptIcon,
  getDeviceIcon,
} from "@exp/ui"
import type { DeviceRow } from "./model"
import {
  DEVICE_AGENT_LABEL,
  NO_LOGIN_REPORTED,
  deviceHasRunnableAgent,
  deviceLogins,
  deviceStatusText,
  deviceWorstHealth,
  healthBadgeLabel,
  loginName,
  miniWindows,
  platformLabel,
  severity,
  usageAge,
  usageState,
  windowReset,
  type DeviceListRow,
  type DeviceLogin,
} from "./device-usage"
import { ago } from "./list-session"

const DevicesIcon = conceptIcon(`nav-devices`)
const DefaultIcon = conceptIcon(`ui-device-default`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

// EXP-1183 — `exponential_devices_list` as the app's Devices page
// (`my-machines.tsx` + `device-logins.tsx`): "My devices" and "Team devices"
// bands over flat rows — the device glyph, name + version (+ default star,
// Shared, the worst login's health chip), Online / last seen, platform and
// the agents it can run; a row folds open to the logins it holds, each with
// its 5h / week / model bars. READ-ONLY: MCP has no account tools (sign-in,
// add, remove stay on the machine's own settings), so no row carries a
// control and there is no "Add account".
export function DevicesView({ devices }: { devices: readonly DeviceRow[] }) {
  const rows = devices as readonly unknown[] as readonly DeviceListRow[]
  const [folded, setFolded] = useState<ReadonlySet<string>>(new Set())
  if (rows.length === 0) {
    return (
      <EmptyState
        icon={DevicesIcon}
        title="No devices"
        description="Open the Exponential desktop app, or install the CLI daemon on a server."
      />
    )
  }
  const mine = rows.filter((device) => !device.owner)
  const team = rows.filter((device) => device.owner)
  const toggle = (id: string) =>
    setFolded((current) => {
      const next = new Set(current)
      if (!next.delete(id)) next.add(id)
      return next
    })
  const band = (label: string, list: readonly DeviceListRow[]) =>
    list.length > 0 && (
      <section className="flex flex-col">
        <GlassSectionHeader label={label} count={list.length} />
        {list.map((device) => (
          <DeviceItem
            key={device.deviceId}
            device={device}
            expanded={!folded.has(device.deviceId)}
            onToggle={() => toggle(device.deviceId)}
          />
        ))}
      </section>
    )
  return (
    <div className="flex flex-col gap-3 p-2">
      {band(`My devices`, mine)}
      {band(`Team devices`, team)}
    </div>
  )
}

function DeviceItem({
  device,
  expanded,
  onToggle,
}: {
  device: DeviceListRow
  expanded: boolean
  onToggle: () => void
}) {
  const logins = deviceLogins(device)
  const health = healthBadgeLabel(deviceWorstHealth(logins) ?? `ok`)
  const KindIcon = getDeviceIcon({ kind: device.kind })
  const name = device.label || device.deviceId
  const platform = platformLabel(device.platform)
  const agents = device.agents ?? []
  const unauthed = (device.unauthedAgents ?? []).filter((agent) => !agents.includes(agent))
  const dim = device.online && !deviceHasRunnableAgent(device)
  return (
    <div
      className={cn(`flex flex-col`, dim && `opacity-60`)}
      data-testid={`device-row-${device.deviceId}`}
    >
      <ListRow
        interactive
        aria-expanded={expanded}
        onClick={onToggle}
        className="gap-2"
        title={device.owner ? `Shared by ${device.owner.name}` : undefined}
      >
        <span aria-hidden className="flex shrink-0 items-center text-muted-foreground">
          {expanded ? (
            <ChevronDownIcon className="size-3" />
          ) : (
            <ChevronRightIcon className="size-3" />
          )}
        </span>
        <KindIcon className="size-4 shrink-0 text-foreground/70" />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium">{name}</span>
            {device.version && (
              <span className="shrink-0 text-[10px] text-muted-foreground/60">
                v{device.version}
              </span>
            )}
            {device.isDefault && (
              <span
                className="shrink-0 text-muted-foreground"
                title="Your default device"
                aria-label="Default device"
              >
                <DefaultIcon className="size-3 fill-current" />
              </span>
            )}
            {!device.owner && (device.sharedTeamIds?.length ?? 0) > 0 && (
              <span className="shrink-0 rounded-sm border border-border/60 px-1 text-[10px] text-muted-foreground">
                Shared
              </span>
            )}
            {health && (
              <span className="shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] font-medium text-amber-500">
                {health}
              </span>
            )}
          </div>
          <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
            {device.online && <LiveDot tone="live" className="size-1.5 shrink-0" />}
            <span className="truncate">
              {[deviceStatusText(device), platform].filter(Boolean).join(` · `)}
            </span>
          </div>
        </div>
        {(agents.length > 0 || unauthed.length > 0) && (
          <span className="flex shrink-0 items-center gap-1.5">
            {agents.map((agent) => (
              <span key={agent} title={DEVICE_AGENT_LABEL[agent] ?? agent}>
                <AgentMark agent={agent} className="size-3.5 text-foreground/80" />
              </span>
            ))}
            {unauthed.map((agent) => (
              <span
                key={agent}
                title={`${DEVICE_AGENT_LABEL[agent] ?? agent} · signed out`}
                className="opacity-40"
              >
                <AgentMark agent={agent} className="size-3.5" />
              </span>
            ))}
          </span>
        )}
      </ListRow>
      {expanded && (
        <div className="flex flex-col gap-1.5 pr-3 pb-2 pl-14">
          {device.agentAccounts == null ? (
            <p className="text-[11px] text-muted-foreground">Checking…</p>
          ) : logins.length === 0 ? (
            <p className="text-[11px] text-muted-foreground">{NO_LOGIN_REPORTED}</p>
          ) : (
            logins.map((login) => <LoginRow key={login.key} login={login} />)
          )}
        </div>
      )}
    </div>
  )
}

/** One login: identity (+ plan) and health, then its numbers. */
function LoginRow({ login }: { login: DeviceLogin }) {
  const name = loginName(login)
  const health = healthBadgeLabel(login.health)
  const state = usageState(login)
  const age = usageAge(login.usage)
  const asOf = ago(login.usage?.fetchedAt ?? login.checkedAt)
  return (
    <div className="flex min-w-0 flex-col gap-0.5" data-testid={`device-login-${login.key}`}>
      <div className="flex min-w-0 items-center gap-1.5">
        <AgentMark agent={login.agent} className="size-3.5 text-foreground/70" />
        <span className="min-w-0 truncate text-xs" title={name}>
          {name}
          {login.email && login.plan && (
            <span className="text-muted-foreground/60">{` · ${login.plan}`}</span>
          )}
        </span>
        {health && (
          <span className="shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] font-medium text-amber-500">
            {health}
          </span>
        )}
      </div>
      <div className="flex min-w-0 items-start gap-2 pl-5">
        {state === `ready` ? (
          <>
            <UsageBars login={login} className={cn(`min-w-0 flex-1`, age && `opacity-50`)} />
            {age && (
              <span className="shrink-0 text-[10px] text-muted-foreground opacity-50">{age}</span>
            )}
          </>
        ) : state === `checking` ? (
          <span className="text-[10px] text-muted-foreground">Checking…</span>
        ) : state === `unmonitored` ? (
          <span className="text-[10px] text-muted-foreground">Usage not monitored</span>
        ) : (
          <span className="text-[10px] text-muted-foreground">
            {asOf ? `No usage reported · as of ${asOf}` : `No usage reported`}
          </span>
        )}
      </div>
    </div>
  )
}

/** `agent-usage-mini.tsx`: up to three `label · meter · NN%` windows. */
function UsageBars({ login, className }: { login: DeviceLogin; className?: string }) {
  return (
    <div className={cn(`flex min-w-0 items-start gap-3`, className)}>
      {miniWindows(login.usage).map((window) => {
        const reset = windowReset(window)
        return (
          <div key={window.key} className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="flex min-w-0 items-center gap-1.5">
              <span className="shrink-0 text-[10px] text-muted-foreground">{window.label}</span>
              <Meter
                value={window.percent}
                tone={severity(window.percent)}
                className="h-1 min-w-4 flex-1"
              />
              <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                {`${window.percent}%`}
              </span>
            </div>
            {reset && (
              <span className="truncate text-center text-[10px] text-muted-foreground/60">
                {reset}
              </span>
            )}
          </div>
        )
      })}
    </div>
  )
}
