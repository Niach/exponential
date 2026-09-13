// EXP-849 (phase 3): switching a RUN to another agent account, from the web.
//
// The session's usage readout is a CONTROL: it opens the accounts the run's own
// machine holds for the run's own agent — each with its live usage bars — and
// continues the run on one of them. A switch is NOT a new wire verb: it is a
// RESUME of this run naming another profile
// (`steer.startSession({ resumeSessionId, deviceId, account })`, interface B)
// sent while the run is still LIVE. The DEVICE does the rest: it ends the live
// run (`ended_by: client`), moves the agent's transcript into the target
// profile's config dir — never the credential — and relaunches there, stamping
// `resumed_from_id` so every list reads the pair as one conversation. The
// client never kills first: a kill-then-resume would leave a dead run behind
// whenever the resume was refused.
//
// CLAUDE ONLY (EXP-849 §E): codex's conversation lives inside its login's own
// rollout store, so there is nothing to move. And BETWEEN TURNS only — the
// device has to stop the agent, so a switch mid-turn would throw away exactly
// the output the reader is waiting for.
//
// Every string and every refusal here is hand-mirrored ×4 (Android
// `domain/SessionAccountSwitch.kt`, iOS `Domain/SessionAccountSwitch.swift`,
// desktop `ui/src/account_switch.rs`); `session-account-switch.test.ts` locks
// them against the native sources.
import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import type { CodingSession, Device, DeviceAgentHealth } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { codingSessionCollection, deviceCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useOpenSession } from "@/hooks/use-open-session"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  agentProfileUsageRows,
  healthBadgeLabel,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceCanResumeRun,
  deviceCanSwitchAccount,
  deviceRowIsOnline,
} from "@/lib/steer-devices"
import {
  findStartedRun,
  STARTED_RUN_DEADLINE_MS,
  STARTED_RUN_SKEW_MS,
} from "@/lib/started-run-match"
import { AgentUsageCards } from "@/components/agent-usage-bar"
import { Button } from "@/components/ui/button"
import { ListRow } from "@/components/ui/glass-rows"

const SwapIcon = conceptIcon(`ui-swap`)

/** The only agent that can change account between messages (EXP-849 §E). */
export const SWITCHABLE_AGENT = `claude`

/** The sheet's section title, byte-identical ×4. */
export const ACCOUNTS_SECTION_TITLE = `Accounts`

/** The primary control on an account row. */
export const SWITCH_LABEL = `Switch to this account`

/** The rate-limit wall's PRIMARY button (EXP-849 interface D). */
export const WALL_SWITCH_LABEL = `Switch account`

/** What a switch costs, said once where the switch is offered: the agent
 *  re-enters the recorded run under the other login, which re-reads the
 *  transcript — one extra context read, not a per-message surcharge. */
export const SWITCH_COST_NOTE = `The run continues under the other account. Re-reading the transcript once costs tokens.`

/** The continuation byline a resumed run's screen carries. */
export const CONTINUATION_NOTE = `Continues an earlier run`

/** What that continuation cost, said ONCE on the new run. */
export const CONTINUATION_COST_NOTE = `The agent re-read the transcript once to pick it up — a one-time cost.`

// ── Refusals ─────────────────────────────────────────────────────────────────
// One sentence each, and the reason is always about the thing the person can
// change. Shown ON the disabled control rather than hiding it, so the switch
// never silently disappears mid-run.

export const REASON_AGENT = `Only claude can switch accounts during a run.`
export const REASON_NOT_MINE = `Only the person who started this run can switch its account.`
export const REASON_ENDED = `This run has ended — resume it instead.`
export const REASON_OFFLINE = `The machine is offline.`
export const REASON_NO_CAP = `Update the app on that machine to switch accounts.`
export const REASON_BUSY = `The agent is working — switching waits for the turn to finish.`
export const REASON_SIGNED_OUT = `Sign in to this account on that machine first.`
export const REASON_NEEDS_RELOGIN = `This account needs a re-login on that machine.`
export const REASON_ALREADY = `This run is already on this account.`

/** One account the run's machine holds for the run's agent. */
export interface SessionAccountOption {
  profileId: string
  /** The identity line: the email, else the plan, else the profile's label. */
  label: string
  plan: string | null
  /** The machine's ACTIVE login for that agent. */
  active: boolean
  /** This run is KNOWN to be on it (usually unknowable — see below). */
  current: boolean
  row: AgentProfileUsageRow
  /** Why this account cannot be switched to right now, or null. */
  blockedReason: string | null
}

