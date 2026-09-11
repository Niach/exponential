// EXP-792 (EXP-747 A1/A2): "Sign in to <agent> on <machine>" as a dialog of
// its own, so a failed remote start's toast, a machine row's Sign in pill and
// the Devices nav can all open the SAME sign-in without going through device
// settings first. Queues `agent_login` on open (a fresh dialog IS the
// intent), then renders what the machine hands back — the CLI's sign-in link,
// Codex's device code, claude's code field (EXP-765) — via the outcome view
// the device-settings tab already uses. The machine flips `signedIn` on its
// next probe; nothing here writes a credential.
import { useEffect, useRef } from "react"
import { LoaderCircle } from "lucide-react"
import { useAgentLogin } from "@/hooks/use-agent-login"
import {
  deviceIsOnline,
  type SteerDevice,
} from "@/lib/steer-devices"
import { AgentLoginOutcome } from "@/components/device-agent-account"
import { agentLabel } from "@/components/agent-usage-bar"
import {
  Dialog,
  DialogContent,
  DialogDescription,
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
  const addsAccount = Boolean(target?.newProfileLabel)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent mobile="sheet" className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>
            {addsAccount
              ? `Add a ${agentLabel(agent)} account`
              : `Sign in to ${agentLabel(agent)}`}
          </DialogTitle>
          <DialogDescription>
            {addsAccount
              ? `The sign-in runs on ${label}. Sign in with the account you want to add; the machine keeps it beside its other logins.`
              : `The sign-in runs on ${label}. Open the link it hands back on any device.`}
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-2">
          {state.pending && (
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <LoaderCircle className="size-3 animate-spin" />
              {online
                ? `Waiting for the sign-in link…`
                : `This machine is offline. The sign-in runs when it comes online.`}
            </p>
          )}
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
          {state.codePending && (
            <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
              <LoaderCircle className="size-3 animate-spin" />
              Sending the code to the machine…
            </p>
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
