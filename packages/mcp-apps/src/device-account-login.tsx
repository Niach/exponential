import { useEffect, useRef, useState } from "react"
import {
  AgentPicker,
  Button,
  DEVICE_READINESS_COPY,
  Input,
  conceptIcon,
} from "@exp/ui"
import { useMcpActions } from "./actions"
import {
  ACCOUNT_LOGIN_TOOL,
  ADD_ACCOUNT_LABEL,
  DEVICE_AGENT_LABEL,
  SIGNED_IN,
  SIGNING_IN,
  SIGN_IN_TIMED_OUT,
  addableAgents,
  loginBaseline,
  loginLanding,
  parseLoginStep,
  type DeviceListRow,
  type LoginTarget,
} from "./device-usage"

const AddIcon = conceptIcon(`ui-add`)
const CheckIcon = conceptIcon(`ui-check`)
const CopyIcon = conceptIcon(`ui-copy`)
const ExternalLinkIcon = conceptIcon(`ui-external-link`)
const LoadingIcon = conceptIcon(`ui-loading`)
const SignInIcon = conceptIcon(`ui-sign-in`)

// EXP-1199 — the web's sign-in dialog (`agent-login-dialog.tsx` +
// `AgentLoginOutcome`, ×4 with iOS `AgentLoginSheet` and the IDE's dialog)
// inline under a device, driven by `exponential_devices_account_login`: the
// machine publishes the sign-in link (+ codex's code), claude's browser code
// goes back through the same tool, and the view re-reads
// `exponential_devices_list` until a profile's `lastLoginAt` moves (the login
// lands on the profile with its email; an email the machine already held
// says `alreadyAdded`). Only the link and the typed code travel; the
// credential stays on the machine.

/** Re-read the devices while a sign-in is open (hosts allow ~30 calls/min). */
export const LOGIN_DEVICES_POLL_MS = 5_000
/** A submitted code waits three heartbeats for the machine to report it. */
const SIGN_IN_TIMEOUT_MS = 120_000
const SUCCESS_LINGER_MS = 1_500
/** Each call waits ~25s server-side; 24 of them ≈ the device's 10-min login. */
const MAX_PENDING_CALLS = 24

type Phase =
  | { kind: `waiting` }
  | { kind: `url`; url: string; code: string | null }
  | { kind: `signing` }
  | { kind: `signed`; alreadyAdded: string | null }
  | { kind: `failed`; message: string }

/** "Add account" under one of your own machines: pick the agent, then the
 *  sign-in flow. */
export function AddAccountRow({
  device,
  onDevices,
}: {
  device: DeviceListRow
  onDevices: (rows: readonly DeviceListRow[]) => void
}) {
  const agents = addableAgents(device)
  const [open, setOpen] = useState(false)
  const [agent, setAgent] = useState(agents[0] ?? ``)
  const [target, setTarget] = useState<LoginTarget | null>(null)
  if (agents.length === 0) return null
  if (target) {
    return (
      <AccountLoginFlow
        device={device}
        agent={agent}
        target={target}
        onDevices={onDevices}
        onDone={() => {
          setTarget(null)
          setOpen(false)
        }}
      />
    )
  }
  if (!open) {
    return (
      <Button
        variant="ghost"
        size="sm"
        className="h-6 w-fit px-1 text-[11px] text-muted-foreground"
        onClick={() => {
          // One agent: nothing to ask, the sign-in starts at once.
          if (agents.length === 1) setTarget({})
          else setOpen(true)
        }}
        data-testid={`add-account-${device.deviceId}`}
      >
        <AddIcon className="size-3" />
        {ADD_ACCOUNT_LABEL}
      </Button>
    )
  }
  return (
    <div className="flex items-center gap-2 text-xs">
      <span className="text-muted-foreground">Agent</span>
      <AgentPicker value={agent} agents={agents} onChange={setAgent} align="start" />
      <Button
        variant="glass"
        size="sm"
        className="h-6 px-2 text-[11px]"
        disabled={!agent}
        onClick={() => setTarget({})}
        data-testid="add-account-continue"
      >
        Continue
      </Button>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 px-1 text-[11px] text-muted-foreground"
        onClick={() => setOpen(false)}
      >
        Cancel
      </Button>
    </div>
  )
}