/** EXP-849: the rules, as a pure function of the synced rows, so the reasons
 *  are the same sentences everywhere (and testable without a renderer).
 *  `null` = switchable. The ORDER matters and is the ×4 order: the most
 *  fundamental refusal wins, so a codex run never reads "the machine is
 *  offline".
 *
 *  `currentAccount` is the profile this run is KNOWN to be on, when the client
 *  knows it — `coding_sessions.agent_account` is SERVER-ONLY (never in the
 *  shape allowlist), so it is normally null, and the machine's own active
 *  login is not a safe stand-in. */
export function switchBlockedReason(input: {
  agent: string | null
  mine: boolean
  sessionEnded: boolean
  deviceOnline: boolean
  /** The machine advertises BOTH halves of the switch: `resume-run` (the
   *  rail it rides) and `account-switch` (the profile machinery). */
  canSwitch: boolean
  /** EXP-848's turn slot: `ended` is the default, so a viewer that has not
   *  seen a `turn` event yet reads as idle. */
  turnEnded: boolean
  option: {
    profileId: string
    signedIn: boolean
    health: DeviceAgentHealth
  }
  currentAccount?: string | null
}): string | null {
  if (input.agent !== SWITCHABLE_AGENT) return REASON_AGENT
  if (!input.mine) return REASON_NOT_MINE
  if (input.sessionEnded) return REASON_ENDED
  if (!input.deviceOnline) return REASON_OFFLINE
  if (!input.canSwitch) return REASON_NO_CAP
  if (!input.turnEnded) return REASON_BUSY
  if (input.option.health === `needs_relogin`) return REASON_NEEDS_RELOGIN
  if (!input.option.signedIn || input.option.health === `signed_out`) {
    return REASON_SIGNED_OUT
  }
  if (input.currentAccount && input.currentAccount === input.option.profileId) {
    return REASON_ALREADY
  }
  return null
}

export interface SessionAccountSwitch {
  /** The accounts the run's machine holds for the run's agent, the active one
   *  first; empty when the machine reported none (or it is not ours). The
   *  readout offers the rows whenever there are any — a refused row keeps its
   *  control and says why, which is the whole point of the reasons. */
  options: SessionAccountOption[]
  /** The profile a switch is in flight for, or null. */
  switchingTo: string | null
  switchTo: (profileId: string) => void
}

/** EXP-863: which option is the account the run is ON — the desktop's
 *  `SwitchTarget.current` rule, in the order the client can know it: the
 *  synced `agent_account` (server-only, so rarely), else the login whose
 *  email the machine's usage report names for this agent, else the machine's
 *  active login for it. -1 = unknown (every row is then "another account"). */
export function activeAccountIndex(
  options: readonly {
    current: boolean
    active: boolean
    row: { email: string | null }
  }[],
  reportedEmail: string | null | undefined
): number {
  const known = options.findIndex((option) => option.current)
  if (known >= 0) return known
  const email = reportedEmail?.trim() ?? ``
  if (email) {
    const byEmail = options.findIndex((option) => option.row.email === email)
    if (byEmail >= 0) return byEmail
  }
  return options.findIndex((option) => option.active)
}

/** The refusals that are about the RUN, not about one login — when every
 *  other account is refused for the same one of these, the overlay says it
 *  once in its footer instead of under each row. */
const GLOBAL_SWITCH_REASONS: readonly string[] = [
  REASON_AGENT,
  REASON_NOT_MINE,
  REASON_ENDED,
  REASON_OFFLINE,
  REASON_NO_CAP,
  REASON_BUSY,
]

/** EXP-863: the ONE footer sentence when switching is refused for every
 *  listed account by the same run-level reason, else null (the footer then
 *  carries `SWITCH_COST_NOTE`). Row-specific refusals (signed out, needs a
 *  re-login) never become the footer: they stay under their row. */
export function globalSwitchBlocker(
  options: readonly Pick<SessionAccountOption, `blockedReason`>[]
): string | null {
  if (options.length === 0) return null
  const first = options[0].blockedReason
  if (!first || !GLOBAL_SWITCH_REASONS.includes(first)) return null
  return options.every((option) => option.blockedReason === first) ? first : null
}

