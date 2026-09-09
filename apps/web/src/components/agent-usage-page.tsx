// EXP-792 (EXP-747 C1-C4): the cross-device usage page. One row per device ×
// agent profile off the synced devices rows (mine + the servers shared with
// the team), grouped by agent, attention first: signed-out rows lead, then
// anything at or over the danger threshold, then the rest. The windows are
// the same `AgentUsageCards` the device-settings tab renders; stale numbers
// keep the dimmed "as of …" treatment. "Refresh" queues `agent_usage_refresh`
// on one of MY machines that runs it (cap `agent-usage-refresh`), never more
// often than the device's own rate-limit floor.
import { useEffect, useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { Link } from "@tanstack/react-router"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { contract } from "@exp/domain-contract"
import type { Device } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import { deviceCollection } from "@/lib/collections"
import {
  agentProfileUsageRows,
  refreshAllowedAt,
  sortAttentionFirst,
  usageIsFresh,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import { deviceCanRefreshUsage, deviceRowIsOnline } from "@/lib/steer-devices"
import { useNow } from "@/hooks/use-now"
import { AgentUsageCards, agentLabel } from "@/components/agent-usage-bar"
import { relativeTime } from "@/components/comment-rows/format"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { Button } from "@/components/ui/button"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { Pill } from "@/components/ui/pill"
import { cn } from "@/lib/utils"

const BackIcon = conceptIcon(`ui-back`)
const RefreshIcon = conceptIcon(`ui-refresh`)
const OfflineIcon = conceptIcon(`ui-device-offline`)

/** How long a queued refresh shows as in flight before giving up on the
 * device answering (it answers by re-reporting on its next beat). */
const REFRESH_PENDING_MS = 45_000

export function AgentUsagePage({
  teamSlug,
  teamId,
  currentUserId,
}: {
  teamSlug: string
  teamId: string
  currentUserId: string
}) {
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
  const rows = useMemo(
    () =>
      sortAttentionFirst(
        agentProfileUsageRows(devices, currentUserId, (seen) =>
          deviceRowIsOnline(seen, now)
        )
      ),
    [devices, currentUserId, now]
  )
  // Contract agent order first, anything else after — an agent group only
  // renders when a machine reported it.
  const groups = useMemo(() => {
    const order = [...contract.codingAgent.values] as string[]
    const byAgent = new Map<string, AgentProfileUsageRow[]>()
    for (const row of rows) {
      const list = byAgent.get(row.agent) ?? []
      list.push(row)
      byAgent.set(row.agent, list)
    }
    const keys = [...byAgent.keys()].sort((a, b) => {
      const ia = order.indexOf(a)
      const ib = order.indexOf(b)
      if (ia === -1 && ib === -1) return a.localeCompare(b)
      if (ia === -1) return 1
      if (ib === -1) return -1
      return ia - ib
    })
    return keys.map((agent) => ({ agent, rows: byAgent.get(agent)! }))
  }, [rows])

  // Refreshes in flight, keyed by row: the stamp the row carried when it was
  // queued — the device's re-report moves it, which clears the spinner.
  const [refreshing, setRefreshing] = useState<
    Record<string, { fetchedAt: string | null; at: number }>
  >({})
  useEffect(() => {
    const keys = Object.keys(refreshing)
    if (keys.length === 0) return
    const next = { ...refreshing }
    let changed = false
    for (const key of keys) {
      const row = rows.find((candidate) => candidate.key === key)
      const stamp = row?.usage?.fetchedAt ?? null
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
  }, [rows, refreshing, now])

  const refresh = async (row: AgentProfileUsageRow) => {
    setRefreshing((current) => ({
      ...current,
      [row.key]: { fetchedAt: row.usage?.fetchedAt ?? null, at: Date.now() },
    }))
    try {
      await trpc.devices.createCommand.mutate({
        deviceId: row.deviceId,
        kind: `agent_usage_refresh`,
        agent: row.agent as (typeof contract.codingAgent.values)[number],
        profileId: row.profileId,
      })
    } catch (error) {
      setRefreshing((current) => {
        const next = { ...current }
        delete next[row.key]
        return next
      })
      toast.error(`Couldn't refresh the usage`, {
        description: trpcErrorMessage(
          error,
          `The refresh could not be queued on the machine.`
        ),
      })
    }
  }

  return (
    <div className="h-full overflow-y-auto">
      <div
        className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        <div className="mb-4 flex items-center gap-2">
          <Button asChild variant="ghost" size="icon-sm" aria-label="Devices">
            <Link to="/t/$teamSlug/devices" params={{ teamSlug }}>
              <BackIcon />
            </Link>
          </Button>
          <h1 className="text-base font-semibold">Usage</h1>
        </div>

        {deviceRows === undefined ? (
          <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
        ) : groups.length === 0 ? (
          <div className="flex items-center gap-2 px-1 py-3 text-xs text-muted-foreground">
            <OfflineIcon className="size-3.5 shrink-0" />
            No machine has reported an agent account yet.
          </div>
        ) : (
          groups.map((group) => (
            <div key={group.agent} className="mb-6">
              <GlassSectionHeader label={agentLabel(group.agent)} />
              <div className="flex flex-col gap-2">
                {group.rows.map((row) => (
                  <UsageRow
                    key={row.key}
                    row={row}
                    now={now}
                    canRefresh={
                      row.mine &&
                      row.online &&
                      deviceCanRefreshUsage({
                        caps: capsByDevice.get(row.deviceId) ?? [],
                      })
                    }
                    refreshing={row.key in refreshing}
                    onRefresh={() => void refresh(row)}
                  />
                ))}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  )
}

function UsageRow({
  row,
  now,
  canRefresh,
  refreshing,
  onRefresh,
}: {
  row: AgentProfileUsageRow
  now: Date
  canRefresh: boolean
  refreshing: boolean
  onRefresh: () => void
}) {
  const fresh = usageIsFresh(row.usage, now)
  const hasWindows = (row.usage?.windows.length ?? 0) > 0
  const nextAllowed = refreshAllowedAt(row.usage, now)
  const asOf = row.usage?.fetchedAt ?? row.checkedAt
  const caption = !row.signedIn
    ? `Not signed in`
    : (row.email ?? row.plan ?? `signed in`)
  return (
    <GlassRow className="flex-col items-stretch gap-2">
      <div className="flex items-center gap-2">
        <span
          className={cn(
            `size-1.5 shrink-0 rounded-full`,
            row.online ? `bg-emerald-500` : `bg-muted-foreground/40`
          )}
          aria-hidden
        />
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline gap-1.5">
            <span className="min-w-0 truncate text-sm font-medium">
              {row.deviceLabel || row.deviceId}
            </span>
            <span className="shrink-0 text-xs text-muted-foreground">
              {row.profileLabel}
            </span>
            {row.active && (
              <Pill size="sm" title="The profile this machine runs by default">
                active
              </Pill>
            )}
          </div>
          <div
            className={cn(
              `truncate text-xs`,
              row.signedIn ? `text-muted-foreground` : `text-amber-500`
            )}
          >
            {caption}
            {row.signedIn && row.email && row.plan && (
              <span className="text-muted-foreground/60">{` · ${row.plan}`}</span>
            )}
          </div>
        </div>
        {canRefresh && (
          <span
            title={
              refreshing
                ? `Waiting for the machine…`
                : nextAllowed
                  ? `Refreshed recently. Next refresh at ${nextAllowed.toLocaleTimeString(undefined, { hour: `2-digit`, minute: `2-digit` })}.`
                  : `Re-read this profile's usage on the machine`
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
      {hasWindows && row.usage ? (
        <div className={cn(!fresh && `opacity-50`)}>
          <AgentUsageCards usage={row.usage} now={now} compact />
          {!fresh && !row.usage.stale && asOf && (
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
    </GlassRow>
  )
}
