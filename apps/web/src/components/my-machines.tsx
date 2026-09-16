// "My devices" (EXP-403): the caller's registered devices — desktops and
// headless `exponential` daemon servers — with live online state, last-seen
// fallback, and the "Add device" dialog (EXP-697: desktop download + the
// CLI install one-liner). Since EXP-481 the rows
// ride the synced devices shape (useRemoteStart composes them) and the ⋯
// menu collapses to Device settings + Remove — rename, team sharing
// (EXP-432), agent defaults and worktree management all live in the Device
// settings dialog. Teammates' shared servers render read-only under "Team
// devices".
//
// EXP-862: nothing on a row SAYS anything about sign-ins any more. A login's
// state is said once, on the login row that owns it (summarised by the
// device row's health badge) — the status line used to repeat it as "codex
// not signed in", and a "Sign in" pill repeated it a third time.
//
// EXP-909: each device LISTS its logins underneath (`DeviceLogins`), editable
// on the caller's own machines and read-only on a teammate's shared server.
// That fold replaced the cross-device Accounts section, so this list also
// carries the 30 s refresh loop that used to live there.
import { useMemo, useState } from "react"
import { LoaderCircle } from "lucide-react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import {
  conceptIcon,
  Button,
  Pill,
  GlassSectionHeader,
  ListRow,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  LiveDot,
} from "@exp/ui"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import {
  describeUpdateBlockers,
  deviceCanRefreshUsage,
  deviceCanUpdateNow,
  deviceHasRunnableAgent,
  deviceIsMine,
  deviceIsOnline,
  deviceUpdateAvailable,
  liveUpdateBlockers,
  showDeviceUpdateButton,
  type SteerDevice,
  type UpdateBlockerSession,
} from "@/lib/steer-devices"
import {
  codingSessionCollection,
  issueCollection,
  userCollection,
} from "@/lib/collections"
import type { CodingSession, Issue, User } from "@/db/schema"
import { useNow } from "@/hooks/use-now"
import { desktopDownloadHref } from "@/lib/desktop-download"
import { DeviceSettingsDialog } from "@/components/device-settings-dialog"
import { DeviceLogins } from "@/components/device-logins"
import { useAgentUsageRefresh } from "@/hooks/use-agent-usage-refresh"
import {
  deviceLoginRows,
  deviceWorstHealth,
  healthBadgeLabel,
} from "@/lib/agent-usage"

