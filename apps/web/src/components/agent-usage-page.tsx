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
  healthBadgeLabel,
  refreshAllowedAt,
  sortAccountGroupsAttentionFirst,
  usageIsFresh,
  SYSTEM_PROFILE_ID,
  type AgentAccountUsageGroup,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceCanRefreshUsage,
  deviceRowIsOnline,
} from "@/lib/steer-devices"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import { AddAccountDialog } from "@/components/add-account-dialog"
import {
  addAccountBlockReason,
  addAccountDevices,
  addAccountLoginTarget,
  clampProfileLabel,
} from "@/lib/agent-account-add"
import { steerDeviceFromRow } from "@/lib/steer-devices"
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { cn } from "@/lib/utils"

const RefreshIcon = conceptIcon(`ui-refresh`)
const OfflineIcon = conceptIcon(`ui-device-offline`)
const CheckIcon = conceptIcon(`ui-check`)
const SignInIcon = conceptIcon(`ui-sign-in`)
const AddIcon = conceptIcon(`ui-add`)

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
          ((row.sharedTeamIds ?? []).includes(teamId) && row.kind === `server`)
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
  // EXP-849: per-AGENT tabs. Stacked agent bands meant codex's two rows
  // pushed claude's (the ones with the limits anyone watches) off the first
  // screen; one agent at a time, claude first by contract order. A single
  // reported agent shows no tab strip at all.
  const [agentTab, setAgentTab] = useState<string | null>(null)
  const activeAgent =
    sections.find((section) => section.agent === agentTab)?.agent ??
    sections[0]?.agent ??
    null
  const activeSection =
    sections.find((section) => section.agent === activeAgent) ?? null

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
  // EXP-827: "Add account" — the Add-device twin in the header. EXP-845: it
  // never hides. A control that vanishes reads as a feature that is not there;
  // a DISABLED one with the reason ("None of those machines is online right
  // now") reads as a machine that is off.
  const [addOpen, setAddOpen] = useState(false)
  const addBlocked = useMemo(
    () => addAccountBlockReason(devices, { currentUserId, now }),
    [devices, currentUserId, now]
  )

  return (
    // EXP-827: `#accounts` is where device settings' Usage button lands.
    <div className="mb-6 scroll-mt-4" id="accounts">
      <GlassSectionHeader
        label="Accounts"
        trailing={
          <>
            {autoRefreshes && (
              <span className="text-[11px] text-muted-foreground">
                Refreshes every 5 minutes
              </span>
            )}
            <span title={addBlocked ?? undefined}>
              <Pill
                mode="action"
                disabled={addBlocked !== null}
                onClick={() => setAddOpen(true)}
                data-testid="add-account-button"
              >
                <AddIcon className="size-3" />
                Add account
              </Pill>
            </span>
          </>
        }
      />
      <AddAccountDialog
        open={addOpen}
        onOpenChange={setAddOpen}
        devices={devices}
        currentUserId={currentUserId}
        now={now}
      />
      {deviceRows === undefined ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">Loading…</div>
      ) : sections.length === 0 ? (
        <div className="flex items-center gap-2 px-3 py-2 text-xs text-muted-foreground">
          <OfflineIcon className="size-3.5 shrink-0" />
          No machine has reported an agent account yet.
        </div>
      ) : (
        <div className="mb-1">
          {sections.length > 1 && (
            <Tabs
              value={activeAgent ?? ``}
              onValueChange={setAgentTab}
              className="px-1 pt-2 pb-1"
            >
              <TabsList>
                {sections.map((section) => (
                  <TabsTrigger key={section.agent} value={section.agent}>
                    {agentLabel(section.agent)}
                  </TabsTrigger>
                ))}
              </TabsList>
            </Tabs>
          )}
          <div className="flex flex-col">
            {(activeSection?.groups ?? []).map((group) => (
              <AccountCard
                key={group.key}
                group={group}
                now={now}
                ownDevices={ownDevices}
                currentUserId={currentUserId}
                refreshing={group.key in refreshing}
                onRefresh={() => void refresh(group, false)}
              />
            ))}
          </div>
        </div>
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
  else if (row.health === `needs_relogin`) parts.push(`needs re-login here`)
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
  currentUserId,
  refreshing,
  onRefresh,
}: {
  group: AgentAccountUsageGroup
  now: Date
  /** The caller's own synced devices rows by device id — a chip of one of
   *  these may open the sign-in menu. */
  ownDevices: Map<string, Device>
  currentUserId: string
  refreshing: boolean
  onRefresh: () => void
}) {
  const fresh = usageIsFresh(group.usage, now)
  // EXP-827: "add a machine to this account" — my online machines with the
  // agent installed that do NOT hold the account yet. Only a NAMED account
  // (an email) can be added elsewhere; the machine reports the login under
  // the same email and the chip joins this card.
  const addTargets = useMemo(
    () =>
      group.signedIn && group.email
        ? addAccountDevices([...ownDevices.values()], {
            currentUserId,
            now,
            agent: group.agent,
            exclude: group.rows.map((row) => row.deviceId),
          })
        : [],
    [group, ownDevices, currentUserId, now]
  )
  // EXP-845: the "+" stays put when no machine can take the login — disabled,
  // with the reason. Only an account nobody could ever add elsewhere (no
  // email to sign in WITH) has no control at all.
  const addBlocked = useMemo(
    () =>
      group.signedIn && group.email
        ? addAccountBlockReason([...ownDevices.values()], {
            currentUserId,
            now,
            agent: group.agent,
            agentLabel: agentLabel(group.agent),
            exclude: group.rows.map((row) => row.deviceId),
          })
        : null,
    [group, ownDevices, currentUserId, now]
  )
  const showAdd = group.signedIn === true && Boolean(group.email)
  const addTo = (row: Device) => {
    const target = addAccountLoginTarget(
      row,
      group.agent,
      clampProfileLabel(group.email ?? group.plan ?? agentLabel(group.agent))
    )
    const device = steerDeviceFromRow(row, { now, currentUserId })
    // The dialog is hosted elsewhere in the tree — open it after the menu
    // closed (same handoff as `DeviceChip`).
    setTimeout(
      () => requestAgentLogin({ device, agent: group.agent, ...target }),
      0
    )
  }
  const hasWindows = (group.usage?.windows.length ?? 0) > 0
  const nextAllowed = refreshAllowedAt(group.usage, now)
  const asOf = group.usage?.fetchedAt ?? group.checkedAt
  const caption = !group.signedIn
    ? `Not signed in`
    : (group.email ?? group.plan ?? `signed in`)
  // EXP-849: HEALTH, not just "signed in" — an expired credential reads
  // `Needs re-login` (the CLI still claims signed in, the probe came back
  // Unauthorized), a missing one `Signed out`. The repair lives on the
  // Devices row that holds it; this row is the decision surface.
  const health = healthBadgeLabel(group.health)
  return (
    <ListRow className="flex-col items-stretch gap-1.5 px-3 py-2.5">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div
            className={cn(
              `flex min-w-0 items-center gap-1.5 text-sm font-medium`,
              !group.signedIn && `text-amber-500`
            )}
          >
            <span className="min-w-0 truncate" title={caption}>
              {caption}
              {group.signedIn && group.email && group.plan && (
                <span className="font-normal text-muted-foreground/60">{` · ${group.plan}`}</span>
              )}
            </span>
            {health && group.signedIn && (
              <span
                className="shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] font-medium text-amber-500"
                title={`This login stopped working on at least one machine — sign in again from Devices.`}
              >
                {health}
              </span>
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
          <DeviceChip key={row.key} row={row} now={now} />
        ))}
        {showAdd && addBlocked !== null && (
          <span title={addBlocked}>
            <Pill
              size="sm"
              mode="action"
              disabled
              aria-label="Add a machine to this account"
              className="px-1.5"
              data-testid={`account-add-device-${group.key}`}
            >
              <AddIcon className="size-3" />
            </Pill>
          </span>
        )}
        {showAdd && addBlocked === null && addTargets.length > 0 && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Pill
                size="sm"
                mode="action"
                aria-label="Add a machine to this account"
                title="Sign in to this account on another machine"
                className="px-1.5"
                data-testid={`account-add-device-${group.key}`}
              >
                <AddIcon className="size-3" />
              </Pill>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              {addTargets.map((row) => (
                <DropdownMenuItem key={row.deviceId} onSelect={() => addTo(row)}>
                  <SignInIcon className="size-4" />
                  {`Sign in on ${row.label || row.deviceId}`}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        )}
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

/** EXP-849: one machine chip — a QUIET presence indicator: the online dot,
 * the machine (· profile), a CHECK when the account is that machine's ACTIVE
 * login, and the health initial when the credential there is broken. It is
 * not a control any more: Accounts decides WHICH account to use, Devices
 * repairs a machine (sign in, re-login, use this account here).
 */
function DeviceChip({
  row,
  now,
}: {
  row: AgentProfileUsageRow
  now: Date
}) {
  const health = healthBadgeLabel(row.health)
  return (
    <Pill
      size="sm"
      dot={row.online ? ONLINE_DOT : OFFLINE_DOT}
      title={chipTitle(row, now)}
      className={cn(!row.online && `text-muted-foreground`)}
    >
      {chipLabel(row)}
      {row.signedIn && row.active && (
        <CheckIcon
          className="size-3 text-emerald-400"
          aria-label="Active on this machine"
        />
      )}
      {health && (
        <span className="text-[10px] font-medium text-amber-500">{health}</span>
      )}
    </Pill>
  )
}
