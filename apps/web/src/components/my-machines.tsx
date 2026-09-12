// "My machines" (EXP-403): the caller's registered devices — desktops and
// headless `exponential` daemon servers — with live online state, last-seen
// fallback, and the "Add device" dialog (EXP-697: desktop download + the
// CLI install one-liner). Since EXP-481 the rows
// ride the synced devices shape (useRemoteStart composes them) and the ⋯
// menu collapses to Edit + Remove — rename, team sharing (EXP-432), agent
// defaults and worktree management all live in the Device settings dialog.
// Teammates' shared servers render read-only under "Team machines".
import { useMemo, useState } from "react"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import { conceptIcon } from "@/lib/icons.generated"
import { relativeTime } from "@/components/comment-rows/format"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  describeUpdateBlockers,
  deviceCanAgentLogin,
  deviceCanUpdateNow,
  deviceHasRunnableAgent,
  deviceIsMine,
  deviceIsOnline,
  deviceUnauthedAgentIds,
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
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  deviceAccountChips,
  deviceWorstHealth,
  healthBadgeLabel,
  type DeviceAccountChip,
} from "@/lib/agent-usage"
import { agentLabel } from "@/components/agent-usage-bar"
import { Button } from "@/components/ui/button"
import { Pill } from "@/components/ui/pill"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import {
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog"
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
const CopyIcon = conceptIcon(`ui-copy`)
const CheckIcon = conceptIcon(`ui-check`)
// EXP-792 (EXP-747): the cross-device usage page + the remote sign-in.
const SignInIcon = conceptIcon(`ui-sign-in`)
// EXP-849: the repair surface's account chips.
const SwapIcon = conceptIcon(`ui-swap`)

/** FEED-36: the tooltip on a queued Update button — the daemon's own rules
 * for getting there (every session ends, or one sits idle for 2 hours). */
export const QUEUED_UPDATE_TOOLTIP = `Live sessions hold this update — the machine restarts itself once every session ends or sits idle for 2 hours.`

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

/** EXP-747 A5: the agent a machine row's "Sign in" pill targets — the first
 * signed-out agent (EXP-849: every agent has a device-code flow). Null when
 * nothing is signed out, or the build cannot run `agent_login`. */
export function signInAgentFor(device: SteerDevice): string | null {
  if (!deviceIsOnline(device) || !deviceCanAgentLogin(device)) return null
  return deviceUnauthedAgentIds(device)[0] ?? null
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

// EXP-849: the Devices surface is the SETUP/REPAIR surface — one row per
// machine with its agents, worktrees and the accounts it holds. Accounts
// (the page's other section) decides WHICH login to run on; everything that
// touches a machine's credentials happens here: the worst health bubbles to
// the row's title, and every account it holds is a chip whose menu signs in,
// re-logins, or makes that login the one this machine uses.
//
// Nothing here ever copies a credential: a chip action queues either the
// machine's OWN `agent_login` (`AgentLoginDialog`, the agent CLI's login in
// that profile's config dir) or `agent_profile_use`, which only points the
// agent at a profile the machine already holds.
function MachineAccountChips({
  device,
  online,
}: {
  device: SteerDevice
  online: boolean
}) {
  const chips = deviceAccountChips({ agentAccounts: device.agentAccounts })
  if (chips.length === 0) return null
  // A repair is only offered on the caller's OWN machine, online, with the
  // `agent_login` cap — the same rule the sign-in pill uses.
  const canLogin =
    deviceIsMine(device) && online && deviceCanAgentLogin(device)
  return (
    <div className="mt-1 flex flex-wrap gap-1">
      {chips.map((chip) => (
        <MachineAccountChip
          key={chip.key}
          device={device}
          chip={chip}
          canLogin={canLogin}
        />
      ))}
    </div>
  )
}

/** `claude · dennis@…` with the active check and the health badge. */
function machineChipLabel(chip: DeviceAccountChip): string {
  const who =
    chip.email ??
    (chip.signedIn ? (chip.plan ?? `signed in`) : chip.profileLabel)
  return `${agentLabel(chip.agent)} · ${who}`
}

function MachineAccountChip({
  device,
  chip,
  canLogin,
}: {
  device: SteerDevice
  chip: DeviceAccountChip
  canLogin: boolean
}) {
  const [busy, setBusy] = useState(false)
  const health = healthBadgeLabel(chip.health)
  const body = (
    <>
      <span className="min-w-0 truncate">{machineChipLabel(chip)}</span>
      {chip.signedIn && chip.active && (
        <CheckIcon
          className="size-3 text-emerald-400"
          aria-label="The account this machine uses"
        />
      )}
      {health && (
        <span className="shrink-0 text-[10px] font-medium text-amber-500">
          {health}
        </span>
      )}
    </>
  )
  if (!canLogin) {
    return (
      <Pill size="sm" className="max-w-full" title={machineChipLabel(chip)}>
        {body}
      </Pill>
    )
  }
  // The ONE action per state: a broken or missing login is signed in again,
  // a healthy one that is not the machine's active login simply BECOMES it.
  // EXP-849: that second case is `agent_profile_use` — the machine points the
  // agent at a profile it already holds and re-heartbeats. Never a logout:
  // signing codex out would revoke the account server-wide, and never a
  // credential copy either (the files stay where the CLI wrote them).
  const switchesTo = chip.signedIn && !chip.active && chip.health !== `needs_relogin`
  const action = !chip.signedIn
    ? `Sign in`
    : chip.health === `needs_relogin`
      ? `Re-login`
      : chip.active
        ? `Sign in again`
        : `Use this account here`
  const useHere = async () => {
    if (busy) return
    setBusy(true)
    try {
      await trpc.devices.createCommand.mutate({
        deviceId: device.deviceId,
        kind: `agent_profile_use`,
        agent: chip.agent as never,
        profileId: chip.profileId,
      })
      toast.success(
        `${device.deviceLabel || device.deviceId} will use this ${agentLabel(chip.agent)} account`
      )
    } catch (error) {
      toast.error(`Couldn't switch the account on that machine`, {
        description: trpcErrorMessage(
          error,
          `The command could not be queued on the machine.`
        ),
      })
    } finally {
      setBusy(false)
    }
  }
  // The login dialog is hosted elsewhere in the tree — hand off a tick after
  // the menu closes (the Accounts section's rule).
  const signIn = () =>
    setTimeout(
      () =>
        requestAgentLogin({
          device,
          agent: chip.agent,
          profileId: chip.profileId,
        }),
      0
    )
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Pill
          size="sm"
          mode="action"
          className="max-w-full"
          title={`${machineChipLabel(chip)} — ${action} on ${device.deviceLabel || device.deviceId}`}
        >
          {body}
        </Pill>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuItem
          disabled={busy}
          onSelect={() => {
            if (switchesTo) void useHere()
            else signIn()
          }}
        >
          {switchesTo ? <SwapIcon /> : <SignInIcon />}
          {action}
        </DropdownMenuItem>
        {/* A switch is the cheap repair; the sign-in stays available under it
            for a login that turns out to be dead after all. */}
        {switchesTo && (
          <DropdownMenuItem onSelect={signIn}>
            <SignInIcon />
            Sign in again
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// The row's second line (native `deviceStatusLine` parity): a live dot +
// "Online" (amber + the signed-out agents when nothing is runnable, EXP-409),
// or the last-seen caption for offline machines.
export function DeviceStatusLine({
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
  const now = useNow()
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
        label="My machines"
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
          No machines yet. Open the Exponential desktop app, or add a device.
        </div>
      ) : (
        <div className="flex flex-col gap-0">
          {mine.map((device) => {
            const online = deviceIsOnline(device)
            // EXP-409: installed-but-signed-out agents grey the machine out
            // (online but nothing runnable) or annotate it (a runnable
            // sibling still covers coding).
            const unauthed = deviceUnauthedAgentIds(device)
            const runnable = deviceHasRunnableAgent(device)
            const signInNeeded = online && !runnable && unauthed.length > 0
            // EXP-836: play hands this machine to the Agent composer, which
            // only starts on an online machine WITH a runnable agent — so the
            // button gates on exactly that. It used to gate on `signInNeeded`
            // alone, so a machine reporting no agents at all (nothing
            // installed, an older build) opened the composer and the
            // pre-picked machine silently lost to the default one.
            const startable = online && runnable
            const KindIcon = device.kind === `server` ? ServerIcon : DesktopIcon
            const latest =
              device.kind === `server`
                ? latestVersions?.cli
                : latestVersions?.desktop
            const outdated = deviceUpdateAvailable(device.version, latest)
            // EXP-747 A5: a signed-out agent gets a Sign in pill in the
            // trailing column, wired to the remote login dialog.
            const signInAgent = unauthed.length > 0 ? signInAgentFor(device) : null
            // FEED-36: a queued update parked behind live sessions says
            // WHICH ones, and a capable daemon offers to end them now.
            const updateQueued = Boolean(
              device.updateRequested && device.updateBlocked
            )
            const blockerLine = updateQueued
              ? describeUpdateBlockers(blockersFor(device), usersById, now)
              : null
            // EXP-849: the worst health of the accounts this machine holds —
            // "needs re-login" is a DIFFERENT problem from "signed out", and
            // the chips below say which account it is. Null when every login
            // is fine (or the machine reported none).
            const healthBadge = healthBadgeLabel(
              deviceWorstHealth({ agentAccounts: device.agentAccounts }) ?? `ok`
            )
            return (
              <ListRow
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
                    {(device.sharedTeamIds?.length ?? 0) > 0 && (
                      <span
                        className="shrink-0 rounded-sm border border-border/60 px-1 text-[10px] text-muted-foreground"
                        title={
                          teamId && device.sharedTeamIds?.includes(teamId)
                            ? `Shared with this team — teammates can start coding sessions on this machine.`
                            : `Shared with other teams.`
                        }
                      >
                        Shared
                      </span>
                    )}
                    {healthBadge && (
                      <span
                        className="shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] font-medium text-amber-500"
                        title={`Sign in again from the account chip below.`}
                      >
                        {healthBadge}
                      </span>
                    )}
                  </div>
                  <DeviceStatusLine
                    online={online}
                    signInNeeded={signInNeeded}
                    unauthed={unauthed}
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
                  {/* EXP-849: the accounts this machine holds — the repair
                      controls live on these chips. */}
                  <MachineAccountChips device={device} online={online} />
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
                      title={`End this machine's live sessions and restart it on the new version now.`}
                    >
                      <UpdateIcon className="size-3" />
                      Update now…
                    </Pill>
                  )}
                  {signInAgent && (
                    <Pill
                      mode="action"
                      onClick={() =>
                        requestAgentLogin({ device, agent: signInAgent })
                      }
                    >
                      <SignInIcon className="size-3" />
                      Sign in
                    </Pill>
                  )}
                  <span
                    title={
                      signInNeeded
                        ? `Sign in to ${unauthed[0]} on this machine first.`
                        : online && !runnable
                          ? `No agent is signed in on this machine.`
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
                        <Button
                          variant="glass"
                          size="icon-sm"
                          aria-label={`Machine menu for ${device.deviceLabel || device.deviceId}`}
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
          <GlassSectionHeader label="Team machines" />
          <div className="flex flex-col gap-0">
            {teamShared.map((device) => {
              const online = deviceIsOnline(device)
              const unauthed = deviceUnauthedAgentIds(device)
              const runnable = deviceHasRunnableAgent(device)
              const signInNeeded = online && !runnable && unauthed.length > 0
              return (
                <ListRow
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
                  {/* The same fixed trailing column as "My machines": a
                      read-only row has no ⋯ menu, so its slot is an empty
                      spacer and the play buttons stay in one line. */}
                  <div className="flex shrink-0 items-center gap-1">
                    <Button
                      variant="glass"
                      size="icon"
                      // EXP-836: same predicate as own machines — the composer
                      // cannot start on a machine with no runnable agent.
                      disabled={!online || !runnable}
                      onClick={() => onStartCoding(device.deviceId)}
                      title={
                        online && !runnable
                          ? `No agent is signed in on this machine.`
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
              {`Update ${updateNowTarget?.deviceLabel || updateNowTarget?.deviceId || `this machine`} now?`}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {updateNowLiveCount > 0
                ? `Ends the ${updateNowLiveCount} live ${
                    updateNowLiveCount === 1 ? `session` : `sessions`
                  } on this machine (repo-backed runs can be resumed from their session page) and restarts it on the new version.`
                : `Ends every live session on this machine (repo-backed runs can be resumed from their session page) and restarts it on the new version.`}
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
            <DialogTitle>Remove machine</DialogTitle>
            <DialogDescription>
              Remove “{removeTarget?.deviceLabel || removeTarget?.deviceId}”
              from your machines? A machine with the daemon still running will
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