/** The switch, as a small state machine: ONE mutation (the live run's own
 *  resume, naming the account), then the wait for the continuation's row —
 *  exactly the hand-off `use-remote-start.ts` and the ended-run Resume button
 *  already use (`findStartedRun({ kind: 'resumed' })`), which opens the new
 *  run's page when it syncs. */
export function useSessionAccountSwitch(
  session: Pick<
    CodingSession,
    `id` | `userId` | `agent` | `agentAccount` | `deviceId` | `status`
  >,
  currentUserId: string,
  {
    turnEnded,
    onBeforeSwitch,
  }: {
    turnEnded: boolean
    /** EXP-866: runs right before the switch mutation goes out — the view
     *  drops its live rate-limit slot there, so the wall this switch is the
     *  way out of does not linger over the run that is about to end. */
    onBeforeSwitch?: () => void
  }
): SessionAccountSwitch {
  const now = useNow(30_000)
  const openSession = useOpenSession()
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  const deviceRow = useMemo(() => {
    const rows = ((deviceRows ?? []) as Device[]).filter(
      (row) => row.deviceId === session.deviceId
    )
    // The session owner's own row wins — two users can see one machine id
    // through a shared server row (`resolveSessionDevice`).
    return rows.find((row) => row.userId === session.userId) ?? rows[0] ?? null
  }, [deviceRows, session.deviceId, session.userId])

  const [pending, setPending] = useState<{
    profileId: string
    sentAt: number
  } | null>(null)
  const deadlineRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(
    () => () => {
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
    },
    []
  )

  const mine = session.userId === currentUserId
  const online = deviceRow ? deviceRowIsOnline(deviceRow.lastSeenAt, now) : false
  // EXP-849: the live switch rides the resume rail AND needs the profile
  // machinery — a machine below the `account-switch` build would resume on
  // the RECORDED account and silently drop the field, so the readout says
  // "update it" instead of offering a switch that does nothing.
  const canSwitch = deviceRow
    ? deviceCanResumeRun({ caps: deviceRow.caps ?? [] }) &&
      deviceCanSwitchAccount({ caps: deviceRow.caps ?? [] })
    : false
  // Server-only column: normally absent on a synced row, so "which account is
  // this run on" stays UNKNOWN rather than being guessed as the ambient one.
  const currentAccount = session.agentAccount ?? null

  const options = useMemo<SessionAccountOption[]>(() => {
    if (!deviceRow || !session.agent) return []
    const rows = agentProfileUsageRows([deviceRow], currentUserId, () => online)
      .filter((row) => row.agent === session.agent)
      .sort((a, b) => Number(b.active) - Number(a.active))
    return rows.map((row) => ({
      profileId: row.profileId,
      label: row.email ?? row.plan ?? row.profileLabel,
      plan: row.plan,
      active: row.active,
      current: currentAccount === row.profileId,
      row,
      blockedReason: switchBlockedReason({
        agent: session.agent,
        mine,
        sessionEnded: session.status === `ended`,
        deviceOnline: online,
        canSwitch,
        turnEnded,
        option: {
          profileId: row.profileId,
          signedIn: row.signedIn,
          health: row.health,
        },
        currentAccount,
      }),
    }))
  }, [
    deviceRow,
    session.agent,
    session.status,
    currentUserId,
    online,
    currentAccount,
    mine,
    turnEnded,
    canSwitch,
  ])

  // The continuation's own row: watched only while a switch is in flight.
  const watching = pending !== null
  const { data: sessionRows } = useLiveQuery(
    (query) => (watching ? query.from({ s: codingSessionCollection }) : undefined),
    [watching]
  )
  useEffect(() => {
    if (!pending) return
    const match = findStartedRun(
      (sessionRows ?? []) as CodingSession[],
      { kind: `resumed`, fromId: session.id },
      session.userId,
      pending.sentAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
    // Open it exactly once — clearing `pending` stops this effect matching
    // again.
    if (deadlineRef.current) clearTimeout(deadlineRef.current)
    setPending(null)
    openSession(match)
  }, [sessionRows, pending, session.id, session.userId, openSession])

  const switchTo = (profileId: string) => {
    if (pending) return
    const option = options.find((entry) => entry.profileId === profileId)
    if (!option || option.blockedReason) return
    if (!session.deviceId) return
    const sentAt = Date.now()
    setPending({ profileId, sentAt })
    onBeforeSwitch?.()
    trpc.steer.startSession
      .mutate(
        {
          resumeSessionId: session.id,
          deviceId: session.deviceId,
          // The picked profile VERBATIM, `system` included: the server reads
          // the PRESENCE of `account` as "this resume is a switch", and it is
          // the only thing that lets a resume ride a LIVE run.
          account: profileId,
        },
        { context: { skipErrorToast: true } }
      )
      .then(() => {
        if (deadlineRef.current) clearTimeout(deadlineRef.current)
        deadlineRef.current = setTimeout(() => {
          setPending(null)
          toast.error(`That machine never continued this run`, {
            description: `Open the Exponential desktop app there to see why.`,
          })
        }, STARTED_RUN_DEADLINE_MS)
      })
      .catch((error: unknown) => {
        setPending(null)
        toast.error(`Couldn't switch the account`, {
          description: trpcErrorMessage(
            error,
            `The machine refused to continue this run on that account.`
          ),
        })
      })
  }

  return { options, switchingTo: pending?.profileId ?? null, switchTo }
}

/** The account rows the usage overlay lists (EXP-863: the OTHER accounts —
 *  the caller filters the active one out and owns the section title and the
 *  footer note): the login, its plan, its live usage bars, and "Switch to this
 *  account" — disabled WITH the reason, never hidden (a control that vanishes
 *  reads as a feature that is not there). `omitReason` is the sentence the
 *  footer already says, so no row repeats it. */
export function SessionAccountRows({
  options,
  switchingTo,
  onSwitch,
  now,
  omitReason = null,
}: {
  options: readonly SessionAccountOption[]
  switchingTo: string | null
  onSwitch: (profileId: string) => void
  now: Date
  omitReason?: string | null
}) {
  if (options.length === 0) return null
  return (
    <div className="flex flex-col">
      {options.map((option) => (
        <SessionAccountRow
          key={option.profileId}
          option={option}
          now={now}
          switching={switchingTo === option.profileId}
          busy={switchingTo !== null}
          hideReason={option.blockedReason === omitReason}
          onSwitch={() => onSwitch(option.profileId)}
        />
      ))}
    </div>
  )
}

function SessionAccountRow({
  option,
  now,
  switching,
  busy,
  hideReason,
  onSwitch,
}: {
  option: SessionAccountOption
  now: Date
  switching: boolean
  busy: boolean
  hideReason: boolean
  onSwitch: () => void
}) {
  const health = healthBadgeLabel(option.row.health)
  // The plan, when the identity line is the email. EXP-863: no "Active login"
  // caption any more — the active account is the overlay's header, never a
  // listed row.
  const subtitle =
    option.plan && option.plan !== option.label ? option.plan : null
  return (
    <ListRow interactive className="flex-col items-stretch gap-1.5 px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5 text-sm">
            <span className="min-w-0 truncate" title={option.label}>
              {option.label}
            </span>
            {health && (
              <span className="shrink-0 text-[10px] font-medium text-amber-500">
                {health}
              </span>
            )}
          </div>
          {subtitle && (
            <div className="truncate text-[11px] text-muted-foreground/70">
              {subtitle}
            </div>
          )}
        </div>
        {/* EXP-862: a ghost control on a flat row — the row's own fill is the
            surface, a second bordered capsule inside it was chrome on chrome. */}
        <Button
          variant="ghost"
          size="sm"
          className="shrink-0"
          disabled={option.blockedReason !== null || busy}
          onClick={onSwitch}
        >
          {switching ? <LoaderCircle className="animate-spin" /> : <SwapIcon />}
          {SWITCH_LABEL}
        </Button>
      </div>
      {option.row.usage && option.row.usage.windows.length > 0 && (
        <div className="opacity-80">
          <AgentUsageCards usage={option.row.usage} now={now} compact dense />
        </div>
      )}
      {/* The refusal sits UNDER the disabled control: it is nearly always
          something the person can change (wait for the turn, sign in on the
          machine), and a vanished control reads as a bug. A run-level reason
          the footer already states is not repeated here (EXP-863). */}
      {option.blockedReason && !hideReason && (
        <p className="text-[11px] text-muted-foreground/70">
          {option.blockedReason}
        </p>
      )}
    </ListRow>
  )
}