// This is a MULTI-CLIENT surface (iOS/Android/desktop render the same list)
// — concepts, never raw lucide glyphs (CLAUDE.md icon rule); the LoaderCircle
// spinner mirrors the agents page's existing raw usage.
const DesktopIcon = conceptIcon(`ui-device`)
// EXP-615: starting a run is a play icon button on every client.
const StartCodingIcon = conceptIcon(`action-run`)
const ServerIcon = conceptIcon(`ui-server`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const DefaultIcon = conceptIcon(`ui-device-default`)
const AddIcon = conceptIcon(`ui-add`)
const UpdateIcon = conceptIcon(`ui-update`)
// EXP-862: the ⋯ menu opens Device settings — the settings gear, ×4. A pencil
// promised an inline rename, not the dialog it opens.
const SettingsIcon = conceptIcon(`nav-settings`)
const RemoveIcon = conceptIcon(`ui-delete`)
const MoreIcon = conceptIcon(`ui-more`)
const CopyIcon = conceptIcon(`ui-copy`)
const CheckIcon = conceptIcon(`ui-check`)

/** FEED-36: the tooltip on a queued Update button — the daemon's own rules
 * for getting there (every session ends, or one sits idle for 2 hours). */
export const QUEUED_UPDATE_TOOLTIP = `Live sessions hold this update — the device restarts itself once every session ends or sits idle for 2 hours.`

/** FEED-36: the caller's LIVE sessions per machine (`running`/`in_review`
 * off the synced coding_sessions shape), with the issue identifier joined
 * for the blocker line. One query for the whole list, not one per row. */
function useUpdateBlockers(): (device: SteerDevice) => UpdateBlockerSession[] {
  const { data: sessionRows } = useLiveQuery(
    (q) =>
      q
        .from({ s: codingSessionCollection })
        .where(({ s }) => inArray(s.status, [`running`, `in_review`])),
    []
  )
  const sessions = (sessionRows ?? []) as CodingSession[]
  const issueIds = useMemo(
    () =>
      [...new Set(sessions.map((s) => s.issueId).filter((id): id is string => !!id))].sort(),
    [sessions]
  )
  const issueKey = issueIds.join(`,`)
  const { data: issueRows } = useLiveQuery(
    (q) =>
      issueIds.length > 0
        ? q
            .from({ i: issueCollection })
            .where(({ i }) => inArray(i.id, issueIds))
        : undefined,
    [issueKey]
  )
  const identifierById = useMemo(() => {
    const map = new Map<string, string>()
    for (const issue of (issueRows ?? []) as Issue[]) {
      map.set(issue.id, issue.identifier)
    }
    return map
  }, [issueRows])
  return (device) =>
    sessions
      .filter((s) => s.deviceId === device.deviceId)
      .map((s) => ({
        issueIdentifier: s.issueId ? (identifierById.get(s.issueId) ?? null) : null,
        actionName: s.actionName,
        userId: s.userId,
        startedAt: s.startedAt,
        updatedAt: s.updatedAt,
      }))
}

// The install script is served by the CLOUD marketing site for every
// instance — self-hosted deployments ship only the web app (no marketing
// pages), so the one-liner always names the target instance explicitly via
// EXP_INSTANCE and the script itself is identical everywhere.
export function buildServerInstallSnippet(origin: string): string {
  return `curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=${origin} sh`
}

/** The icon-only copy control living INSIDE the install-snippet box
 * (EXP-697 — the add-device dialog, shared layout with the IDE's). */
export function CopyIconButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <Button
      variant="glass"
      size="icon-sm"
      className="absolute top-1.5 right-1.5"
      aria-label="Copy install command"
      title="Copy install command"
      onClick={() => {
        void navigator.clipboard.writeText(text)
        setCopied(true)
        window.setTimeout(() => setCopied(false), 1_500)
      }}
    >
      {copied ? (
        <CheckIcon className="size-3.5" />
      ) : (
        <CopyIcon className="size-3.5" />
      )}
    </Button>
  )
}

// The row's second line (native `deviceStatusLine` parity): a live dot +
// "Online", or the last-seen caption for offline devices. EXP-862: it says
// nothing about sign-ins — the account chips own that.
export function DeviceStatusLine({
  online,
  lastSeenAt,
}: {
  online: boolean
  lastSeenAt: string | null | undefined
}) {
  if (!online) {
    return (
      <div className="truncate text-xs text-muted-foreground">
        {lastSeenAt ? `Last seen ${relativeTime(lastSeenAt)}` : `Offline`}
      </div>
    )
  }
  return (
    <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
      <LiveDot tone="live" className="size-1.5 shrink-0" />
      <span className="truncate">Online</span>
    </div>
  )
}

