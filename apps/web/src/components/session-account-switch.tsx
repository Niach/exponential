// EXP-849: the usage wall's "Switch account" control. THE account picker the
// composer uses (`@exp/ui` AccountPicker) behind a button; a pick sends
// `sessions.send({ sessionId, account })` with no text and the host device
// resumes the SAME session on that login between turns. The rules and copy
// live in `lib/session-account-switch.ts`. #EXP-1535: every composer's "+"
// Account › reuses the same switch (`useSessionAccountSwitch`).
import { useMemo, useState } from "react"
import { useLiveQuery } from "@tanstack/react-db"
import {
  AccountPicker,
  Button,
  conceptIcon,
  IconTooltip,
  type AccountPickerOption,
} from "@exp/ui"
import type { Device } from "@/db/schema"
import { deviceCollection } from "@/lib/collections"
import { useNow } from "@/hooks/use-now"
import { useSession } from "@/hooks/use-session"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { healthBadgeLabel } from "@/lib/agent-usage"
import { accountOptionKey } from "@/lib/accounts/account-option"
import type { SessionRow } from "@/lib/session-tree"
import {
  canSteerSession,
  sessionAccountSwitchOptions,
  sessionSwitchAgent,
  SWITCH_COST_NOTE,
  switchBlockedReason,
  WALL_SWITCH_LABEL,
} from "@/lib/session-account-switch"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { trpc } from "@/lib/trpc-client"

const SwapIcon = conceptIcon(`ui-swap`)

type SwitchSession = Pick<
  SessionRow,
  `id` | `cli` | `account` | `deviceId` | `userId` | `busy` | `shared`
>

/** THE session account switch: the session's device, its agent's logins as
 *  THE AccountPicker rows (a refused login disabled), the run-level refusal
 *  and the pick (`sessions.send({ sessionId, account })`, no text). */
export function useSessionAccountSwitch(session: SwitchSession | null) {
  const { data: auth } = useSession()
  const now = useNow(30_000)
  const { data: deviceRows } = useLiveQuery((query) =>
    query.from({ d: deviceCollection })
  )
  // The starter's own row wins: two users can see one machine id through a
  // shared row (`resolveSessionDevice`).
  const device = useMemo(() => {
    if (!session) return null
    const rows = ((deviceRows ?? []) as Device[]).filter(
      (row) => row.deviceId === session.deviceId
    )
    return rows.find((row) => row.userId === session.userId) ?? rows[0] ?? null
  }, [deviceRows, session?.deviceId, session?.userId])
  const [switching, setSwitching] = useState(false)

  const switchOptions = useMemo(
    () => (session ? sessionAccountSwitchOptions(device, session) : []),
    [device, session?.cli, session?.account]
  )
  const deviceOnline = device ? deviceRowIsOnline(device.lastSeenAt, now) : false
  const canSteer = session !== null && canSteerSession(session, auth?.user.id)
  const blocked = session
    ? switchBlockedReason({
        agent: sessionSwitchAgent(session.cli, device),
        deviceOnline,
        busy: session.busy,
      })
    : null
  const options: AccountPickerOption[] = switchOptions.map((option) => ({
    key: accountOptionKey(option),
    agent: option.agent,
    email: option.email,
    hint: healthBadgeLabel(option.health) ?? undefined,
    limits: option.limits,
    disabled: blocked !== null || option.blockedReason !== null,
  }))
  const current = switchOptions.find((option) => option.current)

  const switchTo = async (key: string) => {
    const option = switchOptions.find((entry) => accountOptionKey(entry) === key)
    if (!session || blocked || !option || option.blockedReason) return
    setSwitching(true)
    try {
      await trpc.sessions.send.mutate(
        { sessionId: session.id, account: option.id },
        SKIP_ERROR_TOAST
      )
    } catch (error) {
      failedAction(`switch`, `the account`, error)
    } finally {
      setSwitching(false)
    }
  }

  return {
    canSteer,
    deviceOnline,
    blocked,
    switching,
    options,
    value: current ? accountOptionKey(current) : null,
    switchTo,
  }
}

export function SessionAccountSwitchButton({ session }: { session: SwitchSession }) {
  const { canSteer, blocked, switching, options, value, switchTo } =
    useSessionAccountSwitch(session)
  if (!canSteer) return null

  const button = (
    <Button
      variant="outline"
      size="sm"
      className="h-6 shrink-0"
      disabled={blocked !== null || switching}
      pending={switching}
      title={blocked ? undefined : SWITCH_COST_NOTE}
      data-testid="session-account-switch"
    >
      <SwapIcon />
      {WALL_SWITCH_LABEL}
    </Button>
  )
  // A refused switch keeps its control and says why, on the control.
  if (blocked) return <IconTooltip label={blocked}>{button}</IconTooltip>

  return (
    <AccountPicker
      value={value}
      options={options}
      onChange={(key) => void switchTo(key)}
      mobileTitle={WALL_SWITCH_LABEL}
      align="end"
      trigger={button}
      data-testid="session-account-switch-picker"
    />
  )
}
