// EXP-909: the logins a DEVICE holds, listed under its row.
//
// This replaces the cross-device "Accounts" section (email-merged groups,
// per-agent tabs, a chip per machine with its own menu, a "+" chip). Merging
// the same login across machines answered a question nobody asked: a credential
// lives ON a machine, is repaired ON that machine, and the only surface that
// can act is the device row. So each device simply lists what it reported:
//
//   [brand] dennis@acme.test · max            Needs re-login   [⋯]
//           5h ▁▁ 0%  Week ▆▆ 73%  Fable ██ 100%   as of 18 hours ago
//           Add account
//
// The ⋯ menu is the UNCHANGED `AccountChipMenu` (Sign in / Set as default /
// Remove account, same gating) — only its trigger changed from a pill to the
// ghost ⋯ every other row uses. Team devices are READ-ONLY: their logins are
// somebody else's to repair.
//
// Hand-mirrored ×4 (desktop `machines.rs` `render_login_rows`, iOS
// `DeviceLogins.swift`, Android `DeviceLoginRows`); the ordering, the label and
// the two empty strings live in `lib/agent-usage.ts`.
import { useMemo, useState } from "react"
import { AgentMark, conceptIcon, Button } from "@exp/ui"
import {
  deviceLoginRows,
  healthBadgeLabel,
  loginLabel,
  sortDeviceLogins,
  usageAge,
  usageState,
  NO_LOGIN_REPORTED,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceCanAgentLogin,
  deviceIsMine,
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { addableAgents } from "@/lib/agent-account-add"
import { AddAccountDialog } from "@/components/add-account-dialog"
import { UsageMini } from "@/components/agent-usage-mini"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  AccountChipMenu,
  accountChipActionable,
} from "@/components/device-agent-account"
import { relativeTime } from "@/components/comment-rows/format"
import { cn } from "@/lib/utils"

const MoreIcon = conceptIcon(`ui-more`)
const AddIcon = conceptIcon(`ui-add`)

/** The logins under ONE device row. `readOnly` is the team-devices arm: the
 *  rows render, nothing acts. */
export function DeviceLogins({
  device,
  now,
  readOnly = false,
}: {
  device: SteerDevice
  now: Date
  readOnly?: boolean
}) {
  const rows = useMemo(
    () =>
      sortDeviceLogins(
        deviceLoginRows(
          {
            deviceId: device.deviceId,
            deviceLabel: device.deviceLabel,
            agentAccounts: device.agentAccounts,
            agentUsage: device.agentUsage,
            agentUsageAt: device.agentUsageAt,
          },
          { mine: deviceIsMine(device), online: deviceIsOnline(device) }
        )
      ),
    [device]
  )
  // EXP-845 REVERSED (EXP-909): the "Add account" row renders only where a
  // sign-in can actually be queued — my machine, online, taking the command,
  // with an agent installed to sign in to. A disabled control naming a reason
  // nobody can act on from here was chrome; the machine's own row above
  // already says it is offline.
  const canAdd =
    !readOnly &&
    deviceIsMine(device) &&
    deviceIsOnline(device) &&
    deviceCanAgentLogin(device) &&
    addableAgents(device).length > 0
  if (device.agentAccounts === undefined) {
    // The machine has not reported its accounts yet (an older build, or a row
    // that has not beaten since it registered) — not "no login".
    return <p className="mt-1 text-[11px] text-muted-foreground">Checking…</p>
  }
  if (rows.length === 0 && !canAdd) {
    return (
      <p className="mt-1 text-[11px] text-muted-foreground">
        {NO_LOGIN_REPORTED}
      </p>
    )
  }
  return (
    <div className="mt-1 flex flex-col gap-1">
      {rows.length === 0 && (
        <p className="text-[11px] text-muted-foreground">
          {NO_LOGIN_REPORTED}
        </p>
      )}
      {rows.map((row) => (
        <DeviceLoginRowView
          key={row.key}
          device={device}
          row={row}
          now={now}
          readOnly={readOnly}
        />
      ))}
      {canAdd && <AddAccountRow device={device} />}
    </div>
  )
}

/** One login: identity (+ plan) and health on line 1, its numbers on line 2. */
export function DeviceLoginRowView({
  device,
  row,
  now,
  readOnly,
}: {
  device: SteerDevice
  row: AgentProfileUsageRow
  now: Date
  readOnly: boolean
}) {
  const label = loginLabel(row)
  const health = healthBadgeLabel(row.health)
  const state = usageState(row)
  const age = usageAge(row.usage, now)
  // The "as of …" fallback for a login with no numbers at all — the device
  // stamps `checkedAt` on every probe.
  const asOf = row.usage?.fetchedAt ?? row.checkedAt
  const actionable = !readOnly && accountChipActionable(device, row)
  const menu = (
    <Button
      variant="ghost"
      size="icon-sm"
      className="shrink-0"
      aria-label={`Account menu for ${label} on ${device.deviceLabel || device.deviceId}`}
    >
      <MoreIcon />
    </Button>
  )
  return (
    <div
      className="flex min-w-0 flex-col gap-0.5"
      data-testid={`device-login-row-${row.key}`}
    >
      <div className="flex min-w-0 items-center gap-1.5">
        <AgentMark agent={row.agent} className="size-3.5 text-foreground/70" />
        <span className="min-w-0 truncate text-xs" title={label}>
          {label}
          {/* The plan only alongside an email — it IS the label otherwise. */}
          {row.email && row.plan && (
            <span className="text-muted-foreground/60">{` · ${row.plan}`}</span>
          )}
        </span>
        {health && (
          <span className="shrink-0 text-[10px] font-medium text-amber-500">
            {health}
          </span>
        )}
        <span className="ml-auto shrink-0">
          {actionable ? (
            <AccountChipMenu
              device={device}
              row={row}
              accountLabel={label}
              onSignIn={() =>
                requestAgentLogin({
                  device,
                  agent: row.agent,
                  profileId: row.profileId,
                })
              }
              trigger={menu}
            />
          ) : (
            // The trailing column keeps its width so the rows line up.
            <span aria-hidden className="block size-8" />
          )}
        </span>
      </div>
      <div className="flex min-w-0 items-center gap-2 pl-5">
        {state === `ready` ? (
          <>
            {/* EXP-944: the device list is where a limit is actually planned
                around, so its bars say WHEN they reset. */}
            <UsageMini
              usage={row.usage}
              now={now}
              className={cn(`min-w-0 flex-1`, age && `opacity-50`)}
            />
            {age && (
              <span className="shrink-0 text-[10px] text-muted-foreground opacity-50">
                {age}
              </span>
            )}
          </>
        ) : state === `checking` ? (
          // A login with no windows read YET: "No usage reported" made a
          // device that is simply still working read as broken.
          <span className="text-[10px] text-muted-foreground">Checking…</span>
        ) : (
          <span className="text-[10px] text-muted-foreground">
            {asOf
              ? `No usage reported · as of ${relativeTime(asOf)}`
              : `No usage reported`}
          </span>
        )}
      </div>
    </div>
  )
}

/** The device-bound "Add account" entry — the sign-in runs HERE. */
export function AddAccountRow({ device }: { device: SteerDevice }) {
  const [open, setOpen] = useState(false)
  return (
    <>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 w-fit px-1 text-[11px] text-muted-foreground"
        onClick={() => setOpen(true)}
        data-testid={`add-account-${device.deviceId}`}
      >
        <AddIcon className="size-3" />
        Add account
      </Button>
      <AddAccountDialog open={open} onOpenChange={setOpen} device={device} />
    </>
  )
}
