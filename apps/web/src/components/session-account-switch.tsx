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
import { conceptIcon, Button, IconTooltip, ListRow, useIsMobile } from "@exp/ui"
import { codingSessionCollection, deviceCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { runHasEnded } from "@/lib/past-runs"
import { useOpenSession } from "@/hooks/use-open-session"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  agentProfileUsageRows,
  healthBadgeLabel,
  usageAge,
  type AgentProfileUsageRow,
  accountName,
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
import { UsageMini } from "@/components/agent-usage-mini"
import { cn } from "@/lib/utils"

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

// EXP-974 retired the continuation byline + its cost note: a resumed run and
// the run it came out of share ONE toggle now, and the `Runs` caret is the
// link between them (`lib/sessions/run-chain.ts`).

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
  /** The identity line (`accountName`): the email, else the plan. */
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
 *  `currentAccount` is the profile this run is on: `coding_sessions.agent_account`,
 *  synced since EXP-909. Absent (an older device never stamped it) means
 *  UNKNOWN, never the ambient login — the machine's own active login is not a
 *  safe stand-in. */
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
 *  synced `agent_account` (EXP-909), else the login whose email the machine's
 *  usage report names for this agent, else the machine's active login for it.
 *  -1 = unknown (every row is then "another account"). */
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

// EXP-1051 retired `globalSwitchBlocker` and the overlay footer it fed: the
// refusal now rides the switch button's OWN tooltip, where the control is,
// and `SWITCH_COST_NOTE` rides it as the hint under the label. A footer
// sentence about a button three rows up was the wrong place for both.

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
  // EXP-909: the run's own account, synced on the row — the device stamps it
  // at start (and on the continuation a switch creates). Absent only when an
  // older device never sent one, and that stays UNKNOWN rather than being
  // guessed as the ambient login.
  const currentAccount = session.agentAccount ?? null

  const options = useMemo<SessionAccountOption[]>(() => {
    if (!deviceRow || !session.agent) return []
    const rows = agentProfileUsageRows([deviceRow], currentUserId, () => online)
      .filter((row) => row.agent === session.agent)
      .sort((a, b) => Number(b.active) - Number(a.active))
    return rows.map((row) => ({
      profileId: row.profileId,
      label: accountName(row),
      plan: row.plan,
      active: row.active,
      current: currentAccount === row.profileId,
      row,
      blockedReason: switchBlockedReason({
        agent: session.agent,
        mine,
        sessionEnded: runHasEnded(session),
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
 *  the caller filters the active one out and owns the section title): the
 *  login, its live usage bars, and "Switch to this account" — disabled WITH
 *  the reason, never hidden (a control that vanishes reads as a feature that
 *  is not there). EXP-1051: the reason is the button's TOOLTIP now, so a row
 *  is two lines whether or not the switch is refused. */
export function SessionAccountRows({
  options,
  switchingTo,
  onSwitch,
  now,
}: {
  options: readonly SessionAccountOption[]
  switchingTo: string | null
  onSwitch: (profileId: string) => void
  now: Date
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
  onSwitch,
}: {
  option: SessionAccountOption
  now: Date
  switching: boolean
  busy: boolean
  onSwitch: () => void
}) {
  const health = healthBadgeLabel(option.row.health)
  // EXP-909: the plan is the HEADER's, said once — a row carries the identity
  // and, when the credential is broken, the badge. (EXP-863 had already
  // dropped the "Active login" caption: the active account is the header.)
  const age = usageAge(option.row.usage, now)
  const isMobile = useIsMobile()
  const blocked = option.blockedReason
  return (
    <ListRow interactive className="flex-col items-stretch gap-1 px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <div className="flex min-w-0 flex-1 items-center gap-1.5 text-sm">
          <span className="min-w-0 truncate" title={option.label}>
            {option.label}
          </span>
          {health && (
            <span className="shrink-0 text-[10px] font-medium text-amber-500">
              {health}
            </span>
          )}
        </div>
        {/* EXP-909: ICON-ONLY. The labelled button took the whole row width on
            a 320px popover and cut the email it was switching away from; the
            swap glyph says the same thing in 28px, with `SWITCH_LABEL` as its
            accessible name. EXP-862: ghost, never a second bordered capsule
            inside the row's own fill.
            EXP-1051: the refusal and the cost note are its TOOLTIP — the
            sentence belongs to the control, not to a caption the row had to
            grow for. `IconTooltip` wraps the button in a hoverable span, so a
            DISABLED switch still explains itself. */}
        <IconTooltip
          label={blocked ?? SWITCH_LABEL}
          hint={blocked ? undefined : SWITCH_COST_NOTE}
        >
          {/* A phone cannot hover, and the overlay is a sheet there: a tap on
              the dead button says the reason as a toast instead. */}
          <span
            onClick={isMobile && blocked ? () => toast(blocked) : undefined}
            data-testid="session-account-switch"
          >
            <Button
              variant="ghost"
              size="icon"
              className="shrink-0"
              disabled={blocked !== null || busy}
              onClick={onSwitch}
              aria-label={SWITCH_LABEL}
            >
              {switching ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <SwapIcon />
              )}
            </Button>
          </span>
        </IconTooltip>
      </div>
      <UsageMini usage={option.row.usage} className={cn(age && `opacity-50`)} />
      {age && (
        <p className="text-[11px] text-muted-foreground/70 opacity-50">{age}</p>
      )}
    </ListRow>
  )
}
