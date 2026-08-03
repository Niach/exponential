// "My machines" (EXP-403): the caller's registered devices — desktops and
// headless `exponential` daemon servers — with live online state, last-seen
// fallback, rename/remove, and the "Add server" install one-liner. Fed by
// `devices.list` via useRemoteStart's poll; per-user, never team state.
import { useMemo, useState } from "react"
import { LoaderCircle, MonitorUp } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { parseVersionTuple } from "@/lib/client-version"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import { deviceIsOnline, type SteerDevice } from "@/lib/steer-devices"
import { CopySnippetButton } from "@/components/getting-started/mcp-setup-tabs"
import { SectionLabel } from "@/components/agent-session-row"
import { Button } from "@/components/ui/button"
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
import { Input } from "@/components/ui/input"

// This is a MULTI-CLIENT surface (iOS/Android/desktop render the same list)
// — concepts, never raw lucide glyphs (CLAUDE.md icon rule). MonitorUp and
// the LoaderCircle spinner mirror the agents page's existing raw usage.
const DesktopIcon = conceptIcon(`ui-device`)
const ServerIcon = conceptIcon(`ui-server`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const AddIcon = conceptIcon(`ui-add`)
const UpdateIcon = conceptIcon(`ui-update`)
const RenameIcon = conceptIcon(`ui-edit`)
const RemoveIcon = conceptIcon(`ui-delete`)
const MoreIcon = conceptIcon(`ui-more`)

// The install script is served by the CLOUD marketing site for every
// instance — self-hosted deployments ship only the web app (no marketing
// pages), so the one-liner always names the target instance explicitly via
// EXP_INSTANCE and the script itself is identical everywhere.
export function buildServerInstallSnippet(origin: string): string {
  return `curl -fsSL https://exponential.at/install.sh | EXP_INSTANCE=${origin} sh`
}

/// Version compares below the platform's CLIENT_LATEST_VERSION_* value.
function updateAvailable(
  version: string | null | undefined,
  latest: string | null | undefined
): boolean {
  if (!version || !latest) return false
  const have = parseVersionTuple(version)
  const want = parseVersionTuple(latest)
  if (!have || !want) return false
  for (let i = 0; i < 3; i++) {
    if (have[i] !== want[i]) return have[i] < want[i]
  }
  return false
}

export function MyMachines({
  devices,
  runBusy,
  sentTo,
  onStartCoding,
  onChanged,
  latestVersions,
}: {
  devices: SteerDevice[] | null
  runBusy: boolean
  sentTo: string | null
  onStartCoding: (deviceId: string) => void
  onChanged: () => void
  latestVersions: { desktop: string | null; cli: string | null } | null
}) {
  const [addServerOpen, setAddServerOpen] = useState(false)
  const [renameTarget, setRenameTarget] = useState<SteerDevice | null>(null)
  const [renameValue, setRenameValue] = useState(``)
  const [removeTarget, setRemoveTarget] = useState<SteerDevice | null>(null)
  const [busy, setBusy] = useState(false)
  const [updatingId, setUpdatingId] = useState<string | null>(null)

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

  const rename = async () => {
    if (!renameTarget || !renameValue.trim() || busy) return
    setBusy(true)
    try {
      await trpc.devices.rename.mutate({
        deviceId: renameTarget.deviceId,
        label: renameValue.trim(),
      })
      setRenameTarget(null)
      onChanged()
    } finally {
      setBusy(false)
    }
  }

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
    <div className="mb-4">
      <div className="flex items-center justify-between">
        <SectionLabel label="My machines" count={devices?.length ?? 0} />
        <Button
          variant="ghost"
          size="sm"
          className="text-muted-foreground"
          onClick={() => setAddServerOpen(true)}
        >
          <AddIcon />
          Add server
        </Button>
      </div>

      {devices === null ? (
        <div className="px-3 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : devices.length === 0 ? (
        <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
          <OfflineIcon className="size-3.5 shrink-0" />
          No machines yet. Open the Exponential desktop app, or add a server
          with the install one-liner.
        </div>
      ) : (
        devices.map((device) => {
          const online = deviceIsOnline(device)
          const KindIcon = device.kind === `server` ? ServerIcon : DesktopIcon
          const latest =
            device.kind === `server`
              ? latestVersions?.cli
              : latestVersions?.desktop
          const outdated = updateAvailable(device.version, latest)
          return (
            <div
              key={device.deviceId}
              className="flex items-center gap-2 border-b border-border/30 px-3 py-2"
            >
              <KindIcon className="size-4 shrink-0 text-muted-foreground" />
              <span className="flex min-w-0 flex-1 items-baseline gap-1.5">
                <span className="min-w-0 truncate text-sm">
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
              </span>
              {online ? (
                <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
                  <span className="size-1.5 rounded-full bg-emerald-500" />
                  Online
                </span>
              ) : (
                <span className="text-xs text-muted-foreground">
                  {device.lastSeenAt
                    ? `Last seen ${relativeTime(device.lastSeenAt)}`
                    : `Offline`}
                </span>
              )}
              {device.kind === `server` && online && device.registered && (
                <Button
                  variant="ghost"
                  size="sm"
                  className={outdated ? `text-amber-500` : `text-muted-foreground`}
                  disabled={device.updateRequested || updatingId === device.deviceId}
                  title="Ask the daemon to self-update (it restarts when idle)"
                  onClick={() => void requestUpdate(device)}
                >
                  {device.updateRequested || updatingId === device.deviceId ? (
                    <>
                      <LoaderCircle className="animate-spin" />
                      Updating…
                    </>
                  ) : (
                    <>
                      <UpdateIcon />
                      Update
                    </>
                  )}
                </Button>
              )}
              <Button
                variant="outline"
                size="sm"
                disabled={runBusy || !online}
                onClick={() => onStartCoding(device.deviceId)}
              >
                <MonitorUp />
                Start coding
              </Button>
              {device.registered && (
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="ghost" className="h-5 w-5 p-0">
                      <MoreIcon className="size-4" />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem
                      onSelect={() => {
                        setRenameValue(device.deviceLabel || ``)
                        setRenameTarget(device)
                      }}
                    >
                      <RenameIcon />
                      Rename
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
          )
        })
      )}
      {sentTo && (
        <div className="flex items-center gap-1.5 px-3 py-2 text-xs text-muted-foreground">
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
              registers the machine here — remote starts and steering work
              exactly like on the desktop app.
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

      <Dialog
        open={renameTarget !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setRenameTarget(null)
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Rename machine</DialogTitle>
          </DialogHeader>
          <Input
            value={renameValue}
            onChange={(event) => setRenameValue(event.target.value)}
            maxLength={255}
            autoFocus
            onKeyDown={(event) => {
              if (event.key === `Enter`) void rename()
            }}
          />
          <DialogFooter>
            <Button variant="ghost" disabled={busy} onClick={() => setRenameTarget(null)}>
              Cancel
            </Button>
            <Button disabled={busy || !renameValue.trim()} onClick={() => void rename()}>
              {busy && <LoaderCircle className="animate-spin" />}
              Save
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

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
