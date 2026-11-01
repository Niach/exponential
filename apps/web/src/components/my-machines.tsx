// "My devices" (EXP-403): the caller's registered devices — desktops and
// headless `exponential` daemon servers — with live online state, last-seen
// fallback, and the "Add device" dialog (EXP-697: desktop download + the
// CLI install one-liner; EXP-1111: `add-device-dialog.tsx`). Since EXP-481 the rows ride the synced devices
// shape (useRemoteStart composes them); rename, team sharing (EXP-432),
// agent defaults, worktrees, Update and Remove all live in the Device
// settings dialog. Teammates' shared servers render read-only under "Team
// devices".
//
// EXP-862: nothing on a row SAYS anything about sign-ins any more. A login's
// state is said once, on the login row that owns it (summarised by the
// device row's health badge) — the status line used to repeat it as "codex
// not signed in", and a "Sign in" pill repeated it a third time.
//
// EXP-909: each device lists its logins (editable on the caller's own
// machines, read-only on a teammate's shared server); this list also carries
// the 30 s refresh loop the old Accounts section ran.
//
// A-16 (`fixtures/devices-list.json`): ONE line per device, nothing folds.
// The logins are chips beside the name (limits on hover), and every warning
// (a dead or signed-out login, an update) sits behind the row's attention
// icon instead of being written inline. Online rows first, then the
// "Offline" label over the dimmed rest.
//
// EXP-1560: the list became a GRID of device CARDS. A card = name + version
// + status on top, its ⋯ overflow top-right (the update line, Update, Device
// settings; on a team device only a team OWNER's Remove, FEED-149, which
// drops that member's share of the machine from this team), and its logins
// as brand-mark icons along the bottom — the attention badge sits ON the
// login that needs it (the row's warning triangle and gear are gone). No play
// button: the Agent page composer's device picker is the one way to start a
// run.
import { useCallback, useMemo, useState, type ReactNode } from "react"
import {
  DEVICE_CARD,
  DEVICES_LIST_COPY,
  deviceListRow,
} from "@exp/domain-contract/devices-list"
import {
  conceptIcon,
  Button,
  GlassCard,
  Pill,
  GlassSectionHeader,
  Menu,
  type MenuEntry,
  Prompt,
  SkeletonRows,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { promptActions, removeDeviceSharePrompt } from "@/lib/prompts"
import { cn } from "@/lib/utils"
import {
  deviceReadyForRuns,
  deviceIsMine,
  deviceIsOnline,
  deviceUpdateAvailable,
  type SteerDevice,
} from "@/lib/steer-devices"
import { useNow } from "@/hooks/use-now"
import { AddDeviceDialog } from "@/components/add-device-dialog"
import {
  buildServerInstallSnippet,
  CopyIconButton,
  DeviceStatusLine,
} from "@/components/device-setup"
import { DeviceSettingsDialog } from "@/components/device-settings-dialog"
import {
  DeviceAccountIcons,
  loginLimitHit,
  useDeviceLogins,
} from "@/components/device-logins"
import { Empty } from "@/components/empty"
import { useAgentUsageRefresh } from "@/hooks/use-agent-usage-refresh"
import {
  deviceLoginRows,
  loginLabel,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"

// This is a MULTI-CLIENT surface (iOS/Android/desktop render the same list)
// — concepts, never raw lucide glyphs (CLAUDE.md icon rule).
const DefaultIcon = conceptIcon(`ui-device-default`)
const AddIcon = conceptIcon(`ui-add`)
// EXP-1560: the card's ⋯ overflow and its rows.
const MoreIcon = conceptIcon(`ui-more`)
const SettingsIcon = conceptIcon(`nav-settings`)
const UpdateIcon = conceptIcon(`ui-download`)
// FEED-149: a team owner's Remove on a teammate's shared card (the device
// settings' Remove glyph).
const RemoveIcon = conceptIcon(`ui-delete`)


// Moved into the device-setup block (EXP-1169); re-exported for the other
// install surfaces (getting-started cards).
export { buildServerInstallSnippet, CopyIconButton, DeviceStatusLine }

export function MyMachines({
  devices,
  onChanged,
  latestVersions,
  teamId,
  isTeamOwner = false,
}: {
  devices: SteerDevice[] | null
  onChanged: () => void
  latestVersions: { desktop: string | null; cli: string | null } | null
  /** EXP-432: the current team — the share toggle's target. */
  teamId?: string
  /** FEED-149: the caller owns `teamId` — its Team devices rows offer Remove. */
  isTeamOwner?: boolean
}) {
  const [addServerOpen, setAddServerOpen] = useState(false)
  const [settingsTargetId, setSettingsTargetId] = useState<string | null>(null)
  // FEED-149: the teammate's shared row a team owner is removing.
  const [dropTarget, setDropTarget] = useState<SteerDevice | null>(null)
  const [dropping, setDropping] = useState(false)
  const dropShare = async () => {
    if (!dropTarget?.rowId || !teamId || dropping) return
    setDropping(true)
    try {
      await trpc.devices.dropShare.mutate({ id: dropTarget.rowId, teamId }, SKIP_ERROR_TOAST)
      setDropTarget(null)
      onChanged()
    } catch (error) {
      failedAction(`remove`, `the device`, error)
    } finally {
      setDropping(false)
    }
  }
  // EXP-909: 30 s, not the default minute — the login rows under each device
  // age their "as of …" captions on this clock, and so does the refresh loop.
  const now = useNow(30_000)

  // EXP-875: real memos, keyed on `devices`. A fresh array per render made
  // every memo below miss, so the refresh loop's effects re-fired on every
  // render instead of on the 30 s beat.
  const mine = useMemo(
    () => devices?.filter(deviceIsMine) ?? null,
    [devices]
  )
  const teamShared = useMemo(
    () => devices?.filter((device) => !deviceIsMine(device)) ?? [],
    [devices]
  )
  // EXP-909: the 30 s auto-refresh the deleted Accounts section used to run,
  // keyed by LOGIN and scoped to the caller's own machines — a teammate's
  // server takes no commands from here.
  const ownLogins = useMemo(
    () =>
      (mine ?? []).flatMap((device) =>
        deviceLoginRows(
          {
            deviceId: device.deviceId,
            deviceLabel: device.deviceLabel,
            agentAccounts: device.agentAccounts,
            agentUsage: device.agentUsage,
          },
          { mine: true, online: deviceIsOnline(device) }
        )
      ),
    [mine]
  )
  // EXP-875: the caller's half of the verdict is the MACHINE's — online.
  // Whether the LOGIN could ever answer (mine, signed in, monitored) is the
  // hook's own `usageRefreshEligible`, so no surface can queue a command for
  // a login that can only report nothing.
  const canRefresh = useCallback(
    (row: AgentProfileUsageRow) => row.online,
    []
  )
  useAgentUsageRefresh(ownLogins, canRefresh, now)
  // Re-resolved each render so the dialog always edits the LIVE synced row.
  const settingsTarget =
    mine?.find((device) => device.deviceId === settingsTargetId) ?? null

  const dropCopy = removeDeviceSharePrompt(
    dropTarget?.deviceLabel || dropTarget?.deviceId || `this device`
  )

  const origin = useMemo(
    () =>
      typeof window === `undefined`
        ? `https://app.exponential.at`
        : window.location.origin,
    []
  )

  return (
    <div className="mb-6">
      {/* EXP-616: the iOS Agents screen's plain-text section header — no
          count, the trailing control rides along. */}
      <GlassSectionHeader
        label={DEVICES_LIST_COPY.mine}
        trailing={
          <Pill mode="action" onClick={() => setAddServerOpen(true)}>
            <AddIcon className="size-3" />
            Add device
          </Pill>
        }
      />

      {mine === null ? (
        <SkeletonRows rowHeight={52} />
      ) : mine.length === 0 ? (
        <Empty id="devices" />
      ) : (
        <DeviceGroups
          devices={mine}
          render={(device) => (
            <DeviceCard
              key={device.deviceId}
              device={device}
              now={now}
              latestVersions={latestVersions}
              teamId={teamId}
              onSettings={
                device.registered ? () => setSettingsTargetId(device.deviceId) : undefined
              }
            />
          )}
        />
      )}
      {/* EXP-432: teammates' server devices shared with this team — the same
          row, read-only (owner name in the tooltip). FEED-149: a team owner
          gets Remove in the gear's slot. */}
      {teamShared.length > 0 && (
        <div className="mt-6">
          <GlassSectionHeader label={DEVICES_LIST_COPY.team} />
          <DeviceGroups
            devices={teamShared}
            render={(device) => {
              const removable = Boolean(
                isTeamOwner && teamId && device.rowId && device.sharedTeamIds?.includes(teamId)
              )
              return (
                <DeviceCard
                  // A legacy duplicate shares the same machine id (FEED-149).
                  key={device.rowId ?? device.deviceId}
                  device={device}
                  now={now}
                  latestVersions={latestVersions}
                  teamId={teamId}
                  readOnly
                  onRemoveFromTeam={removable ? () => setDropTarget(device) : undefined}
                />
              )
            }}
          />
        </div>
      )}

      {/* EXP-697: the add-device dialog. EXP-1169: its content is the shared
          device-setup block (`device-setup.tsx`), the one the wizard and the
          join step render too. */}
      <AddDeviceDialog
        open={addServerOpen}
        onOpenChange={setAddServerOpen}
        origin={origin}
      />

      {/* FEED-149: the team owner's Remove, confirmed (prompts.json). */}
      <Prompt
        open={dropTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDropTarget(null)
        }}
        busy={dropping}
        title={dropCopy.title}
        body={dropCopy.body}
        actions={promptActions(dropCopy, {
          remove: { busy: dropping, onSelect: () => dropShare() },
        })}
      />

      {/* EXP-909 follow-up: Update and Remove live in here now, as the
          dialog's last two sections. */}
      <DeviceSettingsDialog
        device={settingsTarget}
        open={settingsTarget !== null}
        onOpenChange={(open) => {
          if (!open) setSettingsTargetId(null)
        }}
        latestVersions={latestVersions}
        onChanged={onChanged}
      />
    </div>
  )
}

/** EXP-1560: the online cards in the list's own order, then the muted
 *  "Offline" label over a grid of the offline ones. Columns follow
 *  `deviceCardColumns` (auto-fill at `card.minWidth`, `card.gap` apart). */
function DeviceGroups({
  devices,
  render,
}: {
  devices: readonly SteerDevice[]
  render: (device: SteerDevice) => ReactNode
}) {
  const online = devices.filter(deviceIsOnline)
  const offline = devices.filter((device) => !deviceIsOnline(device))
  const grid = {
    gridTemplateColumns: `repeat(auto-fill, minmax(${DEVICE_CARD.minWidth}px, 1fr))`,
    gap: DEVICE_CARD.gap,
  }
  return (
    <div className="flex flex-col pt-2">
      {online.length > 0 && (
        <div className="grid" style={grid} data-testid="device-grid">
          {online.map(render)}
        </div>
      )}
      {offline.length > 0 && (
        <>
          <div className="px-3 pt-3 pb-2 text-xs text-muted-foreground">
            {DEVICES_LIST_COPY.offline}
          </div>
          <div className="grid" style={grid}>
            {offline.map(render)}
          </div>
        </>
      )}
    </div>
  )
}

/** EXP-1560: ONE device card — name + version + status, the ⋯ overflow, the
 *  account icons. */
function DeviceCard({
  device,
  now,
  latestVersions,
  teamId,
  readOnly = false,
  onSettings,
  onRemoveFromTeam,
}: {
  device: SteerDevice
  now: Date
  latestVersions: { desktop: string | null; cli: string | null } | null
  teamId?: string
  readOnly?: boolean
  /** Own REGISTERED devices: the ⋯'s Device settings. */
  onSettings?: () => void
  /** FEED-149: a team owner's Remove on a teammate's shared card. */
  onRemoveFromTeam?: () => void
}) {
  const [updating, setUpdating] = useState(false)
  const online = deviceIsOnline(device)
  const mine = deviceIsMine(device)
  // EXP-409: a device with nothing runnable greys out; EXP-1196: the doctor
  // report decides when the device sends one.
  const runnable = deviceReadyForRuns(device)
  const latest =
    (device.kind === `server` ? latestVersions?.cli : latestVersions?.desktop) ?? null
  const outdated = deviceUpdateAvailable(device.version, latest)
  const logins = useDeviceLogins(device)
  const row = deviceListRow({
    online,
    mine,
    server: device.kind === `server`,
    registered: device.registered === true,
    outdated,
    latest,
    // FEED-36 / EXP-1075: an update parked behind live sessions.
    updateQueued: Boolean(device.updateRequested && device.updateBlocked),
    accountsReported: device.agentAccounts !== undefined,
    logins: logins.map((login) => ({
      agent: login.agent,
      profileId: login.profileId,
      label: loginLabel(login),
      health: login.health,
      limitHit: loginLimitHit(login, now),
    })),
  })
  const requestUpdate = async () => {
    if (updating) return
    setUpdating(true)
    try {
      await trpc.devices.requestUpdate.mutate({ deviceId: device.deviceId }, SKIP_ERROR_TOAST)
    } catch (error) {
      failedAction(`update`, `the device`, error, { retry: () => void requestUpdate() })
    } finally {
      setUpdating(false)
    }
  }
  const update = row.attention.find((item) => item.kind !== `login`) ?? null
  const label = device.deviceLabel || device.deviceId
  const entries: MenuEntry[] = []
  if (update) {
    entries.push({ kind: `item`, id: `update-line`, label: update.text, disabled: true, onSelect: () => {} })
    if (update.action === `update`) {
      entries.push({
        kind: `item`,
        id: `update`,
        label: DEVICES_LIST_COPY.update,
        icon: UpdateIcon,
        // Requested and not yet applied: pending, never clicked twice.
        disabled: updating || device.updateRequested === true,
        onSelect: () => void requestUpdate(),
      })
    }
  }
  if (onSettings) {
    if (entries.length > 0) entries.push({ kind: `separator`, id: `sep` })
    entries.push({ kind: `item`, id: `settings`, label: DEVICES_LIST_COPY.settings, icon: SettingsIcon, onSelect: onSettings })
  }
  if (onRemoveFromTeam) {
    entries.push({
      kind: `item`,
      id: `remove`,
      label: DEVICES_LIST_COPY.removeFromTeam,
      icon: RemoveIcon,
      destructive: true,
      onSelect: onRemoveFromTeam,
    })
  }
  const shared = mine && (device.sharedTeamIds?.length ?? 0) > 0
  return (
    <GlassCard
      data-testid={`device-row-${device.deviceId}`}
      className={cn(
        `flex min-w-0 flex-col justify-between gap-3`,
        !online && `opacity-60`,
        online && !runnable && `opacity-60`
      )}
      style={{ padding: DEVICE_CARD.padding }}
    >
      <div className="flex min-w-0 items-start gap-2">
        {/* FEED-15: name + version (+ star) on top, live/last-seen state and
            the tags beneath. */}
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span
              className="min-w-0 truncate text-sm font-medium"
              // EXP-525: a teammate's shared card keeps the attribution here.
              title={!mine && device.owner ? `Shared by ${device.owner.name}` : label}
            >
              {label}
            </span>
            {device.version && (
              <span
                // Only my own machine's update is mine to act on.
                className={`shrink-0 text-[10px] ${mine && outdated ? `text-amber-500` : `text-muted-foreground/60`}`}
                title={mine && outdated ? `Update available: ${latest}` : undefined}
              >
                v{device.version}
              </span>
            )}
            {mine && device.isDefault && (
              <span
                className="shrink-0 text-muted-foreground"
                title={`Your default device — preselected when you start a coding session.`}
                aria-label="Default device"
              >
                <DefaultIcon className="size-3 fill-current" />
              </span>
            )}
          </div>
          <div className="flex min-w-0 flex-wrap items-center gap-1.5">
            <DeviceStatusLine online={online} lastSeenAt={device.lastSeenAt} />
            {shared && (
              <span
                className="shrink-0 rounded-sm border border-border/60 px-1 text-[10px] text-muted-foreground"
                title={
                  teamId && device.sharedTeamIds?.includes(teamId)
                    ? `Shared with this team — teammates can start coding sessions on this device.`
                    : `Shared with other teams.`
                }
              >
                Shared
              </span>
            )}
          </div>
        </div>
        {entries.length > 0 ? (
          <Menu
            entries={entries}
            align="end"
            aria-label={`${DEVICES_LIST_COPY.deviceActions}: ${label}`}
            trigger={
              <Button
                variant="ghost"
                size="icon-sm"
                className="-mt-1 -mr-1 size-7 shrink-0 text-muted-foreground"
                aria-label={`${DEVICES_LIST_COPY.deviceActions}: ${label}`}
                title={DEVICES_LIST_COPY.deviceActions}
                data-testid={`device-menu-${device.deviceId}`}
              >
                <MoreIcon className="size-4" />
              </Button>
            }
          />
        ) : (
          <span aria-hidden className="size-7 shrink-0" />
        )}
      </div>
      <DeviceAccountIcons
        device={device}
        rows={logins}
        attention={row.attention}
        now={now}
        readOnly={readOnly}
      />
    </GlassCard>
  )
}
