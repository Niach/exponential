// "My machines" (EXP-403): the caller's registered devices — desktops and
// headless `exponential` daemon servers — with live online state, last-seen
// fallback, and the "Add server" install one-liner. Since EXP-481 the rows
// ride the synced devices shape (useRemoteStart composes them) and the ⋯
// menu collapses to Edit + Remove — rename, team sharing (EXP-432), agent
// defaults and worktree management all live in the Device settings dialog.
// Teammates' shared servers render read-only under "Team machines".
import { useMemo, useState } from "react"
import { LoaderCircle } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import {
  deviceHasRunnableAgent,
  deviceIsMine,
  deviceIsOnline,
  deviceUnauthedAgentIds,
  deviceUpdateAvailable,
  showDeviceUpdateButton,
  type SteerDevice,
} from "@/lib/steer-devices"
import { CopySnippetButton } from "@/components/getting-started/mcp-setup-tabs"
import { DeviceSettingsDialog } from "@/components/device-settings-dialog"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

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
const EditIcon = conceptIcon(`ui-edit`)
const RemoveIcon = conceptIcon(`ui-delete`)
const MoreIcon = conceptIcon(`ui-more`)

// The install script is served by the CLOUD marketing site for every
// instance — self-hosted deployments ship only the web app (no marketing
// pages), so the one-liner always names the target instance explicitly via
// EXP_INSTANCE and the script itself is identical everywhere.
export function buildServerInstallSnippet(origin: string): string {
  return `curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=${origin} sh`
}

// The row's second line (native `deviceStatusLine` parity): a live dot +
// "Online" (amber + the signed-out agents when nothing is runnable, EXP-409),
// or the last-seen caption for offline machines.
function DeviceStatusLine({
  online,
  signInNeeded,
  unauthed,
  lastSeenAt,
}: {
  online: boolean
  signInNeeded: boolean
  unauthed: string[]
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
      <span
        className={`size-1.5 shrink-0 rounded-full ${
          signInNeeded ? `bg-amber-500` : `bg-emerald-500`
        }`}
      />
      <span className="truncate">
        {signInNeeded ? `${unauthed.join(`, `)} not signed in` : `Online`}
        {!signInNeeded && unauthed.length > 0 && (
          <span className="text-muted-foreground/60">
            {` · ${unauthed.join(`, `)} not signed in`}
          </span>
        )}
      </span>
    </div>
  )
}

