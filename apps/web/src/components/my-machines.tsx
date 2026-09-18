// "My devices" (EXP-403): the caller's registered devices — desktops and
// headless `exponential` daemon servers — with live online state, last-seen
// fallback, and the "Add device" dialog (EXP-697: desktop download + the
// CLI install one-liner). Since EXP-481 the rows ride the synced devices
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
// EXP-909: each device LISTS its logins underneath (`DeviceLogins`), editable
// on the caller's own machines and read-only on a teammate's shared server.
// That fold replaced the cross-device Accounts section, so this list also
// carries the 30 s refresh loop that used to live there.
//
// EXP-909 follow-up: a row carries ONE control ×4 — the settings gear, on the
// caller's own REGISTERED devices, hidden until the row is hovered (always on
// phones). No play button (the Agent page composer's device picker is the one
// way to start a run), no ⋯ menu, no inline update controls, and no spacers
// standing in for them. A team device row has no control at all.
import { useCallback, useMemo, useState } from "react"
import { inArray, useLiveQuery } from "@tanstack/react-db"
import {
  conceptIcon,
  getDeviceIcon,
  Button,
  Pill,
  GlassSectionHeader,
  ListRow,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  LiveDot,
} from "@exp/ui"
import { relativeTime } from "@/components/comment-rows/format"
import { cn } from "@/lib/utils"
import {
  describeUpdateBlockers,
  deviceCanRefreshUsage,
  deviceHasRunnableAgent,
  deviceIsMine,
  deviceIsOnline,
  deviceUpdateAvailable,
  liveUpdateBlockers,
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
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"

// This is a MULTI-CLIENT surface (iOS/Android/desktop render the same list)
// — concepts, never raw lucide glyphs (CLAUDE.md icon rule).
const OfflineIcon = conceptIcon(`ui-device-offline`)
const DefaultIcon = conceptIcon(`ui-device-default`)
const AddIcon = conceptIcon(`ui-add`)
// EXP-909 follow-up: the row's ONE control is the settings gear, ×4.
const SettingsIcon = conceptIcon(`nav-settings`)
const CopyIcon = conceptIcon(`ui-copy`)
const CheckIcon = conceptIcon(`ui-check`)
// EXP-944: the fold chevron — the same affordance the session tree uses.
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)

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
  onChanged,
  latestVersions,
  teamId,
}: {
  devices: SteerDevice[] | null
  onChanged: () => void
  latestVersions: { desktop: string | null; cli: string | null } | null
  /** EXP-432: the current team — the share toggle's target. */
  teamId?: string
}) {
  const [addServerOpen, setAddServerOpen] = useState(false)
  const [settingsTargetId, setSettingsTargetId] = useState<string | null>(null)
  // EXP-944: devices COLLAPSE. The list answers "which machines do I have and
  // are they up" first; a machine's logins, their usage bars and its "Add
  // account" are the second question, and three machines' worth of them made
  // the first one unreadable. Session-only state: a fold is not a setting.
  const [expandedIds, setExpandedIds] = useState<ReadonlySet<string>>(
    () => new Set()
  )
  const toggleExpanded = (deviceId: string) =>
    setExpandedIds((current) => {
      const next = new Set(current)
      if (!next.delete(deviceId)) next.add(deviceId)
      return next
    })
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
  // EXP-875: the caller's half of the verdict is the MACHINE's — online and
  // advertising the cap. Whether the LOGIN could ever answer (mine, signed
  // in, monitored) is the hook's own `usageRefreshEligible`, so no surface
  // can queue a command for a login that can only report nothing.
  const canRefresh = useCallback(
    (row: AgentProfileUsageRow) =>
      row.online &&
      deviceCanRefreshUsage({ caps: capsByDevice.get(row.deviceId) ?? [] }),
    [capsByDevice]
  )
  useAgentUsageRefresh(ownLogins, canRefresh, now)
  // Re-resolved each render so the dialog always edits the LIVE synced row.
  const settingsTarget =
    mine?.find((device) => device.deviceId === settingsTargetId) ?? null
  // FEED-36: the dialog's "Update now" confirmation counts the caller's live
  // sessions on that machine — one query for the page, not one per row.
  const settingsLiveSessions = settingsTarget
    ? liveUpdateBlockers(blockersFor(settingsTarget), now).length
    : 0

  const origin = useMemo(
    () =>
      typeof window === `undefined`
        ? `https://app.exponential.at`
        : window.location.origin,
    []
  )
  const snippet = buildServerInstallSnippet(origin)
  const userAgent = typeof navigator === `undefined` ? `` : navigator.userAgent

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
            const KindIcon = getDeviceIcon(device)
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
            const expanded = expandedIds.has(device.deviceId)
            return (
              <div
                key={device.deviceId}
                className={cn(
                  `flex flex-col`,
                  online && !runnable && `opacity-60`
                )}
              >
                <ListRow
                  interactive
                  aria-expanded={expanded}
                  onClick={() => toggleExpanded(device.deviceId)}
                  data-testid={`device-row-${device.deviceId}`}
                  // `group` = the hover scope the trailing gear fades in on.
                  className="group gap-2"
                >
                  {/* EXP-944: the whole row folds, so the chevron is an
                      affordance, not a second target. */}
                  <span
                    aria-hidden
                    className="flex shrink-0 items-center justify-center text-muted-foreground"
                  >
                    {expanded ? (
                      <ChevronDownIcon className="size-3" />
                    ) : (
                      <ChevronRightIcon className="size-3" />
                    )}
                  </span>
                  <KindIcon className="size-4 shrink-0 text-foreground/70" />
                  {/* FEED-15: the native two-line row — name + version (+ Shared)
                      on top, live/last-seen state beneath, the gear trailing. */}
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
                  </div>
                  {/* EXP-909 follow-up: the ONE trailing control — a ghost
                      settings gear (no circle, no border), on the caller's own
                      REGISTERED devices only. ≥md it fades in on row hover or
                      keyboard focus; on phones it is always painted. An
                      unregistered row gets nothing at all — no spacer. */}
                  {device.registered && (
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      className="shrink-0 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 max-md:opacity-100"
                      aria-label={`Device settings for ${device.deviceLabel || device.deviceId}`}
                      title="Device settings"
                      onClick={(event) => {
                        // The row folds; the gear opens settings.
                        event.stopPropagation()
                        setSettingsTargetId(device.deviceId)
                      }}
                    >
                      <SettingsIcon />
                    </Button>
                  )}
                </ListRow>
                {/* EXP-909: the logins this device holds — every repair control
                    lives on these rows. EXP-944: they live in the FOLD now, and
                    the icon and the gear above stay level with the name instead
                    of centring themselves against the whole block. The inset
                    lines the logins up under the device name. */}
                {expanded && (
                  <div className="pr-3 pb-2 pl-14">
                    <DeviceLogins device={device} now={now} />
                  </div>
                )}
              </div>
            )
          })}
        </div>
      )}
      {/* EXP-432: teammates' server devices shared with this team — read-only
          rows (owner name in the tooltip, no rename/remove/update). The Agent
          page composer is what aims a run at one. */}
      {teamShared.length > 0 && (
        <div className="mt-6">
          <GlassSectionHeader label="Team devices" />
          <div className="flex flex-col gap-0">
            {teamShared.map((device) => {
              const online = deviceIsOnline(device)
              const runnable = deviceHasRunnableAgent(device)
              const KindIcon = getDeviceIcon(device)
              return (
                <ListRow
                  key={device.deviceId}
                  interactive
                  className={online && !runnable ? `opacity-60` : undefined}
                >
                  <KindIcon className="size-4 shrink-0 text-foreground/70" />
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
                  {/* EXP-909 follow-up: no trailing control at all — a
                      teammate's machine is not ours to settle, and starting a
                      run on it goes through the Agent page composer. */}
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

      {/* EXP-909 follow-up: Update and Remove live in here now, as the
          dialog's last two sections. */}
      <DeviceSettingsDialog
        device={settingsTarget}
        open={settingsTarget !== null}
        onOpenChange={(open) => {
          if (!open) setSettingsTargetId(null)
        }}
        latestVersions={latestVersions}
        liveSessionCount={settingsLiveSessions}
        onChanged={onChanged}
      />
    </div>
  )
}
