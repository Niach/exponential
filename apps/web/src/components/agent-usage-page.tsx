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
// EXP-849: the page is the DECISION surface (which login to run on), the
// Devices list the setup/repair one — so a card badges its login's HEALTH
// ("Needs re-login" is not "Signed out"), and the cards sit behind one tab per
// agent rather than stacking codex's rows on top of claude's.
//
// EXP-862: the page refreshes ITSELF and says so nowhere. While it is open,
// every account whose freshest report is past the device's own rate-limit
// floor gets ONE `agent_usage_refresh` queued on a machine that runs it (cap
// `agent-usage-refresh`), so the numbers on screen are never older than ~5
// minutes on a machine that answers — no refresh button, no "Refreshes every
// 5 minutes" caption to read past.
//
// The device chips carry the SAME three-state menu the device rows do
// (Sign in / Set as default / Remove account, `AccountChipMenu`): a person
// looking at an account here should not have to go find its device row.
import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { contract } from "@exp/domain-contract"
import type { Device } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { deviceCollection } from "@/lib/collections"
import {
  accountUsageGroups,
  agentProfileUsageRows,
  healthBadgeLabel,
  refreshAllowedAt,
  sortAccountGroupsAttentionFirst,
  usageIsFresh,
  usageState,
  SYSTEM_PROFILE_ID,
  type AgentAccountUsageGroup,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceCanRefreshUsage,
  deviceRowIsOnline,
  steerDeviceFromRow,
} from "@/lib/steer-devices"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import { AddAccountDialog } from "@/components/add-account-dialog"
import {
  AccountChipMenu,
  accountChipActionable,
} from "@/components/device-agent-account"
import {
  addAccountBlockReason,
  addAccountDevices,
  addAccountLoginTarget,
  clampProfileLabel,
} from "@/lib/agent-account-add"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { useNow } from "@/hooks/use-now"
import { AgentUsageCards } from "@/components/agent-usage-bar"
import { AgentPickerTabs, agentLabel } from "@/components/agent-picker"
import { relativeTime } from "@/components/comment-rows/format"
import { GlassSectionHeader, ListRow } from "@/components/ui/glass-rows"
import { Pill } from "@/components/ui/pill"
import { cn } from "@/lib/utils"

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
  teamId,
  currentUserId,
}: {
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

  // EXP-862: the ONLY refresh path is the page's own — it fails QUIETLY. A
  // command still queued from the last round answers CONFLICT, and the next
  // tick simply looks again; the global error toast is skipped for the same
  // reason.
  const refresh = async (group: AgentAccountUsageGroup) => {
    const target = group.refreshTarget
    if (!target) return
    setRefreshing((current) => ({
      ...current,
      [group.key]: { fetchedAt: group.usage?.fetchedAt ?? null, at: Date.now() },
    }))
    try {
      await trpc.devices.createCommand.mutate(
        {
          deviceId: target.deviceId,
          kind: `agent_usage_refresh`,
          agent: target.agent as (typeof contract.codingAgent.values)[number],
          profileId: target.profileId,
        },
        { context: { skipErrorToast: true } }
      )
    } catch {
      setRefreshing((current) => {
        const next = { ...current }
        delete next[group.key]
        return next
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
      void refresh(group)
    }
    // `refreshing` is read, not a trigger: a mark appearing or clearing must
    // not start another round on its own.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [groups, now])

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
    <div className="mb-6">
      <GlassSectionHeader
        label="Accounts"
        trailing={
          /* EXP-862: the "+ Add device" twin — same pill, same glyph, same
             place in the band. */
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
          No device has reported an agent account yet.
        </div>
      ) : (
        <div className="mb-1">
          {sections.length > 1 && (
            /* EXP-862: the ONE agent picker's segmented strip — brand mark +
               label, the app's single segmented control. */
            <AgentPickerTabs
              value={activeAgent ?? ``}
              onChange={setAgentTab}
              agents={sections.map((section) => section.agent)}
              className="pt-1 pb-1"
            />
          )}
          <div className="flex flex-col">
            {(activeSection?.groups ?? []).map((group) => (
              <AccountCard
                key={group.key}
                group={group}
                now={now}
                ownDevices={ownDevices}
                currentUserId={currentUserId}
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
}: {
  group: AgentAccountUsageGroup
  now: Date
  /** The caller's own synced devices rows by device id — the candidates the
   *  "+" can sign this account in on, and the rows whose caps decide what a
   *  device chip's menu may offer. */
  ownDevices: Map<string, Device>
  currentUserId: string
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
    // closed (the same tick-later handoff the Devices chips use).
    setTimeout(
      () => requestAgentLogin({ device, agent: group.agent, ...target }),
      0
    )
  }
  const asOf = group.usage?.fetchedAt ?? group.checkedAt
  // EXP-862: the caption is the IDENTITY, never a status — "Not signed in"
  // as a title said the same thing the badge beside it says, twice.
  const caption =
    group.email ??
    group.plan ??
    group.rows[0]?.profileLabel ??
    agentLabel(group.agent)
  // EXP-849: HEALTH, not just "signed in" — an expired credential reads
  // `Needs re-login` (the CLI still claims signed in, the probe came back
  // Unauthorized), a missing one `Signed out`. EXP-862: a signed-OUT account
  // wears it too — it is the only thing that says so now.
  const health = healthBadgeLabel(group.health)
  // EXP-862: what the card says about its numbers — cards, "Checking…" for a
  // signed-in login nothing has read yet (every login is read now, so it is a
  // beat or two), else the "no usage" line. ×4 with iOS/Android/desktop.
  const state = usageState({
    signedIn: group.signedIn,
    unmonitored: group.rows.every((row) => row.unmonitored),
    usage: group.usage,
  })
  return (
    <ListRow interactive className="flex-col items-stretch gap-1.5 px-3 py-2.5">
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
            {health && (
              <span className="shrink-0 rounded-sm border border-amber-500/40 px-1 text-[10px] font-medium text-amber-500">
                {health}
              </span>
            )}
          </div>
        </div>
      </div>
      <div className="flex flex-wrap gap-1">
        {group.rows.map((row) => (
          <DeviceChip
            key={row.key}
            row={row}
            now={now}
            deviceRow={ownDevices.get(row.deviceId) ?? null}
            currentUserId={currentUserId}
          />
        ))}
        {showAdd && addBlocked !== null && (
          <span title={addBlocked}>
            <Pill
              size="sm"
              mode="action"
              disabled
              aria-label="Add a device to this account"
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
                aria-label="Add a device to this account"
                title="Sign in to this account on another device"
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
      {state === `ready` && group.usage ? (
        <div className={cn(`pt-0.5`, !fresh && `opacity-50`)}>
          <AgentUsageCards usage={group.usage} now={now} compact dense />
          {!fresh && !group.usage.stale && asOf && (
            <p className="mt-1 text-[11px] text-muted-foreground">
              as of {relativeTime(asOf)}
            </p>
          )}
        </div>
      ) : state === `checking` ? (
        // A login with no windows read YET: "No usage reported" made a device
        // that is simply still working read as broken. The state alone decides
        // the caption (desktop `usage_caption`) — the device stamps
        // `checkedAt` on every probe, so an as-of guard here would hide this
        // line on almost every row that needs it.
        <p className="text-[11px] text-muted-foreground">Checking…</p>
      ) : (
        <p className="text-[11px] text-muted-foreground">
          {asOf ? `No usage reported · as of ${relativeTime(asOf)}` : `No usage reported`}
        </p>
      )}
    </ListRow>
  )
}

/** EXP-849/EXP-862: one device chip — the online dot, the device (· profile),
 * a CHECK when the account is that device's ACTIVE login, and the health badge
 * when the credential there is broken. On one of the caller's OWN devices it
 * carries the shared account menu (Sign in / Set as default / Remove account);
 * a teammate's device, an offline one, or a build that takes none of the
 * commands renders the statement it always was.
 */
function DeviceChip({
  row,
  now,
  deviceRow,
  currentUserId,
}: {
  row: AgentProfileUsageRow
  now: Date
  /** The caller's OWN synced row for this chip's device, or null. */
  deviceRow: Device | null
  currentUserId: string
}) {
  const health = healthBadgeLabel(row.health)
  const device = useMemo(
    () =>
      deviceRow ? steerDeviceFromRow(deviceRow, { now, currentUserId }) : null,
    [deviceRow, now, currentUserId]
  )
  const body = (
    <>
      {chipLabel(row)}
      {row.signedIn && row.active && (
        <CheckIcon
          className="size-3 text-emerald-400"
          aria-label="Active on this device"
        />
      )}
      {health && (
        <span className="text-[10px] font-medium text-amber-500">{health}</span>
      )}
    </>
  )
  if (!device || !accountChipActionable(device, row)) {
    return (
      <Pill
        size="sm"
        dot={row.online ? ONLINE_DOT : OFFLINE_DOT}
        title={chipTitle(row, now)}
        className={cn(!row.online && `text-muted-foreground`)}
      >
        {body}
      </Pill>
    )
  }
  return (
    <AccountChipMenu
      device={device}
      row={row}
      onSignIn={() =>
        requestAgentLogin({
          device,
          agent: row.agent,
          profileId: row.profileId,
        })
      }
      trigger={
        <Pill
          size="sm"
          mode="action"
          dot={row.online ? ONLINE_DOT : OFFLINE_DOT}
          title={chipTitle(row, now)}
        >
          {body}
        </Pill>
      }
    />
  )
}