export function MyMachines({
  devices,
  onStartCoding,
  onChanged,
  latestVersions,
  teamId,
}: {
  devices: SteerDevice[] | null
  /** EXP-825: a navigation to the Agent page composer with this machine
   * pre-picked — the row itself never starts anything. */
  onStartCoding: (deviceId: string) => void
  onChanged: () => void
  latestVersions: { desktop: string | null; cli: string | null } | null
  /** EXP-432: the current team — the share toggle's target. */
  teamId?: string
}) {
  const [addServerOpen, setAddServerOpen] = useState(false)
  const [settingsTargetId, setSettingsTargetId] = useState<string | null>(null)
  const [removeTarget, setRemoveTarget] = useState<SteerDevice | null>(null)
  // FEED-36: the "Update now" confirmation — ends the machine's live sessions.
  const [updateNowTarget, setUpdateNowTarget] = useState<SteerDevice | null>(null)
  const [busy, setBusy] = useState(false)
  const [updatingId, setUpdatingId] = useState<string | null>(null)
  // EXP-909: 30 s, not the default minute — the login rows under each device
  // age their "as of …" captions on this clock, and so does the refresh loop.
  const now = useNow(30_000)
  const blockersFor = useUpdateBlockers()
  const { data: userRows } = useLiveQuery(
    (q) => q.from({ u: userCollection }),
    []
  )
  const usersById = useMemo(() => {
    const map = new Map<string, Pick<User, `name` | `email`>>()
    for (const user of (userRows ?? []) as User[]) map.set(user.id, user)
    return map
  }, [userRows])

  const mine = devices?.filter(deviceIsMine) ?? null
  const teamShared = devices?.filter((device) => !deviceIsMine(device)) ?? []
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
            agentUsageAt: device.agentUsageAt,
          },
          { mine: true, online: deviceIsOnline(device) }
        )
      ),
    [mine]
  )
  const capsByDevice = useMemo(
    () => new Map((mine ?? []).map((device) => [device.deviceId, device.caps ?? []])),
    [mine]
  )
  useAgentUsageRefresh(
    ownLogins,
    (row) =>
      row.online &&
      deviceCanRefreshUsage({ caps: capsByDevice.get(row.deviceId) ?? [] }),
    now
  )
  // Re-resolved each render so the dialog always edits the LIVE synced row.
  const settingsTarget =
    mine?.find((device) => device.deviceId === settingsTargetId) ?? null

  const requestUpdate = async (device: SteerDevice) => {
    if (updatingId) return
    setUpdatingId(device.deviceId)
    try {
      await trpc.devices.requestUpdate.mutate({ deviceId: device.deviceId })
      onChanged()
    } finally {
      setUpdatingId(null)
    }
  }

  const origin = useMemo(
    () =>
      typeof window === `undefined`
        ? `https://app.exponential.at`
        : window.location.origin,
    []
  )
  const snippet = buildServerInstallSnippet(origin)
  const userAgent = typeof navigator === `undefined` ? `` : navigator.userAgent

  const remove = async () => {
    if (!removeTarget || busy) return
    setBusy(true)
    try {
      await trpc.devices.remove.mutate({ deviceId: removeTarget.deviceId })
      setRemoveTarget(null)
      onChanged()
    } finally {
      setBusy(false)
    }
  }

  const updateNow = async () => {
    if (!updateNowTarget || busy) return
    setBusy(true)
    try {
      await trpc.devices.requestUpdate.mutate({
        deviceId: updateNowTarget.deviceId,
        endSessions: true,
      })
      setUpdateNowTarget(null)
      onChanged()
    } finally {
      setBusy(false)
    }
  }
  const updateNowLiveCount = updateNowTarget
    ? liveUpdateBlockers(blockersFor(updateNowTarget), now).length
    : 0

  return (
    <div className="mb-6">
      {/* EXP-616: the iOS Agents screen's plain-text section header — no
          count, the trailing control rides along. */}
      <GlassSectionHeader
        label="My devices"
        trailing={
          <Pill mode="action" onClick={() => setAddServerOpen(true)}>
            <AddIcon className="size-3" />
            Add device
          </Pill>
        }
      />

      {mine === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : mine.length === 0 ? (
        <div className="flex items-center gap-2 px-1 py-3 text-xs text-muted-foreground">
          <OfflineIcon className="size-3.5 shrink-0" />
          No devices yet. Open the Exponential desktop app, or add a device.
        </div>
      ) : (
        <div className="flex flex-col gap-0">
          {mine.map((device) => {
            const online = deviceIsOnline(device)
            // EXP-409: a device with nothing runnable greys out — EXP-862
            // leaves the WHY to the account chips.
            const runnable = deviceHasRunnableAgent(device)
            // EXP-836: play hands this device to the Agent composer, which
            // only starts on an online device WITH a runnable agent — so the
            // button gates on exactly that. It used to gate on the sign-in
            // state alone, so a device reporting no agents at all (nothing
            // installed, an older build) opened the composer and the
            // pre-picked device silently lost to the default one.
            const startable = online && runnable
            const KindIcon = device.kind === `server` ? ServerIcon : DesktopIcon
            const latest =
              device.kind === `server`
                ? latestVersions?.cli
                : latestVersions?.desktop
            const outdated = deviceUpdateAvailable(device.version, latest)
            // FEED-36: a queued update parked behind live sessions says
            // WHICH ones, and a capable daemon offers to end them now.
            const updateQueued = Boolean(
              device.updateRequested && device.updateBlocked
            )
            const blockerLine = updateQueued
              ? describeUpdateBlockers(blockersFor(device), usersById, now)
              : null
            // EXP-849: the worst health of the accounts this device holds —
            // "needs re-login" is a DIFFERENT problem from "signed out", and
            // the chips below say which account it is. Null when every login
            // is fine (or the device reported none).
            const healthBadge = healthBadgeLabel(
              deviceWorstHealth({ agentAccounts: device.agentAccounts }) ?? `ok`
            )
            return (
              <ListRow
                key={device.deviceId}
                interactive
                className={online && !runnable ? `opacity-60` : undefined}
              >
                <KindIcon className="size-4 shrink-0 text-foreground/70" />
                {/* FEED-15: the native two-line row — name + version (+ Shared)
                    on top, live/last-seen state beneath, controls trailing —
                    so phones never wrap the launcher onto its own line. */}
                <div className="min-w-0 flex-1">
                  <div className="flex min-w-0 items-baseline gap-1.5">
                    <span className="min-w-0 truncate text-sm font-medium">
                      {device.deviceLabel || device.deviceId}
                    </span>
                    {device.version && (
                      <span
                        className={`shrink-0 text-[10px] ${
                          outdated
                            ? `text-amber-500`
                            : `text-muted-foreground/60`
                        }`}
                        title={
                          outdated ? `Update available: ${latest}` : undefined
                        }
                      >
                        v{device.version}
                      </span>
                    )}
                    {device.isDefault && (
                      <span
                        className="shrink-0 text-muted-foreground"
                        title={`Your default device — preselected when you start a coding session.`}
                        aria-label="Default device"
                      >
                        <DefaultIcon className="size-3 fill-current" />
                      </span>
                    )}
                    {(device.sharedTeamIds?.length ?? 0) > 0 && (
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
                    {healthBadge && (
                      <span className="shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] font-medium text-amber-500">
                        {healthBadge}
                      </span>
                    )}
                  </div>
                  <DeviceStatusLine
                    online={online}
                    lastSeenAt={device.lastSeenAt}
                  />
                  {blockerLine && (
                    <div
                      className="truncate text-xs text-amber-500"
                      title={blockerLine}
                    >
                      {blockerLine}
                    </div>
                  )}
                  {/* EXP-909: the logins this device holds — every repair
                      control lives on these rows. */}
                  <DeviceLogins device={device} now={now} />
                </div>
                {/* EXP-698: the fixed trailing column — a play slot and a ⋯
                    slot, so the controls line up down the list. A row without
                    a menu renders the slot as an empty spacer of the same
                    size rather than sliding its play button over. */}
                <div className="flex shrink-0 items-center gap-1">
                  {/* EXP-420: only when a newer version really exists (or an
                      update is already in flight — keep its progress visible). */}
                  {(showDeviceUpdateButton(device, latest) ||
                    updatingId === device.deviceId) && (
                    <Button
                      variant="ghost"
                      size="sm"
                      className={outdated ? `text-amber-500` : `text-muted-foreground`}
                      disabled={device.updateRequested || updatingId === device.deviceId}
                      title={
                        updateQueued
                          ? QUEUED_UPDATE_TOOLTIP
                          : `Ask the daemon to self-update (it restarts when idle)`
                      }
                      onClick={() => void requestUpdate(device)}
                    >
                      {updateQueued ? (
                        // EXP-411: parked behind live sessions — say so instead
                        // of spinning until the last one closes.
                        <>
                          <UpdateIcon />
                          <span className="max-sm:sr-only">Queued</span>
                        </>
                      ) : device.updateRequested ||
                        updatingId === device.deviceId ? (
                        <>
                          <LoaderCircle className="animate-spin" />
                          <span className="max-sm:sr-only">Updating…</span>
                        </>
                      ) : (
                        <>
                          <UpdateIcon />
                          <span className="max-sm:sr-only">Update</span>
                        </>
                      )}
                    </Button>
                  )}
                  {updateQueued && deviceCanUpdateNow(device) && (
                    <Pill
                      mode="action"
                      onClick={() => setUpdateNowTarget(device)}
                      title={`End this device's live sessions and restart it on the new version now.`}
                    >
                      <UpdateIcon className="size-3" />
                      Update now…
                    </Pill>
                  )}
                  <span
                    title={
                      online && !runnable
                        ? `No agent is signed in on this device.`
                        : undefined
                    }
                  >
                    <Button
                      variant="glass"
                      size="icon"
                      disabled={!startable}
                      onClick={() => onStartCoding(device.deviceId)}
                      aria-label="Start coding"
                      // The wrapping span explains a sign-in block; its tooltip
                      // must not be shadowed by this one.
                      title={startable ? `Start coding` : undefined}
                    >
                      <StartCodingIcon />
                    </Button>
                  </span>
                  {device.registered ? (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        {/* EXP-862: a ⋯ is a GHOST icon button ×4 — no
                            circle, no border. */}
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label={`Device menu for ${device.deviceLabel || device.deviceId}`}
                        >
                          <MoreIcon />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {/* EXP-481: rename, sharing, agent defaults and
                            worktrees all live in the settings dialog. */}
                        <DropdownMenuItem
                          onSelect={() => setSettingsTargetId(device.deviceId)}
                        >
                          <SettingsIcon />
                          Device settings
                        </DropdownMenuItem>
                        <DropdownMenuItem
                          variant="destructive"
                          onSelect={() => setRemoveTarget(device)}
                        >
                          <RemoveIcon />
                          Remove
                        </DropdownMenuItem>
                      </DropdownMenuContent>
                    </DropdownMenu>
                  ) : (
                    <span aria-hidden className="size-8 shrink-0" />
                  )}
                </div>
              </ListRow>
            )
          })}
        </div>
      )}
      {/* EXP-432: teammates' server devices shared with this team —
          read-only rows (owner name shown, no rename/remove/update), but
          fully startable. */}
      {teamShared.length > 0 && (
        <div className="mt-6">
          <GlassSectionHeader label="Team devices" />
          <div className="flex flex-col gap-0">
            {teamShared.map((device) => {
              const online = deviceIsOnline(device)
              const runnable = deviceHasRunnableAgent(device)
              return (
                <ListRow
                  key={device.deviceId}
                  interactive
                  className={online && !runnable ? `opacity-60` : undefined}
                >
                  <ServerIcon className="size-4 shrink-0 text-foreground/70" />
                  <div className="min-w-0 flex-1">
                    {/* EXP-525: no people names inline — a teammate's shared
                        row keeps the attribution in its tooltip. */}
                    <div
                      className="min-w-0 truncate text-sm font-medium"
                      title={
                        device.owner ? `Shared by ${device.owner.name}` : undefined
                      }
                    >
                      {device.deviceLabel || device.deviceId}
                    </div>
                    <DeviceStatusLine
                      online={online}
                      lastSeenAt={device.lastSeenAt}
                    />
                    {/* EXP-909: a teammate's machine lists its logins too —
                        read-only: its credentials are theirs to repair. */}
                    <DeviceLogins device={device} now={now} readOnly />
                  </div>
                  {/* The same fixed trailing column as "My devices": a
                      read-only row has no ⋯ menu, so its slot is an empty
                      spacer and the play buttons stay in one line. */}
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      variant="glass"
                      size="icon"
                      // EXP-836: same predicate as own devices — the composer
                      // cannot start on a device with no runnable agent.
                      disabled={!online || !runnable}
                      onClick={() => onStartCoding(device.deviceId)}
                      title={
                        online && !runnable
                          ? `No agent is signed in on this device.`
                          : `Start coding`
                      }
                      aria-label="Start coding"
                    >
                      <StartCodingIcon />
                    </Button>
                    <span aria-hidden className="size-8 shrink-0" />
                  </div>
                </ListRow>
              )
            })}
          </div>
        </div>
      )}

      {/* EXP-697: the add-device dialog — byte-matched copy and layout with
          the IDE's (`machines.rs` open_add_server_dialog). Desktop download
          first, then the CLI one-liner shown on two fixed lines (never a
          horizontal scroll) with the copy control inside the box. No footer. */}
      <Dialog open={addServerOpen} onOpenChange={setAddServerOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Add device</DialogTitle>
            <DialogDescription>
              To run coding sessions, install the desktop app.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-4">
            <Button asChild className="w-fit">
              <a href={desktopDownloadHref(userAgent)} target="_blank" rel="noreferrer">
                Download desktop app
              </a>
            </Button>
            <p className="text-sm text-muted-foreground">
              Or install the Exponential CLI on a server:
            </p>
            <div className="relative">
              <pre className="rounded-md border bg-muted/30 p-3 pr-10 text-left text-xs whitespace-pre-wrap">
                {`curl -fsSL https://exponential.at/install.sh |\n  EXP_INSTANCE=${origin} sh`}
              </pre>
              <CopyIconButton text={snippet} />
            </div>
          </div>
        </DialogContent>
      </Dialog>

      <DeviceSettingsDialog
        device={settingsTarget}
        open={settingsTarget !== null}
        onOpenChange={(open) => {
          if (!open) setSettingsTargetId(null)
        }}
      />

      {/* FEED-36: Update now — the daemon ends every live session on the
          machine and restarts on the queued version; confirmed, since it
          interrupts work (repo-backed runs resume from their session page). */}
      <AlertDialog
        open={updateNowTarget !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setUpdateNowTarget(null)
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {`Update ${updateNowTarget?.deviceLabel || updateNowTarget?.deviceId || `this device`} now?`}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {updateNowLiveCount > 0
                ? `Ends the ${updateNowLiveCount} live ${
                    updateNowLiveCount === 1 ? `session` : `sessions`
                  } on this device (repo-backed runs can be resumed from their session page) and restarts it on the new version.`
                : `Ends every live session on this device (repo-backed runs can be resumed from their session page) and restarts it on the new version.`}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={busy}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={busy}
              onClick={(event) => {
                event.preventDefault()
                void updateNow()
              }}
            >
              {busy && <LoaderCircle className="animate-spin" />}
              Update now
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <Dialog
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setRemoveTarget(null)
        }}
      >
        <DialogContent mobile="alert" className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Remove device</DialogTitle>
            <DialogDescription>
              Remove “{removeTarget?.deviceLabel || removeTarget?.deviceId}”
              from your devices? A device with the daemon still running will
              re-register itself on its next heartbeat.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogCancel disabled={busy} onClick={() => setRemoveTarget(null)} />
            <Button variant="destructive" disabled={busy} onClick={() => void remove()}>
              {busy && <LoaderCircle className="animate-spin" />}
              Remove
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
