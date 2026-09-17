// EXP-792 (EXP-747 A1/A2): the sign-in dialog — the ONE place an agent login
// is driven from the web, so a failed remote start's toast, a device row's
// account chip and an account row's "+" all open the SAME flow without going
// through device settings first. Queues `agent_login` on open (a fresh dialog
// IS the intent), then renders what the device hands back: the CLI's sign-in
// link, Codex's device code, claude's code field (EXP-765).
//
// EXP-862: a title and ONE status line, ×4 (iOS `AgentLoginSheet`, desktop's
// login dialog). Nothing here writes, holds or forwards a credential.
//
// EXP-940: and it ENDS. The code goes back to the machine, the dialog spins on
// "Signing in…", the device's next heartbeat lands the login and turns that
// into "Signed in", and the dialog closes a beat later. It used to stop on the
// device's own sentence about finishing the sign-in and sit there forever,
// whether or not the sign-in ever landed; a wait that never lands is now a
// short error with a "Try again" that re-queues the login.
import { useEffect, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import type { Device } from "@/db/schema"
import { useAgentLogin } from "@/hooks/use-agent-login"
import { deviceCollection } from "@/lib/collections"
import {
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  agentLoginLanded,
  AgentLoginOutcome,
} from "@/components/device-agent-account"
import { agentLabel } from "@/components/agent-picker"
import {
  Button,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  conceptIcon,
} from "@exp/ui"

// Multi-client surface (iOS `AgentLoginSheet`, the IDE's login dialog) — a
// CONCEPT, never a raw glyph.
const CheckIcon = conceptIcon(`ui-check`)

// EXP-940: the three lines the end of a sign-in can say, short and ×4.
export const SIGNING_IN = `Signing in…`
export const SIGNED_IN = `Signed in`
export const SIGN_IN_TIMED_OUT = `The machine did not confirm the sign-in.`

/** How long the success notice stays up before the dialog closes itself. */
const SUCCESS_LINGER_MS = 1_500
/** How long a submitted code waits for the machine's next heartbeat to report
 * the login before the dialog gives up and offers a retry. A probe plus a
 * heartbeat is ~30s, so this is three beats of headroom. */
const SIGN_IN_TIMEOUT_MS = 120_000

export interface AgentLoginTarget {
  device: SteerDevice
  agent: string
  /** EXP-827: sign into this EXISTING profile on the machine (absent = the
   *  ambient login). */
  profileId?: string
  /** EXP-827: create a profile with this label on the machine, then sign
   *  into it — "Add account" / "add this machine to an account". */
  newProfileLabel?: string
}

export function AgentLoginDialog({
  target,
  open,
  onOpenChange,
}: {
  /** The machine (one of the caller's own) and the agent to sign in. */
  target: AgentLoginTarget | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const device = target?.device
  const agent = target?.agent ?? ``
  const online = device ? deviceIsOnline(device) : false
  const login = useAgentLogin({
    deviceId: device?.deviceId,
    online,
    active: open,
  })
  const state = login.stateFor(agent)

  // Queue once per open — the dialog opening IS the sign-in request. A
  // signed-out agent never needs the switch arm, and the codex logout
  // confirmation belongs to Switch account in device settings.
  const queuedRef = useRef<string | null>(null)
  useEffect(() => {
    if (!open || !device || !agent) {
      queuedRef.current = null
      return
    }
    const key = `${device.deviceId}:${agent}:${target?.profileId ?? ``}:${target?.newProfileLabel ?? ``}`
    if (queuedRef.current === key) return
    queuedRef.current = key
    login.queueLogin(agent, false, {
      profileId: target?.profileId,
      newProfileLabel: target?.newProfileLabel,
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId, agent, target?.profileId, target?.newProfileLabel])

  const label = device ? device.deviceLabel || device.deviceId : ``

  // EXP-862: closes itself on success. The device re-probes after the login
  // and its next heartbeat reports the TARGETED login as usable — that
  // TRANSITION (never the state it opened in) is the signal, so re-signing a
  // healthy login stays open until it really lands. `agentLoginLanded` is the
  // predicate: a revoked credential and a not-yet-created profile both read
  // as "signed in" on the account itself.
  const { data: deviceRows } = useLiveQuery(
    (query) =>
      open && device?.rowId
        ? query
            .from({ d: deviceCollection })
            .where(({ d }) => eq(d.id, device.rowId))
        : undefined,
    [open, device?.rowId]
  )
  const row = (deviceRows?.[0] as Device | undefined) ?? null
  const account = agent ? (row?.agentAccounts?.[agent] ?? null) : null
  const landed = agentLoginLanded(account, {
    profileId: target?.profileId,
    newProfileLabel: target?.newProfileLabel,
  })
  // EXP-940: it no longer VANISHES on that transition. The code goes in, the
  // dialog spins on "Signing in…", the landing turns it into "Signed in", and
  // the dialog closes a beat later — so the flow ends with an answer instead
  // of a sentence about a machine that may never come back.
  const [signing, setSigning] = useState(false)
  const [signedIn, setSignedIn] = useState(false)
  const [timedOut, setTimedOut] = useState(false)
  const hadLanded = useRef<boolean | null>(null)
  useEffect(() => {
    if (!open) {
      hadLanded.current = null
      setSigning(false)
      setSignedIn(false)
      setTimedOut(false)
      return
    }
    if (hadLanded.current === null) {
      hadLanded.current = landed
      return
    }
    if (!hadLanded.current && landed) setSignedIn(true)
    hadLanded.current = landed
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, landed])

  useEffect(() => {
    if (!open || !signedIn) return
    const timer = setTimeout(() => onOpenChange(false), SUCCESS_LINGER_MS)
    return () => clearTimeout(timer)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, signedIn])

  // A waiting sign-in is bounded: the machine reports its logins on every
  // heartbeat, so silence past the bound is a failure, not progress.
  useEffect(() => {
    if (!open || !signing || signedIn) return
    const timer = setTimeout(() => setTimedOut(true), SIGN_IN_TIMEOUT_MS)
    return () => clearTimeout(timer)
  }, [open, signing, signedIn])

  // Re-queue the login from the top: the same command the dialog fires on
  // open, with the phase reset so the status line speaks again.
  const retry = () => {
    if (!device || !agent) return
    setTimedOut(false)
    setSigning(false)
    setSignedIn(false)
    hadLanded.current = landed
    login.queueLogin(agent, false, {
      profileId: target?.profileId,
      newProfileLabel: target?.newProfileLabel,
    })
  }

  const failure = timedOut ? SIGN_IN_TIMED_OUT : state.codeError || state.error
  const waiting = signing || state.codePending

  // The ONE status line, byte-identical ×4: who is signing in where, and what
  // is happening right now — never a paragraph.
  const statusText = state.pending
    ? `Waiting for ${label} to publish the ${agentLabel(agent)} sign-in link…`
    : `${agentLabel(agent)} on ${label}`

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        mobile="sheet"
        className="sm:max-w-md"
        aria-describedby={undefined}
      >
        <DialogHeader>
          <DialogTitle>Sign in</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          {/* Who is signing in where — it stays put through every phase. */}
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            {state.pending && (
              <LoaderCircle className="size-3 shrink-0 animate-spin" />
            )}
            {statusText}
          </p>
          {signedIn ? (
            <p className="flex items-center gap-1.5 text-xs text-foreground">
              <CheckIcon className="size-3.5 shrink-0 text-emerald-500" />
              {SIGNED_IN}
            </p>
          ) : failure ? (
            <>
              <p className="text-xs text-destructive">{failure}</p>
              <Button
                variant="glass"
                size="sm"
                className="w-fit"
                onClick={retry}
              >
                Try again
              </Button>
            </>
          ) : waiting ? (
            // The code is in; the machine finishes on its own and reports the
            // login on its next heartbeat.
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <LoaderCircle className="size-3 shrink-0 animate-spin" />
              {SIGNING_IN}
            </p>
          ) : (
            state.result && (
              <AgentLoginOutcome
                result={state.result}
                codePending={state.codePending}
                onEnterCode={(code) => {
                  setSigning(true)
                  login.queueLoginCode(agent, code)
                }}
              />
            )
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}

// ── One host per team layout ────────────────────────────────────────────────
// A remote start can fail from nine surfaces (`useRemoteStart` callers), and
// a sonner toast's action runs outside any of them. So the request is a tiny
// external store: `requestAgentLogin` from anywhere, ONE `AgentLoginDialogHost`
// mounted in the team route renders it.
import { useSyncExternalStore } from "react"

let pendingTarget: AgentLoginTarget | null = null
const listeners = new Set<() => void>()

function emit() {
  for (const listener of listeners) listener()
}

export function requestAgentLogin(target: AgentLoginTarget): void {
  pendingTarget = target
  emit()
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

export function AgentLoginDialogHost() {
  const target = useSyncExternalStore(
    subscribe,
    () => pendingTarget,
    () => null
  )
  return (
    <AgentLoginDialog
      target={target}
      open={target !== null}
      onOpenChange={(open) => {
        if (!open) {
          pendingTarget = null
          emit()
        }
      }}
    />
  )
}