export function MyMachines({
  devices,
  runBusy,
  sentTo,
  onStartCoding,
  onChanged,
  latestVersions,
  teamId,
}: {
  devices: SteerDevice[] | null
  runBusy: boolean
  sentTo: string | null
  onStartCoding: (deviceId: string) => void
  onChanged: () => void
  latestVersions: { desktop: string | null; cli: string | null } | null
  /** EXP-432: the current team — the share toggle's target. */
  teamId?: string
}) {
  const [addServerOpen, setAddServerOpen] = useState(false)
  const [settingsTargetId, setSettingsTargetId] = useState<string | null>(null)
  const [removeTarget, setRemoveTarget] = useState<SteerDevice | null>(null)
  const [busy, setBusy] = useState(false)
  const [updatingId, setUpdatingId] = useState<string | null>(null)

  const mine = devices?.filter(deviceIsMine) ?? null
  const teamShared = devices?.filter((device) => !deviceIsMine(device)) ?? []
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

  const snippet = useMemo(
    () =>
      buildServerInstallSnippet(
        typeof window === `undefined`
          ? `https://app.exponential.at`
          : window.location.origin
      ),
    []
  )

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

  return (
    <div className="mb-6">
      {/* EXP-616: the iOS Agents screen's plain-text section header — no
          count, the trailing control rides along. */}
      <GlassSectionHeader
        label="My machines"
        trailing={
          <Button
            variant="outline"
            size="sm"
            className="h-6 gap-1 px-2 text-xs"
            onClick={() => setAddServerOpen(true)}
          >
            <AddIcon className="size-3.5" />
            Add server
          </Button>
        }
      />

      {mine === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : mine.length === 0 ? (
        <div className="flex items-center gap-2 px-1 py-3 text-xs text-muted-foreground">
          <OfflineIcon className="size-3.5 shrink-0" />
          No machines yet. Open the Exponential desktop app, or add a server.
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          {mine.map((device) => {
            const online = deviceIsOnline(device)
            // EXP-409: installed-but-signed-out agents grey the machine out
            // (online but nothing runnable) or annotate it (a runnable
            // sibling still covers coding).
            const unauthed = deviceUnauthedAgentIds(device)
            const runnable = deviceHasRunnableAgent(device)
            const signInNeeded = online && !runnable && unauthed.length > 0
            const KindIcon = device.kind === `server` ? ServerIcon : DesktopIcon
            const latest =
              device.kind === `server`
                ? latestVersions?.cli
                : latestVersions?.desktop
            const outdated = deviceUpdateAvailable(device.version, latest)
            return (
              <GlassRow
                key={device.deviceId}
                className={signInNeeded ? `opacity-60` : undefined}
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
                        title={`Your default machine — preselected when you start a coding session.`}
                        aria-label="Default machine"
                      >
                        <DefaultIcon className="size-3 fill-current" />
                      </span>
                    )}
                    {device.sharedTeamId && (
                      <span
                        className="shrink-0 rounded-sm border border-border/60 px-1 text-[10px] text-muted-foreground"
                        title={
                          device.sharedTeamId === teamId
                            ? `Shared with this team — teammates can start coding sessions on this machine.`
                            : `Shared with another team.`
                        }
                      >
                        Shared
                      </span>
                    )}
                  </div>
                  <DeviceStatusLine
                    online={online}
                    signInNeeded={signInNeeded}
                    unauthed={unauthed}
                    lastSeenAt={device.lastSeenAt}
                  />
                </div>
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
                        device.updateRequested && device.updateBlocked
                          ? `A coding session is running — this machine updates itself once all sessions are closed.`
                          : `Ask the daemon to self-update (it restarts when idle)`
                      }
                      onClick={() => void requestUpdate(device)}
                    >
                      {device.updateRequested && device.updateBlocked ? (
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
                  <span
                    title={
                      signInNeeded
                        ? `No agent is signed in on this machine — sign in on the machine first (e.g. run \`${unauthed[0]}\` there).`
                        : undefined
                    }
                  >
                    <Button
                      variant="glass"
                      size="icon"
                      disabled={runBusy || !online || signInNeeded}
                      onClick={() => onStartCoding(device.deviceId)}
                      aria-label="Start coding"
                      // The wrapping span explains a sign-in block; its tooltip
                      // must not be shadowed by this one.
                      title={signInNeeded ? undefined : `Start coding`}
                    >
                      <StartCodingIcon />
                    </Button>
                  </span>
                  {device.registered && (
                    <DropdownMenu>
                      <DropdownMenuTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="size-8 text-foreground/50"
                          aria-label={`Machine menu for ${device.deviceLabel || device.deviceId}`}
                        >
                          <MoreIcon className="size-4" />
                        </Button>
                      </DropdownMenuTrigger>
                      <DropdownMenuContent align="end">
                        {/* EXP-481: rename, sharing, agent defaults and
                            worktrees all live in the settings dialog. */}
                        <DropdownMenuItem
                          onSelect={() => setSettingsTargetId(device.deviceId)}
                        >
                          <EditIcon />
                          Edit
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
                  )}
                </div>
              </GlassRow>
            )
          })}
        </div>
      )}
      {/* EXP-432: teammates' server devices shared with this team —
          read-only rows (owner name shown, no rename/remove/update), but
          fully startable. */}
      {teamShared.length > 0 && (
        <div className="mt-6">
          <GlassSectionHeader label="Team machines" />
          <div className="flex flex-col gap-2">
            {teamShared.map((device) => {
              const online = deviceIsOnline(device)
              const unauthed = deviceUnauthedAgentIds(device)
              const runnable = deviceHasRunnableAgent(device)
              const signInNeeded = online && !runnable && unauthed.length > 0
              return (
                <GlassRow
                  key={device.deviceId}
                  className={signInNeeded ? `opacity-60` : undefined}
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
                      signInNeeded={signInNeeded}
                      unauthed={unauthed}
                      lastSeenAt={device.lastSeenAt}
                    />
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      variant="glass"
                      size="icon"
                      disabled={runBusy || !online || signInNeeded}
                      onClick={() => onStartCoding(device.deviceId)}
                      aria-label="Start coding"
                      title="Start coding"
                    >
                      <StartCodingIcon />
                    </Button>
                  </div>
                </GlassRow>
              )
            })}
          </div>
        </div>
      )}

      {sentTo && (
        <div className="flex items-center gap-1.5 px-1 py-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          Start sent to {sentTo}. Waiting for the machine…
        </div>
      )}

      <Dialog open={addServerOpen} onOpenChange={setAddServerOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>Add a server</DialogTitle>
            <DialogDescription>
              Run this on any Linux or macOS machine. It installs the
              `exponential` CLI, signs you in with a device code, and
              registers the machine here.
            </DialogDescription>
          </DialogHeader>
          <pre className="overflow-auto rounded-md border bg-muted/30 p-3 text-left text-xs">
            {snippet}
          </pre>
          <DialogFooter className="sm:justify-start">
            <CopySnippetButton label="Copy" text={snippet} />
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <DeviceSettingsDialog
        device={settingsTarget}
        open={settingsTarget !== null}
        onOpenChange={(open) => {
          if (!open) setSettingsTargetId(null)
        }}
      />

      <Dialog
        open={removeTarget !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setRemoveTarget(null)
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Remove machine</DialogTitle>
            <DialogDescription>
              Remove “{removeTarget?.deviceLabel || removeTarget?.deviceId}”
              from your machines? A machine with the daemon still running will
              re-register itself on its next heartbeat.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="ghost" disabled={busy} onClick={() => setRemoveTarget(null)}>
              Cancel
            </Button>
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
