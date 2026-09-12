// EXP-792 (EXP-747 A1/A2): the sign-in dialog — the ONE place an agent login
// is driven from the web, so a failed remote start's toast, a device row's
// account chip and an account row's "+" all open the SAME flow without going
// through device settings first. Queues `agent_login` on open (a fresh dialog
// IS the intent), then renders what the device hands back: the CLI's sign-in
// link, Codex's device code, claude's code field (EXP-765).
//
// EXP-862: a title and ONE status line, ×4 (iOS `AgentLoginSheet`, desktop's
// login dialog) — and it CLOSES ITSELF the moment the device reports the
// login as signed in. Nothing here writes, holds or forwards a credential.
import { useEffect, useRef } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { LoaderCircle } from "lucide-react"
import type { Device } from "@/db/schema"
import { useAgentLogin } from "@/hooks/use-agent-login"
import { deviceCollection } from "@/lib/collections"
import { SYSTEM_PROFILE_ID } from "@/lib/agent-usage"
import {
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { AgentLoginOutcome } from "@/components/device-agent-account"
import { agentLabel } from "@/components/agent-picker"
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"

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
  // and its next heartbeat reports the profile as signed in — that TRANSITION
  // (never the state it opened in) is the signal, so re-signing a healthy
  // login stays open until it really lands.
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
  const profileId = target?.profileId
  const signedIn = !account
    ? false
    : profileId && profileId !== SYSTEM_PROFILE_ID
      ? (account.profiles ?? []).some(
          (profile) => profile.id === profileId && profile.signedIn === true
        )
      : account.signedIn === true
  const wasSignedIn = useRef<boolean | null>(null)
  useEffect(() => {
    if (!open) {
      wasSignedIn.current = null
      return
    }
    if (wasSignedIn.current === null) {
      wasSignedIn.current = signedIn
      return
    }
    if (!wasSignedIn.current && signedIn) onOpenChange(false)
    wasSignedIn.current = signedIn
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, signedIn])

  // The ONE status line, byte-identical ×4: who is signing in where, and what
  // is happening right now — never a paragraph.
  const statusText = state.codePending
    ? `Sending the code to ${label}…`
    : state.pending
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
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            {(state.pending || state.codePending) && (
              <LoaderCircle className="size-3 animate-spin" />
            )}
            {statusText}
          </p>
          {state.result && (
            <AgentLoginOutcome
              result={state.result}
              codePending={state.codePending}
              onEnterCode={(code) => login.queueLoginCode(agent, code)}
            />
          )}
          {state.error && (
            <p className="text-xs text-destructive">{state.error}</p>
          )}
          {state.codeResult && (
            <p className="text-xs text-muted-foreground">{state.codeResult}</p>
          )}
          {state.codeError && (
            <p className="text-xs text-destructive">{state.codeError}</p>
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
