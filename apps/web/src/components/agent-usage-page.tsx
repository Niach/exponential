// EXP-792 (EXP-747 C1-C4): the cross-device usage page. EXP-817: one card
// per ACCOUNT (an agent plus the login the machines named) off the synced
// devices rows (mine + the servers shared with the team), grouped by agent,
// attention first: signed-out cards lead, then anything at or over the
// danger threshold, then the rest. The machines holding the account are
// chips on the card; the numbers are the FRESHEST machine's report (they are
// the account's limits, so every machine reads the same ones). The windows
// are the same `AgentUsageCards` the device-settings tab renders, in the
// dense rhythm, laid out as a grid so every account fits on one screen;
// stale numbers keep the dimmed "as of …" treatment.
//
// "Refresh" queues `agent_usage_refresh` on one of MY machines that runs it
// (cap `agent-usage-refresh`), never more often than the device's own
// rate-limit floor. EXP-817: while the page is open it does that BY ITSELF
// for every account whose freshest report is past the floor, so the numbers
// on screen are never older than ~5 minutes on a machine that answers.
import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { contract } from "@exp/domain-contract"
import type { Device } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { deviceCollection } from "@/lib/collections"
import {
  accountUsageGroups,
  agentProfileUsageRows,
  refreshAllowedAt,
  sortAccountGroupsAttentionFirst,
  usageIsFresh,
  SYSTEM_PROFILE_ID,
  type AgentAccountUsageGroup,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceCanAgentLogin,
  deviceCanRefreshUsage,
  deviceRowIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useNow } from "@/hooks/use-now"
import { AgentUsageCards, agentLabel } from "@/components/agent-usage-bar"
import { relativeTime } from "@/components/comment-rows/format"
import { Button } from "@/components/ui/button"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import { Pill } from "@/components/ui/pill"
import { cn } from "@/lib/utils"

