import { useEffect, useState } from "react"
import {
  AgentRunMark,
  Button,
  DeviceAccountPanel,
  EmptyState,
  GlassCard,
  GlassSectionHeader,
  LiveDot,
  cn,
  conceptIcon,
  getDeviceIcon,
  type UsageWindow,
} from "@exp/ui"
import {
  DEVICE_CARD,
  DEVICES_LIST_COPY,
  deviceCardAccountIcons,
  deviceListRow,
  type DeviceAttentionItem,
} from "@exp/domain-contract/devices-list"
import type { DeviceRow } from "./model"
import {
  addableAgents,
  canSignInOn,
  deviceHasRunnableAgent,
  deviceLogins,
  deviceStatusText,
  loginName,
  loginSignsIn,
  severity,
  usageAge,
  usageState,
  windowReset,
  type DeviceListRow,
  type DeviceLogin,
} from "./device-usage"
import { AccountLoginFlow, AddAccountRow } from "./device-account-login"

const DevicesIcon = conceptIcon(`nav-devices`)
const DefaultIcon = conceptIcon(`ui-device-default`)
const AddIcon = conceptIcon(`ui-add`)
const MoreIcon = conceptIcon(`ui-more`)

// EXP-1183 → #VAPP-292 — `exponential_devices {op: "list"}` as the app's
// Devices page (web `my-machines.tsx` `DeviceGroups` + `DeviceCard`,
// `device-logins.tsx` `DeviceAccountIcons`, fixture `devices-list.json`):
// "My devices" and "Team devices" bands over a grid of device cards, online
// first, then the muted "Offline" label over the offline ones. A card is the
// device glyph, name + version (+ default star), Online / last seen (+
// Shared), and ONE row of brand-mark icons, a login needing attention wearing
// the amber run-mark badge. Every card starts collapsed: a press on an icon
// opens THAT login's panel (`DeviceAccountPanel`: email, plan, its status,
// the usage windows). EXP-1199: your own online machines take a sign-in
// (`exponential_devices {op: "account_login"}`): the panel's Sign in on a
// dead login, the `+` for another account. Sign out, remove and the device
// settings stay on the clients.
export function DevicesView({ devices }: { devices: readonly DeviceRow[] }) {
  const [rows, setRows] = useState<readonly DeviceListRow[]>(
    () => devices as readonly unknown[] as readonly DeviceListRow[]
  )
  useEffect(() => {
    setRows(devices as readonly unknown[] as readonly DeviceListRow[])
  }, [devices])
  // A sign-in re-reads YOUR machines: patch those rows, keep the rest.
  const refresh = (fresh: readonly DeviceListRow[]) =>
    setRows((current) =>
      current.map((row) => fresh.find((next) => next.deviceId === row.deviceId) ?? row)
    )
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
  const band = (label: string, list: readonly DeviceListRow[]) =>
    list.length > 0 && (
      <div className="flex flex-col">
        <GlassSectionHeader label={label} count={list.length} />
        <DeviceGroups devices={list} onDevices={refresh} />
      </div>
    )
  return (
    <div className="flex flex-col gap-3 p-2">
      {band(DEVICES_LIST_COPY.mine, mine)}
      {band(DEVICES_LIST_COPY.team, team)}
    </div>
  )
}

/** EXP-1560: online cards, then "Offline" over the rest, auto-filled
 *  columns at `card.minWidth`. */
function DeviceGroups({
  devices,
  onDevices,
}: {
  devices: readonly DeviceListRow[]
  onDevices: (rows: readonly DeviceListRow[]) => void
}) {
  const online = devices.filter((device) => device.online)
  const offline = devices.filter((device) => !device.online)
  const grid = {
    gridTemplateColumns: `repeat(auto-fill, minmax(min(100%, ${DEVICE_CARD.minWidth}px), 1fr))`,
    gap: DEVICE_CARD.gap,
  }
  const cards = (list: readonly DeviceListRow[]) => (
    <div className="grid items-start" style={grid}>
      {list.map((device) => (
        <DeviceCard key={device.deviceId} device={device} onDevices={onDevices} />
      ))}
    </div>
  )
  return (
    <div className="flex flex-col pt-2">
      {online.length > 0 && cards(online)}
      {offline.length > 0 && (
        <>
          <div className="px-3 pt-3 pb-2 text-xs text-muted-foreground">
            {DEVICES_LIST_COPY.offline}
          </div>
          {cards(offline)}
        </>
      )}
    </div>
  )
}

