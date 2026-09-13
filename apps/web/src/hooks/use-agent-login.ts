// EXP-792 (EXP-747 A1): the remote agent sign-in flow, extracted from the
// device-settings dialog so the launch-failure toasts, the machine rows and
// the settings tab all drive the SAME loop. `agent_login` is a durable
// device command the machine completes EARLY, the moment the agent CLI puts
// a sign-in URL on the grid (`AgentLoginProgress`); EXP-765's
// `agent_login_code` hands claude's browser code back to the login still
// waiting for it. Both are tracked per agent under the keys
// `device-agent-account.tsx` defines, polled via `devices.getCommand` while
// the consumer is mounted and `active`.
import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { trpcErrorMessage } from "@/lib/trpc-error"
import {
  agentLoginCodeKey,
  agentLoginKey,
  agentOfLoginCodeKey,
} from "@/components/device-agent-account"

interface TrackedCommand {
  id: string
  key: string
}

export interface AgentLoginState {
  pending: boolean
  /** The finished login's `result` (the JSON `LoginProgress`). */
  result: string | null
  error: string
  codePending: boolean
  codeResult: string | null
  codeError: string
}

/** EXP-827: WHICH account profile a login lands in — an existing one by id,
 * or a new one the machine creates first (labelled). Absent = the ambient
 * login (`system`). */
export interface AgentLoginProfileTarget {
  profileId?: string
  newProfileLabel?: string
}

export interface AgentLogin {
  stateFor: (agent: string) => AgentLoginState
  /** Queue a login; `switchAccount` signs the current account out first. The
   * caller owns any confirmation (codex's logout is server-side). */
  queueLogin: (
    agent: string,
    switchAccount: boolean,
    target?: AgentLoginProfileTarget
  ) => void
  /** EXP-765: hand claude's authorization code back to the waiting login. */
  queueLoginCode: (agent: string, code: string) => void
  /** Whether any login/code command is still in flight. */
  busy: boolean
}

export function useAgentLogin({
  deviceId,
  online,
  active,
}: {
  /** The steer device id of one of the caller's OWN machines. */
  deviceId: string | undefined
  /** Drives the poll cadence: an offline machine keeps its commands queued
   * server-side, so polling slowly is enough. */
  online: boolean
  /** Poll only while the consumer shows the outcome. */
  active: boolean
}): AgentLogin {
  const [tracked, setTracked] = useState<TrackedCommand[]>([])
  const [results, setResults] = useState<Record<string, string>>({})
  const [errors, setErrors] = useState<Record<string, string>>({})

  const queueCommand = async (
    key: string,
    input:
      | ({ kind: `agent_login`; agent: string; switch: boolean } & AgentLoginProfileTarget)
      | { kind: `agent_login_code`; agent: string; code: string }
  ) => {
    if (!deviceId) return
    setErrors((current) => ({ ...current, [key]: `` }))
    setResults((current) => {
      if (!(key in current)) return current
      const next = { ...current }
      delete next[key]
      return next
    })
    try {
      const { id } = await trpc.devices.createCommand.mutate(
        {
          deviceId,
          ...input,
        },
        // The dialog prints the failure itself — no second toast from the
        // global link.
        { context: { skipErrorToast: true } }
      )
      setTracked((current) => [...current, { id, key }])
    } catch (error) {
      setErrors((current) => ({
        ...current,
        [key]: trpcErrorMessage(error, `Couldn't queue that on the device.`),
      }))
    }
  }

  // Poll queued commands until terminal. A login's whole answer (the sign-in
  // URL) lives in `result`, and only a `done` row ever carries one; failures
  // land in `errors` like every other command.
  useEffect(() => {
    if (!active || tracked.length === 0) return
    let cancelled = false
    const tick = async () => {
      for (const command of tracked) {
        try {
          const result = await trpc.devices.getCommand.query({
            commandId: command.id,
          })
          if (cancelled || result.status === `pending`) continue
          if (result.status === `done` && result.result) {
            const text = result.result
            setResults((current) => {
              const next = { ...current, [command.key]: text }
              // EXP-765: the code went in — the link has served its purpose
              // and the row flips signed-in on the machine's re-probe.
              const codeAgent = agentOfLoginCodeKey(command.key)
              if (codeAgent) delete next[agentLoginKey(codeAgent)]
              return next
            })
          }
          setTracked((current) => current.filter((c) => c.id !== command.id))
          if (result.status === `failed`) {
            setErrors((current) => ({
              ...current,
              [command.key]:
                result.result ?? `The device reported a failure.`,
            }))
          }
        } catch {
          // Transient — keep polling.
        }
      }
    }
    // Leading tick (EXP-490): with the relay nudge an online machine answers
    // in ~1-2s — a trailing-only interval added a guaranteed 2s.
    void tick()
    const interval = setInterval(() => void tick(), online ? 2_000 : 8_000)
    return () => {
      cancelled = true
      clearInterval(interval)
    }
  }, [active, tracked, online])

  const pendingKey = (key: string) =>
    tracked.some((command) => command.key === key)

  const queueLogin = (
    agent: string,
    switchAccount: boolean,
    target: AgentLoginProfileTarget = {}
  ) => {
    // A fresh login supersedes whatever its code round trip last said.
    const codeKey = agentLoginCodeKey(agent)
    setResults((current) => {
      if (!(codeKey in current)) return current
      const next = { ...current }
      delete next[codeKey]
      return next
    })
    setErrors((current) => ({ ...current, [codeKey]: `` }))
    void queueCommand(agentLoginKey(agent), {
      kind: `agent_login`,
      agent,
      switch: switchAccount,
      ...(target.profileId ? { profileId: target.profileId } : {}),
      ...(target.newProfileLabel
        ? { newProfileLabel: target.newProfileLabel }
        : {}),
    })
  }

  const queueLoginCode = (agent: string, code: string) =>
    void queueCommand(agentLoginCodeKey(agent), {
      kind: `agent_login_code`,
      agent,
      code,
    })

  const stateFor = (agent: string): AgentLoginState => ({
    pending: pendingKey(agentLoginKey(agent)),
    result: results[agentLoginKey(agent)] ?? null,
    error: errors[agentLoginKey(agent)] ?? ``,
    codePending: pendingKey(agentLoginCodeKey(agent)),
    codeResult: results[agentLoginCodeKey(agent)] ?? null,
    codeError: errors[agentLoginCodeKey(agent)] ?? ``,
  })

  return { stateFor, queueLogin, queueLoginCode, busy: tracked.length > 0 }
}
