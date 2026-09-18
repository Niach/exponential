import { useEffect, useMemo, useState } from "react"
import {
  AgentPicker,
  agentLabel,
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  GlassGroup,
} from "@exp/ui"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import {
  addAccountLoginTarget,
  addableAgents,
  nextProfileLabel,
} from "@/lib/agent-account-add"
import type { SteerDevice } from "@/lib/steer-devices"

// EXP-827: "Add account" — sign in with another account on a machine and hand
// off to the shared sign-in dialog (`AgentLoginDialogHost`) with a
// `newProfileLabel` (or the ambient login while that is still free): the
// machine creates the profile, runs the CLI's own login in it and reports it
// on its next probe, so the login appears under the device by itself.
//
// EXP-909: DEVICE-BOUND. The dialog used to pick the machine too, because it
// hung off a cross-device Accounts section with no machine in hand; it is
// opened from a row UNDER one now, so the only question left is which agent.

export function AddAccountDialog({
  open,
  onOpenChange,
  device,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The machine the sign-in runs on — the caller already checked it can take
   *  one (own, online, cap `agent-login`, an agent installed). */
  device: SteerDevice
}) {
  const agents = useMemo(() => addableAgents(device), [device])
  const [agent, setAgent] = useState<string>(``)
  // The default latches on open: the machine's first installed agent.
  useEffect(() => {
    if (!open) return
    setAgent((current) => (agents.includes(current) ? current : (agents[0] ?? ``)))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const label = device.deviceLabel || device.deviceId

  const submit = () => {
    if (!agent) return
    const target = addAccountLoginTarget(
      device,
      agent,
      nextProfileLabel(device, agent, agentLabel(agent))
    )
    onOpenChange(false)
    // The sign-in dialog is hosted elsewhere in the tree; open it after this
    // one closed (the Radix close + focus return would swallow it otherwise).
    setTimeout(() => requestAgentLogin({ device, agent, ...target }), 0)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent mobile="sheet" className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add account</DialogTitle>
          <DialogDescription>
            {`The sign-in runs on ${label}. Sign in with the account you want to add.`}
          </DialogDescription>
        </DialogHeader>
        {agents.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {`${label} reports no agent to sign in to.`}
          </p>
        ) : (
          <GlassGroup>
            {/* EXP-862: the ONE agent picker — brand mark + chevron, the
                label in its tooltip and its menu rows. */}
            <div className="flex items-center gap-3 px-4 py-3">
              <span className="text-sm text-foreground">Agent</span>
              <span className="ml-auto">
                <AgentPicker
                  value={agent}
                  agents={agents}
                  onChange={setAgent}
                  align="end"
                />
              </span>
            </div>
          </GlassGroup>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={agent === ``}
            onClick={submit}
            data-testid="add-account-continue"
          >
            Continue
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
