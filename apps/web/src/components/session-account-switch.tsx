// EXP-849 (phase 3): switching a RUN to another agent account, from the web.
//
// The session's usage readout is a CONTROL now: it opens the accounts the
// run's own machine holds for the run's own agent — each with its live usage
// bars — and switches to one. A switch is NOT a new wire verb: it is a
// RESUME of this run naming another profile
// (`steer.startSession({ resumeSessionId, deviceId, account })`, interface B),
// so the device re-enters the recorded run with the other login, the new
// `coding_sessions` row carries `resumedFromId` and every list presents it as
// a continuation. A live run is stopped first (a resume only ever re-enters an
// ENDED run) — which is why the control also refuses mid-turn: ending a run
// the agent is thinking in would throw that turn away.
//
// CLAUDE ONLY (the user's decision): codex keeps one account per session, and
// no credential is ever copied, snapshotted or restored anywhere in this flow
// — the device moves the transcript, never the login.
import { useEffect, useMemo, useRef, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import type { CodingSession, Device } from "@/db/schema"
import { conceptIcon } from "@/lib/icons.generated"
import { codingSessionCollection, deviceCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useOpenSession } from "@/hooks/use-open-session"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  agentProfileUsageRows,
  healthBadgeLabel,
  SYSTEM_PROFILE_ID,
  type AgentProfileUsageRow,
} from "@/lib/agent-usage"
import {
  deviceCanAgentLogin,
  deviceCanResumeRun,
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
import { cn } from "@/lib/utils"

const SwapIcon = conceptIcon(`ui-swap`)
const CheckIcon = conceptIcon(`ui-check`)

/** The one-time cost of a switch, said once, where the switch happens: the
 *  other account re-reads the conversation before it can answer. */
export const SWITCH_COST_NOTE = `A switch continues this run on the other account — the agent re-reads the conversation once, which spends tokens on the new account.`

/** One account the run's machine holds for the run's agent. */
export interface SessionAccountOption {
  profileId: string
  /** The email, else the plan, else the profile's label. */
  label: string
  profileLabel: string
  plan: string | null
  /** The machine's ACTIVE login for that agent. */
  active: boolean
  /** This run is already on it. */
  current: boolean
  row: AgentProfileUsageRow
  /** Why this account cannot be switched to right now, or null. */
  blockedReason: string | null
}

/** EXP-849: the rules, as a pure function of the synced rows, so the reasons
 *  are the same sentences everywhere (and testable without a renderer).
 *  `null` = switchable. The ORDER matters: the most fundamental refusal wins,
 *  so a codex run never reads "the machine is offline". */
export function switchBlockedReason(input: {
  agent: string | null
  ownRun: boolean
  deviceRow: Pick<Device, `caps` | `lastSeenAt`> | null
  online: boolean
  turnEnded: boolean
  option: { current: boolean; signedIn: boolean }
  canAgentLogin: boolean
}): string | null {
  if (!input.ownRun) return `Only the owner can steer this run.`
  if (input.agent !== `claude`) {
    return `Only claude can move a running conversation to another account.`
  }
  if (!input.deviceRow) return `This run's machine is no longer registered.`
  if (!input.online) return `This run's machine is offline.`
  if (!deviceCanResumeRun({ caps: input.deviceRow.caps ?? [] })) {
    return `This machine runs an older Exponential app that cannot continue a run.`
  }
  if (!input.turnEnded) {
    return `The agent is working — switch once it finishes its turn.`
  }
  if (input.option.current) return `This run is already on this account.`
  if (!input.option.signedIn) {
    return input.canAgentLogin
      ? `Not signed in on this machine — sign in to it from Devices first.`
      : `Not signed in on this machine, and this build cannot sign in remotely.`
  }
  return null
}

export interface SessionAccountSwitch {
  /** The accounts the run's machine holds for the run's agent, the active one
   *  first; empty when the machine reported none (or it is not ours). */
  options: SessionAccountOption[]
  /** Whether ANY account could be switched to — the readout only advertises
   *  the switch when one could happen. */
  canSwitch: boolean
  /** The profile a switch is in flight for, or null. */
  switchingTo: string | null
  switchTo: (profileId: string) => void
}

/** The switch, as a small state machine: a LIVE run is stopped first (the
 *  resume only re-enters an ended run), the synced row flipping to `ended`
 *  fires the resume, and the resumed row appearing opens it — exactly the
 *  hand-off `use-remote-start.ts` and the ended-run Resume button already
 *  use (`findStartedRun({ kind: 'resumed' })`). */
export function useSessionAccountSwitch(
  session: Pick<
    CodingSession,
    `id` | `userId` | `agent` | `agentAccount` | `deviceId` | `status`
  >,
  currentUserId: string,
  /** EXP-848's turn slot: the switch is offered BETWEEN turns only — ending a
   *  run the agent is thinking in would throw that turn away. */
  { turnEnded }: { turnEnded: boolean }
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
    phase: `stopping` | `resuming`
    sentAt: number
  } | null>(null)
  const deadlineRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  useEffect(
    () => () => {
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
    },
    []
  )

  const ownRun = session.userId === currentUserId
  const online = deviceRow ? deviceRowIsOnline(deviceRow.lastSeenAt, now) : false
  const canAgentLogin = deviceRow
    ? deviceCanAgentLogin({ caps: deviceRow.caps ?? [] })
    : false
  const currentProfile = session.agentAccount ?? SYSTEM_PROFILE_ID

  const options = useMemo<SessionAccountOption[]>(() => {
    if (!deviceRow || !session.agent) return []
    const rows = agentProfileUsageRows([deviceRow], currentUserId, () => online)
      .filter((row) => row.agent === session.agent)
      .sort((a, b) => Number(b.active) - Number(a.active))
    return rows.map((row) => {
      const current = row.profileId === currentProfile
      return {
        profileId: row.profileId,
        label: row.email ?? row.plan ?? row.profileLabel,
        profileLabel: row.profileLabel,
        plan: row.plan,
        active: row.active,
        current,
        row,
        blockedReason: switchBlockedReason({
          agent: session.agent,
          ownRun,
          deviceRow,
          online,
          turnEnded,
          option: { current, signedIn: row.signedIn },
          canAgentLogin,
        }),
      }
    })
  }, [
    deviceRow,
    session.agent,
    currentUserId,
    online,
    currentProfile,
    ownRun,
    turnEnded,
    canAgentLogin,
  ])

  // Fire the resume the moment the stopped run's row reports `ended`.
  const watching = pending !== null
  const { data: sessionRows } = useLiveQuery(
    (query) => (watching ? query.from({ s: codingSessionCollection }) : undefined),
    [watching]
  )
  const resume = async (profileId: string, sentAt: number) => {
    try {
      await trpc.steer.startSession.mutate(
        {
          resumeSessionId: session.id,
          deviceId: session.deviceId!,
          account: profileId,
        },
        { context: { skipErrorToast: true } }
      )
      setPending({ profileId, phase: `resuming`, sentAt })
      if (deadlineRef.current) clearTimeout(deadlineRef.current)
      deadlineRef.current = setTimeout(() => {
        setPending(null)
        toast.error(`That machine never continued this run`, {
          description: `Open the Exponential desktop app there to see why.`,
        })
      }, STARTED_RUN_DEADLINE_MS)
    } catch (error) {
      setPending(null)
      toast.error(`Couldn't switch the account`, {
        description: trpcErrorMessage(
          error,
          `The machine refused to continue this run on that account.`
        ),
      })
    }
  }

  useEffect(() => {
    if (!pending || pending.phase !== `stopping`) return
    if (session.status !== `ended`) return
    void resume(pending.profileId, pending.sentAt)
    // `resume` is stable enough for this edge (it closes over ids only).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending, session.status])

  // The continuation's own row: open it exactly once (clearing `pending`
  // stops this effect from matching again).
  useEffect(() => {
    if (!pending || pending.phase !== `resuming`) return
    const match = findStartedRun(
      (sessionRows ?? []) as CodingSession[],
      { kind: `resumed`, fromId: session.id },
      session.userId,
      pending.sentAt - STARTED_RUN_SKEW_MS
    )
    if (!match) return
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
    // A live run is stopped first; an already-ended one resumes straight away.
    if (session.status === `ended`) {
      setPending({ profileId, phase: `resuming`, sentAt })
      void resume(profileId, sentAt)
      return
    }
    setPending({ profileId, phase: `stopping`, sentAt })
    trpc.steer.killSession
      .mutate({ sessionId: session.id }, { context: { skipErrorToast: true } })
      .catch((error: unknown) => {
        setPending(null)
        toast.error(`Couldn't switch the account`, {
          description: trpcErrorMessage(
            error,
            `This run could not be stopped, so it was left where it is.`
          ),
        })
      })
  }

  return {
    options,
    canSwitch: options.some((option) => option.blockedReason === null),
    switchingTo: pending?.profileId ?? null,
    switchTo,
  }
}

/** The account rows the usage readout opens: the login, its plan, its live
 *  usage bars, and "Switch to this account" — disabled WITH the reason, never
 *  hidden (a control that vanishes reads as a feature that is not there). */
export function SessionAccountRows({
  state,
  now,
}: {
  state: SessionAccountSwitch
  now: Date
}) {
  if (state.options.length === 0) return null
  return (
    <div className="space-y-1.5">
      <div className="flex flex-col">
        {state.options.map((option) => (
          <SessionAccountRow
            key={option.profileId}
            option={option}
            now={now}
            switching={state.switchingTo === option.profileId}
            busy={state.switchingTo !== null}
            onSwitch={() => state.switchTo(option.profileId)}
          />
        ))}
      </div>
      {/* The one-time cost, said where the switch happens. */}
      <p className="text-[11px] text-muted-foreground/70">{SWITCH_COST_NOTE}</p>
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
  return (
    <ListRow className="flex-col items-stretch gap-1.5 px-3 py-2">
      <div className="flex min-w-0 items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex min-w-0 items-center gap-1.5 text-sm">
            <span className="min-w-0 truncate" title={option.label}>
              {option.label}
            </span>
            {option.plan && option.label !== option.plan && (
              <span className="shrink-0 text-xs text-muted-foreground/60">
                {option.plan}
              </span>
            )}
            {option.current && (
              <CheckIcon
                className="size-3 shrink-0 text-emerald-400"
                aria-label="This run's account"
              />
            )}
            {health && (
              <span className="shrink-0 text-[10px] font-medium text-amber-500">
                {health}
              </span>
            )}
          </div>
        </div>
        {!option.current && (
          <span title={option.blockedReason ?? undefined}>
            <Button
              variant="glass"
              size="sm"
              className="shrink-0"
              disabled={option.blockedReason !== null || busy}
              onClick={onSwitch}
            >
              {switching ? (
                <LoaderCircle className="animate-spin" />
              ) : (
                <SwapIcon />
              )}
              Switch to this account
            </Button>
          </span>
        )}
      </div>
      {option.row.usage && option.row.usage.windows.length > 0 && (
        <div className={cn(option.current ? `` : `opacity-80`)}>
          <AgentUsageCards usage={option.row.usage} now={now} compact dense />
        </div>
      )}
      {option.current && (
        <p className="text-[11px] text-muted-foreground">
          This run is on this account.
        </p>
      )}
    </ListRow>
  )
}