export function AccountLoginFlow({
  device,
  agent,
  target,
  onDevices,
  onDone,
}: {
  device: DeviceListRow
  agent: string
  target: LoginTarget
  onDevices: (rows: readonly DeviceListRow[]) => void
  onDone: () => void
}) {
  const { call, openLink } = useMcpActions()
  const [phase, setPhase] = useState<Phase>({ kind: `waiting` })
  const [attempt, setAttempt] = useState(0)
  const [draft, setDraft] = useState(``)
  const deviceId = device.deviceId
  const machine = device.label || deviceId
  const agentName = DEVICE_AGENT_LABEL[agent] ?? agent
  // `agent-login-dialog.tsx`: the baseline is captured when the sign-in is
  // queued (each attempt), and only a `lastLoginAt` that moves past it lands.
  const baseline = useRef(loginBaseline(device, agent))

  // The flow opening IS the sign-in request; a pending answer is checked on
  // by its commandId until the machine publishes the link.
  useEffect(() => {
    let live = true
    const run = async () => {
      let args: Record<string, unknown> = {
        deviceId,
        agent,
        ...(target.profileId ? { profileId: target.profileId } : {}),
      }
      for (let n = 0; n < MAX_PENDING_CALLS && live; n += 1) {
        const result = await call(ACCOUNT_LOGIN_TOOL, args)
        if (!live) return
        const step = result.kind === `ok` ? parseLoginStep(result.data) : null
        if (result.kind === `error` || !step) {
          setPhase({
            kind: `failed`,
            message: result.kind === `error` ? result.message : `The device reported a failure.`,
          })
          return
        }
        if (step.status === `pending`) {
          args = { deviceId, agent, commandId: step.commandId }
          continue
        }
        if (step.status === `url`) setPhase({ kind: `url`, url: step.url, code: step.code })
        else if (step.status === `failed`) setPhase({ kind: `failed`, message: step.message })
        else setPhase({ kind: `signing` })
        return
      }
      if (live) setPhase({ kind: `failed`, message: SIGN_IN_TIMED_OUT })
    }
    setPhase({ kind: `waiting` })
    baseline.current = loginBaseline(device, agent)
    void run()
    return () => {
      live = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attempt])

  // While the link is out (codex lands without a code) or the code is in,
  // re-read the machines until the login lands.
  const watching = phase.kind === `url` || phase.kind === `signing`
  useEffect(() => {
    if (!watching) return
    let live = true
    const tick = async () => {
      const result = await call<DeviceListRow[]>(`exponential_devices_list`, {})
      if (!live || result.kind !== `ok` || !Array.isArray(result.data)) return
      onDevices(result.data)
      const fresh = result.data.find((row) => row.deviceId === deviceId)
      const landing = fresh ? loginLanding(fresh, agent, baseline.current, target) : null
      if (landing) {
        setPhase({
          kind: `signed`,
          alreadyAdded: landing.duplicate
            ? DEVICE_READINESS_COPY.alreadyAdded.replace(`{email}`, landing.email)
            : null,
        })
      }
    }
    const timer = setInterval(() => void tick(), LOGIN_DEVICES_POLL_MS)
    return () => {
      live = false
      clearInterval(timer)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [watching, attempt])

  useEffect(() => {
    if (phase.kind !== `signing`) return
    const timer = setTimeout(
      () => setPhase({ kind: `failed`, message: SIGN_IN_TIMED_OUT }),
      SIGN_IN_TIMEOUT_MS
    )
    return () => clearTimeout(timer)
  }, [phase.kind])

  useEffect(() => {
    if (phase.kind !== `signed`) return
    const timer = setTimeout(onDone, SUCCESS_LINGER_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [phase.kind])

  const submitCode = async () => {
    const code = draft.trim()
    if (!code) return
    setDraft(``)
    setPhase({ kind: `signing` })
    const result = await call(ACCOUNT_LOGIN_TOOL, { deviceId, agent, code })
    const step = result.kind === `ok` ? parseLoginStep(result.data) : null
    if (result.kind === `error`) setPhase({ kind: `failed`, message: result.message })
    else if (step?.status === `failed`) setPhase({ kind: `failed`, message: step.message })
  }

  return (
    <div
      className="flex flex-col gap-1.5 rounded-md border border-border/60 p-2"
      data-testid={`account-login-${deviceId}`}
    >
      {/* The ONE status line (×4): who is signing in where. */}
      <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
        {phase.kind === `waiting` && <LoadingIcon className="size-3 shrink-0 animate-spin" />}
        {phase.kind === `waiting`
          ? `Waiting for ${machine} to publish the ${agentName} sign-in link…`
          : `${agentName} on ${machine}`}
      </p>
      {phase.kind === `signed` && (
        <p className="flex items-center gap-1.5 text-xs text-foreground">
          <CheckIcon className="size-3.5 shrink-0 text-emerald-500" />
          {phase.alreadyAdded ?? SIGNED_IN}
        </p>
      )}
      {phase.kind === `signing` && (
        <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <LoadingIcon className="size-3 shrink-0 animate-spin" />
          {SIGNING_IN}
        </p>
      )}
      {phase.kind === `failed` && (
        <>
          <p className="text-xs text-destructive">{phase.message}</p>
          <Button
            variant="glass"
            size="sm"
            className="h-7 w-fit text-xs"
            onClick={() => setAttempt((n) => n + 1)}
          >
            Try again
          </Button>
        </>
      )}
      {phase.kind === `url` && (
        <>
          <Button
            variant="glass"
            size="sm"
            className="h-7 w-fit max-w-full text-xs"
            onClick={() => openLink(phase.url)}
            title={phase.url}
          >
            <ExternalLinkIcon className="size-3 shrink-0" />
            Open sign-in page
          </Button>
          {phase.code && (
            <div className="flex items-center gap-1.5">
              <span className="font-mono text-xs">{phase.code}</span>
              <Button
                variant="ghost"
                className="h-5 w-5 p-0 text-muted-foreground"
                aria-label="Copy code"
                title="Copy code"
                onClick={() => void navigator.clipboard?.writeText(phase.code ?? ``)}
              >
                <CopyIcon className="size-3" />
              </Button>
            </div>
          )}
          {!phase.code && (
            <div className="flex items-center gap-1.5">
              <Input
                value={draft}
                onChange={(event) => setDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === `Enter`) {
                    event.preventDefault()
                    void submitCode()
                  }
                }}
                placeholder="Code from the browser"
                aria-label="Code from the browser"
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
                className="h-7 font-mono text-xs"
              />
              <Button
                variant="glass"
                size="sm"
                className="h-7 shrink-0 text-xs"
                disabled={!draft.trim()}
                onClick={() => void submitCode()}
              >
                <SignInIcon className="size-3" />
                Enter code
              </Button>
            </div>
          )}
          <p className="text-[11px] text-muted-foreground">
            {phase.code
              ? `Open the link on any device and enter the code on the machine.`
              : `Open the link on any device, then paste the code it shows here.`}
          </p>
        </>
      )}
      {phase.kind !== `signed` && (
        <Button variant="ghost" size="sm" className="h-6 w-fit px-1 text-[11px] text-muted-foreground" onClick={onDone}>
          Cancel
        </Button>
      )}
    </div>
  )
}
