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
// into "Signed in", and the dialog closes a beat later. The machine lands a
// login on the profile whose EMAIL it signed in as; an address it already held
// (other than the one clicked) raises the `alreadyAdded` warning toast. It used to stop on the
// device's own sentence about finishing the sign-in and sit there forever,
// whether or not the sign-in ever landed; a wait that never lands is now a
// short error with a "Try again" that re-queues the login.
//
// EXP-1600: a claude sign-in is Keep signed in (`claude setup-token`, one
// year, no refresh grant) by default; one muted line says the trade-off and a
// ghost text button switches to the browser login (Advanced) and back
// (devices-list.json copy, ×4).
import { useEffect, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Device } from "@/db/schema"
import { useAgentLogin } from "@/hooks/use-agent-login"
import { deviceCollection } from "@/lib/collections"
import {
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import {
  agentLoginBaseline,
  agentLoginFailure,
  agentLoginLanding,
  alreadyAddedCopy,
  AgentLoginOutcome,
  type AgentLoginBaseline,
} from "@/components/device-agent-account"
import {
  agentLabel,
  Button,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  conceptIcon,
  Spinner,
  toast,
} from "@exp/ui"
import { failedAction } from "@/lib/action-error"
import { loginIsLongLived } from "@/lib/agent-usage"
import { DEVICES_LIST_COPY } from "@exp/domain-contract/devices-list"

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
  /** The profile the sign-in is FOR (Sign in on a row); absent = Add
   *  account. Only the duplicate check reads it: the login lands on the
   *  profile whose email it signed in as. */
  profileId?: string
  /** EXP-1533 / EXP-1600: a long-lived `claude setup-token` login. Absent =
   *  claude's DEFAULT (long-lived); false = the browser login. Same dialog,
   *  same link + code. */
  longLived?: boolean
  /** EXP-1626: the name a long-lived ADD (no `profileId`) gives its new
   *  profile (Add account's Name field, Switch to Keep signed in's
   *  `switchedName`); ignored for any other login. */
  name?: string
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
  // EXP-1600: the mode this open signs in with — claude starts long-lived,
  // the ghost button flips it (and re-queues), an older device's refusal of
  // a long-lived Add falls back to the browser login once.
  const [longLived, setLongLived] = useState(() =>
    loginIsLongLived(agent, target?.longLived)
  )
  const [fellBack, setFellBack] = useState(false)
  const openKey = `${device?.deviceId ?? ``}:${agent}:${target?.profileId ?? ``}:${String(target?.longLived)}:${target?.name ?? ``}`
  const lastOpenKey = useRef<string | null>(null)
  if (open && lastOpenKey.current !== openKey) {
    lastOpenKey.current = openKey
    const initial = loginIsLongLived(agent, target?.longLived)
    if (initial !== longLived) setLongLived(initial)
    if (fellBack) setFellBack(false)
  }
  if (!open && lastOpenKey.current !== null) lastOpenKey.current = null

  // EXP-862: closes itself on success. The device stamps `lastLoginAt` on the
  // profile a login COMMITS into and its next heartbeat reports it — a move
  // against the baseline captured at queue time is the signal
  // (`agentLoginLanding`), so re-signing a healthy login stays open until it
  // really lands, wherever it lands.
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

  // Queue once per open — the dialog opening IS the sign-in request — and
  // capture the baseline beside it (or as soon as the row is there). A
  // signed-out agent never needs the switch arm, and the codex logout
  // confirmation belongs to Switch account in device settings.
  const queuedRef = useRef<string | null>(null)
  const baselineRef = useRef<AgentLoginBaseline | null>(null)
  useEffect(() => {
    if (!open || !device || !agent) {
      queuedRef.current = null
      baselineRef.current = null
      return
    }
    const key = `${device.deviceId}:${agent}:${target?.profileId ?? ``}:${longLived}:${target?.name ?? ``}`
    if (queuedRef.current === key) return
    queuedRef.current = key
    baselineRef.current = row ? agentLoginBaseline(account) : null
    login.queueLogin(agent, false, {
      profileId: target?.profileId,
      longLived,
      name: target?.name,
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, device?.deviceId, agent, target?.profileId, longLived, target?.name])
  if (open && queuedRef.current !== null && baselineRef.current === null && row) {
    baselineRef.current = agentLoginBaseline(account)
  }
  const landing = baselineRef.current
    ? agentLoginLanding(account, baselineRef.current, target?.profileId)
    : null

  const label = device ? device.deviceLabel || device.deviceId : ``
  // EXP-940: it no longer VANISHES on that transition. The code goes in, the
  // dialog spins on "Signing in…", the landing turns it into "Signed in", and
  // the dialog closes a beat later — so the flow ends with an answer instead
  // of a sentence about a machine that may never come back.
  const [signing, setSigning] = useState(false)
  const [signedIn, setSignedIn] = useState(false)
  const [timedOut, setTimedOut] = useState(false)
  useEffect(() => {
    if (!open) {
      setSigning(false)
      setSignedIn(false)
      setTimedOut(false)
      return
    }
    if (!landing || signedIn) return
    setSignedIn(true)
    // The login landed on a profile the machine already held, and not the
    // one clicked: that one is refreshed, the clicked one is untouched.
    if (landing.duplicate) toast.warning(alreadyAddedCopy(landing.email))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, landing?.profileId, landing?.duplicate])

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
    baselineRef.current = row ? agentLoginBaseline(account) : null
    login.queueLogin(agent, false, {
      profileId: target?.profileId,
      longLived,
      name: target?.name,
    })
  }

  // EXP-1600: flip between Keep signed in and the browser login; the queue
  // effect re-queues on the new mode.
  const switchMode = () => {
    setTimedOut(false)
    setSigning(false)
    setSignedIn(false)
    setLongLived((current) => !current)
  }

  const rawFailure = timedOut
    ? SIGN_IN_TIMED_OUT
    : state.codeError || state.error || agentLoginFailure(state.result)
  // An older device refuses a long-lived Add: fall back to the browser login
  // once instead of failing.
  const refusedAdd =
    longLived &&
    !target?.profileId &&
    !fellBack &&
    rawFailure === DEVICES_LIST_COPY.longLivedRefused
  useEffect(() => {
    if (!open || !refusedAdd) return
    setFellBack(true)
    setLongLived(false)
  }, [open, refusedAdd])
  const failure = refusedAdd ? `` : rawFailure
  const waiting = signing || state.codePending

  // EXP-1626: closing before the login landed stops it on the machine
  // (`agent_login_cancel`), so its `claude setup-token` run does not linger.
  // Captured while open: the close clears the host's target in the same
  // render. A landed or already failed login has nothing left to stop.
  const loginCommandId = login.loginCommandIdFor(agent)
  const cancelRef = useRef<{ agent: string; id: string | null; settled: boolean } | null>(null)
  if (open) {
    cancelRef.current = {
      agent,
      id: loginCommandId,
      settled:
        signedIn || Boolean(state.error || agentLoginFailure(state.result)),
    }
  }
  const wasOpenRef = useRef(open)
  useEffect(() => {
    const wasOpen = wasOpenRef.current
    wasOpenRef.current = open
    if (!wasOpen || open) return
    const pending = cancelRef.current
    cancelRef.current = null
    if (pending?.id && !pending.settled) login.cancelLogin(pending.agent, pending.id)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  // FEED-79: a failed sign-in is a toast (once per failure, Retry re-queues
  // the login); the dialog keeps its status line and the Try again button.
  const retryRef = useRef(retry)
  retryRef.current = retry
  useEffect(() => {
    if (!open || !failure) return
    failedAction(`sign in`, agentLabel(agent), failure, {
      retry: () => retryRef.current(),
    })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, failure])

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
            {state.pending && <Spinner />}
            {statusText}
          </p>
          {agent === `claude` && !signedIn && (
            <p className="text-xs text-muted-foreground" data-slot="login-mode-note">
              {longLived
                ? DEVICES_LIST_COPY.keepSignedInNote
                : DEVICES_LIST_COPY.browserLoginNote}
            </p>
          )}
          {signedIn ? (
            <p className="flex items-center gap-1.5 text-xs text-foreground">
              <CheckIcon className="size-3.5 shrink-0 text-emerald-500" />
              {SIGNED_IN}
            </p>
          ) : failure ? (
            <Button
              variant="glass"
              size="sm"
              className="w-fit"
              onClick={retry}
            >
              Try again
            </Button>
          ) : waiting ? (
            // The code is in; the machine finishes on its own and reports the
            // login on its next heartbeat.
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <Spinner label={SIGNING_IN} />
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
          {agent === `claude` && !signedIn && !waiting && (
            <Button
              variant="ghost"
              size="sm"
              className="w-fit px-0 text-xs text-muted-foreground"
              onClick={switchMode}
            >
              {longLived
                ? DEVICES_LIST_COPY.useBrowserLogin
                : DEVICES_LIST_COPY.useKeepSignedIn}
            </Button>
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