const RefreshIcon = conceptIcon(`ui-refresh`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const CheckIcon = conceptIcon(`ui-check`)
const SwapIcon = conceptIcon(`ui-swap`)
const SignInIcon = conceptIcon(`ui-sign-in`)

/** How long a queued refresh shows as in flight before giving up on the
 * device answering (it answers by re-reporting on its next beat). */
const REFRESH_PENDING_MS = 45_000

/** EXP-817: the page's own refresh never re-tries one account faster than
 * this — a queued command the machine has not answered yet is a CONFLICT on
 * the server, and hammering it buys nothing. */
const AUTO_REFRESH_RETRY_MS = 60_000

/** The chip dot colours — the same pair the device lists use. */
const ONLINE_DOT = `var(--color-emerald-500)`
const OFFLINE_DOT = `color-mix(in oklab, var(--color-muted-foreground) 40%, transparent)`

/** EXP-818: the Devices page's Accounts section — the Usage page, folded in.
 *  One row per agent account under a group band; the machine chips carry a
 *  check where the account is the ACTIVE login and open a sign-in / switch
 *  menu on one of the caller's own machines (`AgentLoginDialog`). */
export function AgentAccountsSection({
  teamSlug,
  teamId,
  currentUserId,
}: {
  teamSlug: string
  teamId: string
  currentUserId: string
}) {
  void teamSlug
  // Every synced row: own machines + the servers teammates shared with the
  // team (the shape is already server-scoped to exactly that).
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const now = useNow(30_000)
  const devices = useMemo(
    () =>
      ((deviceRows ?? []) as Device[]).filter(
        (row) =>
          row.userId === currentUserId ||
          (row.sharedTeamId === teamId && row.kind === `server`)
      ),
    [deviceRows, currentUserId, teamId]
  )
  const capsByDevice = useMemo(
    () => new Map(devices.map((row) => [row.deviceId, row.caps ?? []])),
    [devices]
  )
  const groups = useMemo(() => {
    const rows = agentProfileUsageRows(devices, currentUserId, (seen) =>
      deviceRowIsOnline(seen, now)
    )
    return sortAccountGroupsAttentionFirst(
      accountUsageGroups(
        rows,
        (row) =>
          row.mine &&
          row.online &&
          deviceCanRefreshUsage({ caps: capsByDevice.get(row.deviceId) ?? [] })
      )
    )
  }, [devices, capsByDevice, currentUserId, now])
  // Contract agent order first, anything else after — an agent section only
  // renders when a machine reported it.
  const sections = useMemo(() => {
    const order = [...contract.codingAgent.values] as string[]
    const byAgent = new Map<string, AgentAccountUsageGroup[]>()
    for (const group of groups) {
      const list = byAgent.get(group.agent) ?? []
      list.push(group)
      byAgent.set(group.agent, list)
    }
    const keys = [...byAgent.keys()].sort((a, b) => {
      const ia = order.indexOf(a)
      const ib = order.indexOf(b)
      if (ia === -1 && ib === -1) return a.localeCompare(b)
      if (ia === -1) return 1
      if (ib === -1) return -1
      return ia - ib
    })
    return keys.map((agent) => ({ agent, groups: byAgent.get(agent)! }))
  }, [groups])

  // Refreshes in flight, keyed by account: the stamp the account carried
  // when it was queued — the device's re-report moves it, which clears the
  // spinner.
  const [refreshing, setRefreshing] = useState<
    Record<string, { fetchedAt: string | null; at: number }>
  >({})
  useEffect(() => {
    const keys = Object.keys(refreshing)
    if (keys.length === 0) return
    const next = { ...refreshing }
    let changed = false
    for (const key of keys) {
      const group = groups.find((candidate) => candidate.key === key)
      const stamp = group?.usage?.fetchedAt ?? null
      const entry = refreshing[key]!
      if (
        stamp !== entry.fetchedAt ||
        Date.now() - entry.at > REFRESH_PENDING_MS
      ) {
        delete next[key]
        changed = true
      }
    }
    if (changed) setRefreshing(next)
  }, [groups, refreshing, now])

  const refresh = async (group: AgentAccountUsageGroup, silent: boolean) => {
    const target = group.refreshTarget
    if (!target) return
    setRefreshing((current) => ({
      ...current,
      [group.key]: { fetchedAt: group.usage?.fetchedAt ?? null, at: Date.now() },
    }))
    try {
      await trpc.devices.createCommand.mutate({
        deviceId: target.deviceId,
        kind: `agent_usage_refresh`,
        agent: target.agent as (typeof contract.codingAgent.values)[number],
        profileId: target.profileId,
      })
    } catch (error) {
      setRefreshing((current) => {
        const next = { ...current }
        delete next[group.key]
        return next
      })
      // The page's own refresh fails quietly: a command still queued from
      // the last round is a CONFLICT, and the next tick simply looks again.
      if (silent) return
      toast.error(`Couldn't refresh the usage`, {
        description: trpcErrorMessage(
          error,
          `The refresh could not be queued on the machine.`
        ),
      })
    }
  }

  // EXP-817: keep the page current while it is open. Every tick, each
  // account with an eligible machine and a freshest report past the floor
  // gets ONE refresh queued — never while one is in flight, never twice
  // inside `AUTO_REFRESH_RETRY_MS`. The floor is the device's own 429
  // budget, so this can never out-poll what the machine allows itself.
  const autoAttempts = useRef(new Map<string, number>())
  useEffect(() => {
    const at = Date.now()
    for (const group of groups) {
      if (!group.refreshTarget) continue
      if (group.key in refreshing) continue
      if (refreshAllowedAt(group.usage, now) !== null) continue
      const last = autoAttempts.current.get(group.key) ?? 0
      if (at - last < AUTO_REFRESH_RETRY_MS) continue
      autoAttempts.current.set(group.key, at)
      void refresh(group, true)
    }
    // `refreshing` is read, not a trigger: a mark appearing or clearing must
    // not start another round on its own.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [groups, now])

  const autoRefreshes = groups.some((group) => group.refreshTarget !== null)

  const ownDevices = useMemo(
    () => new Map(devices.filter((row) => row.userId === currentUserId).map((row) => [row.deviceId, row])),
    [devices, currentUserId]
  )

  return (
    <div className="mb-6">
      <GlassSectionHeader
        label="Accounts"
        trailing={
          autoRefreshes ? (
            <span className="text-[11px] text-muted-foreground">
              Refreshes every 5 minutes
            </span>
          ) : undefined
        }
      />
      {deviceRows === undefined ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">Loading…</div>
      ) : sections.length === 0 ? (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-muted-foreground">
          <OfflineIcon className="size-3.5 shrink-0" />
          No machine has reported an agent account yet.
        </div>
      ) : (
        sections.map((section) => (
          <div key={section.agent} className="mb-1">
            <div className="px-3 pt-2 pb-1 text-xs font-medium text-muted-foreground">
              {agentLabel(section.agent)}
            </div>
            <div className="flex flex-col">
              {section.groups.map((group) => (
                <AccountCard
                  key={group.key}
                  group={group}
                  now={now}
                  ownDevices={ownDevices}
                  refreshing={group.key in refreshing}
                  onRefresh={() => void refresh(group, false)}
                />
              ))}
            </div>
          </div>
        ))
      )}
    </div>
  )
}

/** The `Studio · Personal` chip text: the machine, plus the profile when it
 * is not the ambient login. */
function chipLabel(row: AgentProfileUsageRow): string {
  const device = row.deviceLabel || row.deviceId
  return row.profileId === SYSTEM_PROFILE_ID
    ? device
    : `${device} · ${row.profileLabel}`
}

/** What hovering a chip says: this machine's OWN report age and state. */
function chipTitle(row: AgentProfileUsageRow, now: Date): string {
  const parts: string[] = [row.online ? `online` : `offline`]
  if (!row.signedIn) parts.push(`not signed in`)
  const stamp = row.usage?.fetchedAt ?? row.checkedAt
  if (stamp) {
    parts.push(
      usageIsFresh(row.usage, now)
        ? `reported ${relativeTime(stamp)}`
        : `as of ${relativeTime(stamp)}`
    )
  } else {
    parts.push(`no usage reported`)
  }
  return parts.join(` · `)
}

function AccountCard({
  group,
  now,
  ownDevices,
  refreshing,
  onRefresh,
}: {
  group: AgentAccountUsageGroup
  now: Date
  /** The caller's own synced devices rows by device id — a chip of one of
   *  these may open the sign-in menu. */
  ownDevices: Map<string, Device>
  refreshing: boolean
  onRefresh: () => void
}) {
  const fresh = usageIsFresh(group.usage, now)
  const hasWindows = (group.usage?.windows.length ?? 0) > 0
  const nextAllowed = refreshAllowedAt(group.usage, now)
  const asOf = group.usage?.fetchedAt ?? group.checkedAt
  const caption = !group.signedIn
    ? `Not signed in`
    : (group.email ?? group.plan ?? `signed in`)
  return (
    <ListRow className="flex-col items-stretch gap-1.5 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div
            className={cn(
              `truncate text-sm font-medium`,
              !group.signedIn && `text-amber-500`
            )}
            title={caption}
          >
            {caption}
            {group.signedIn && group.email && group.plan && (
              <span className="font-normal text-muted-foreground/60">{` · ${group.plan}`}</span>
            )}
          </div>
        </div>
        {group.refreshTarget && (
          <span
            title={
              refreshing
                ? `Waiting for ${group.refreshTarget.deviceLabel || `the machine`}…`
                : nextAllowed
                  ? `Refreshed recently. Next refresh at ${nextAllowed.toLocaleTimeString(undefined, { hour: `2-digit`, minute: `2-digit` })}.`
                  : `Re-read this account's usage on ${group.refreshTarget.deviceLabel || `the machine`}`
            }
          >
            <Button
              variant="glass"
              size="icon-sm"
              aria-label="Refresh usage"
              disabled={refreshing || nextAllowed !== null}
              onClick={onRefresh}
            >
              {refreshing ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <RefreshIcon />
              )}
            </Button>
          </span>
        )}
      </div>
      <div className="flex flex-wrap gap-1">
        {group.rows.map((row) => (
          <DeviceChip
            key={row.key}
            row={row}
            now={now}
            device={ownDevices.get(row.deviceId) ?? null}
          />
        ))}
      </div>
      {hasWindows && group.usage ? (
        <div className={cn(`pt-0.5`, !fresh && `opacity-50`)}>
          <AgentUsageCards usage={group.usage} now={now} compact dense />
          {!fresh && !group.usage.stale && asOf && (
            <p className="mt-1 text-[11px] text-muted-foreground">
              as of {relativeTime(asOf)}
            </p>
          )}
        </div>
      ) : (
        <p className="text-[11px] text-muted-foreground">
          {asOf ? `No usage reported · as of ${relativeTime(asOf)}` : `No usage reported`}
        </p>
      )}
    </ListRow>
  )
}

/** EXP-818: one machine chip — the online dot, the machine (· profile), and a
 * CHECK when the account is the ACTIVE login on that machine. A chip of one
 * of MY machines that can run a sign-in opens a menu: "Switch account on X"
 * (signed in) or "Sign in on X" (`canLogin` — the device-agent-account rule:
 * online, the `agent-login` cap, never pi). */
function DeviceChip({
  row,
  now,
  device,
}: {
  row: AgentProfileUsageRow
  now: Date
  device: Device | null
}) {
  const label = chipLabel(row)
  const title = chipTitle(row, now)
  const body = (
    <>
      {label}
      {row.signedIn && row.active && (
        <CheckIcon className="size-3 text-emerald-400" aria-label="Active on this machine" />
      )}
    </>
  )
  const canLogin =
    device !== null &&
    row.online &&
    row.agent !== `pi` &&
    deviceCanAgentLogin({ caps: device.caps ?? [] })
  if (!canLogin) {
    return (
      <Pill
        size="sm"
        dot={row.online ? ONLINE_DOT : OFFLINE_DOT}
        title={title}
        className={cn(!row.online && `text-muted-foreground`)}
      >
        {body}
      </Pill>
    )
  }
  const action = row.signedIn ? `Switch account` : `Sign in`
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Pill
          size="sm"
          mode="action"
          dot={row.online ? ONLINE_DOT : OFFLINE_DOT}
          title={title}
        >
          {body}
        </Pill>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start">
        <DropdownMenuItem
          onSelect={() =>
            requestAgentLogin({
              device: {
                deviceId: device!.deviceId,
                deviceLabel: device!.label ?? device!.deviceId,
                caps: device!.caps ?? [],
              } as SteerDevice,
              agent: row.agent,
            })
          }
        >
          {row.signedIn ? <SwapIcon className="size-4" /> : <SignInIcon className="size-4" />}
          {`${action} on ${row.deviceLabel || row.deviceId}`}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