/** A login at its limit: a FRESH report with a window at 100%. */
function limitHit(login: DeviceLogin): boolean {
  return (
    usageState(login) === `ready` &&
    usageAge(login.usage) === null &&
    Math.max(0, ...(login.usage?.windows ?? []).map((window) => window.percent)) >= 100
  )
}

type LoginAttention = Extract<DeviceAttentionItem, { kind: `login` }>

/** Every window of a login, as the panel's rows (wire labels; the 5h and
 *  weekly windows caption their reset). */
function panelWindows(login: DeviceLogin): UsageWindow[] {
  return (login.usage?.windows ?? []).map((window) => ({
    key: window.key,
    label: window.label,
    percent: window.percent,
    tone: severity(window.percent),
    caption: windowReset(window),
  }))
}

function DeviceCard({
  device,
  onDevices,
}: {
  device: DeviceListRow
  onDevices: (rows: readonly DeviceListRow[]) => void
}) {
  const logins = deviceLogins(device)
  const mine = !device.owner
  const online = device.online === true
  const row = deviceListRow({
    online,
    mine,
    server: device.kind === `server`,
    registered: true,
    outdated: false,
    latest: null,
    updateQueued: false,
    accountsReported: device.agentAccounts != null,
    logins: logins.map((login) => ({
      agent: login.agent,
      profileId: login.profileId,
      label: loginName(login),
      health: login.health,
      limitHit: limitHit(login),
    })),
    now: new Date().toISOString(),
  })
  // Collapsed until an icon is pressed.
  const [open, setOpen] = useState<string | null>(null)
  const [revealed, setRevealed] = useState(false)
  const [adding, setAdding] = useState(false)
  const openIndex = logins.findIndex((login) => login.key === open)
  const icons = deviceCardAccountIcons(
    logins.length,
    revealed || openIndex >= deviceCardAccountIcons(logins.length, false).visible
  )
  const itemFor = (login: DeviceLogin) =>
    row.attention.find(
      (item): item is LoginAttention =>
        item.kind === `login` && item.agent === login.agent && item.profileId === login.profileId
    ) ?? null
  const signsIn = canSignInOn(device)
  const canAdd = signsIn && addableAgents(device).length > 0
  const DeviceIcon = getDeviceIcon({ kind: device.kind })
  const label = device.label || device.deviceId
  const shared = mine && (device.sharedTeamIds?.length ?? 0) > 0
  const openLogin = openIndex >= 0 ? logins[openIndex]! : null
  return (
    <GlassCard
      data-testid={`device-row-${device.deviceId}`}
      className={cn(
        `flex min-w-0 flex-col gap-3`,
        (!online || !deviceHasRunnableAgent(device)) && `opacity-60`
      )}
      style={{ padding: DEVICE_CARD.padding }}
    >
      <div className="flex min-w-0 items-start gap-2">
        <DeviceIcon
          aria-hidden
          className="mt-px shrink-0 text-muted-foreground"
          style={{ width: DEVICE_CARD.deviceIconSize, height: DEVICE_CARD.deviceIconSize }}
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span
              className="min-w-0 truncate text-sm font-medium"
              title={device.owner ? `Shared by ${device.owner.name}` : label}
            >
              {label}
            </span>
            {device.version && (
              <span className="shrink-0 text-[10px] text-muted-foreground/60">
                v{device.version}
              </span>
            )}
            {mine && device.isDefault && (
              <span
                className="shrink-0 text-muted-foreground"
                title="Your default device — preselected when you start a coding session."
                aria-label="Default device"
              >
                <DefaultIcon className="size-3" />
              </span>
            )}
          </div>
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
              {online && <LiveDot tone="live" className="size-1.5 shrink-0" />}
              <span className="truncate">{deviceStatusText(device)}</span>
            </div>
            {shared && (
              <span className="shrink-0 rounded-sm border border-border/60 px-1 text-[10px] text-muted-foreground">
                Shared
              </span>
            )}
          </div>
        </div>
      </div>
      <div className="flex min-w-0 flex-col" style={{ gap: DEVICE_CARD.panelGap }}>
        <div className="flex min-w-0 flex-wrap items-center gap-1">
          {device.agentAccounts == null ? (
            <span className="text-xs text-muted-foreground">{DEVICES_LIST_COPY.checking}</span>
          ) : logins.length === 0 ? (
            <span className="text-xs text-muted-foreground">{DEVICES_LIST_COPY.noAccount}</span>
          ) : (
            logins.slice(0, icons.visible).map((login) => {
              const item = itemFor(login)
              const name = loginName(login)
              return (
                <Button
                  key={login.key}
                  variant="ghost"
                  size="icon-sm"
                  className={cn(`size-7`, open === login.key && `bg-glass-active hover:bg-glass-active`)}
                  aria-label={`${name} on ${label}`}
                  aria-expanded={open === login.key}
                  title={login.email || name}
                  data-testid={`device-login-chip-${login.key}`}
                  data-attention={item ? item.status : undefined}
                  onClick={() => setOpen((current) => (current === login.key ? null : login.key))}
                >
                  <AgentRunMark
                    agent={login.agent}
                    state={item ? `needs_input` : undefined}
                    ringClassName="ring-card-opaque"
                    className="size-4 [&>svg]:size-4"
                  />
                </Button>
              )
            })
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
          {canAdd && !adding && (
            <Button
              variant="ghost"
              size="icon-sm"
              className="size-7 text-muted-foreground"
              aria-label={DEVICES_LIST_COPY.addAccount}
              title={DEVICES_LIST_COPY.addAccount}
              data-testid={`add-account-${device.deviceId}`}
              onClick={() => setAdding(true)}
            >
              <AddIcon className="size-4" />
            </Button>
          )}
        </div>
        {adding && (
          <AddAccountRow device={device} onDevices={onDevices} autoStart onClose={() => setAdding(false)} />
        )}
        {openLogin && (
          <LoginPanel
            key={openLogin.key}
            login={openLogin}
            item={itemFor(openLogin)}
            device={signsIn ? device : null}
            onDevices={onDevices}
          />
        )}
      </div>
    </GlassCard>
  )
}

/** #EXP-1643: the open login's panel. */
function LoginPanel({
  login,
  item,
  device,
  onDevices,
}: {
  login: DeviceLogin
  item: LoginAttention | null
  /** The machine when it takes a sign-in (own, online, `agent-login`). */
  device: DeviceListRow | null
  onDevices: (rows: readonly DeviceListRow[]) => void
}) {
  const [signingIn, setSigningIn] = useState(false)
  const ready = usageState(login) === `ready`
  return (
    <DeviceAccountPanel
      name={login.email || loginName(login)}
      plan={login.email ? login.plan : null}
      status={item ? DEVICES_LIST_COPY.status[item.status] : null}
      windows={ready ? panelWindows(login) : []}
      stale={ready ? usageAge(login.usage) !== null : false}
      {...(device && loginSignsIn(login) && !signingIn
        ? { signIn: { label: DEVICES_LIST_COPY.signIn, onSelect: () => setSigningIn(true) } }
        : {})}
    >
      {device && signingIn && (
        <AccountLoginFlow
          device={device}
          agent={login.agent}
          target={{ profileId: login.profileId }}
          onDevices={onDevices}
          onDone={() => setSigningIn(false)}
        />
      )}
    </DeviceAccountPanel>
  )
}
