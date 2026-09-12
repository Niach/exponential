import { useEffect, useMemo, useState } from "react"
import type { Device } from "@/db/schema"
import { AgentPicker, agentLabel } from "@/components/agent-picker"
import { requestAgentLogin } from "@/components/agent-login-dialog"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { GlassGroup, GlassPickerRow } from "@/components/ui/glass-rows"
import {
  addAccountDevices,
  addAccountLoginTarget,
  addableAgents,
  nextProfileLabel,
} from "@/lib/agent-account-add"
import { steerDeviceFromRow } from "@/lib/steer-devices"

// EXP-827: "Add account" — pick one of MY online machines (cap `agent-login`)
// and an agent installed there, then hand off to the shared sign-in dialog
// (`AgentLoginDialogHost`) with a `newProfileLabel` (or the ambient login
// while that is still free): the machine creates the profile, runs the CLI's
// own login in it and reports it on its next probe, so the account appears
// in the Accounts section by itself.

export function AddAccountDialog({
  open,
  onOpenChange,
  devices,
  currentUserId,
  now,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** Every synced device row; the dialog keeps the caller's own. */
  devices: Device[]
  currentUserId: string
  now: Date
}) {
  const candidates = useMemo(
    () => addAccountDevices(devices, { currentUserId, now }),
    [devices, currentUserId, now]
  )
  const [deviceId, setDeviceId] = useState<string>(``)
  const [agent, setAgent] = useState<string>(``)

  // Defaults latch on open: the first machine, its first agent.
  useEffect(() => {
    if (!open) return
    setDeviceId((current) =>
      candidates.some((row) => row.deviceId === current)
        ? current
        : (candidates[0]?.deviceId ?? ``)
    )
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])
  const device = candidates.find((row) => row.deviceId === deviceId) ?? null
  const agents = useMemo(() => (device ? addableAgents(device) : []), [device])
  useEffect(() => {
    setAgent((current) => (agents.includes(current) ? current : (agents[0] ?? ``)))
  }, [agents])

  const label = device ? device.label || device.deviceId : ``
  const canContinue = device !== null && agent !== ``

  const submit = () => {
    if (!device || !agent) return
    const target = addAccountLoginTarget(
      device,
      agent,
      nextProfileLabel(device, agent, agentLabel(agent))
    )
    const steerDevice = steerDeviceFromRow(device, { now, currentUserId })
    onOpenChange(false)
    // The sign-in dialog is hosted elsewhere in the tree; open it after this
    // one closed (the Radix close + focus return would swallow it otherwise).
    setTimeout(() => requestAgentLogin({ device: steerDevice, agent, ...target }), 0)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent mobile="sheet" className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add account</DialogTitle>
          <DialogDescription>
            {device
              ? `The sign-in runs on ${label}. Sign in with the account you want to add.`
              : `Sign in with another account on one of your machines.`}
          </DialogDescription>
        </DialogHeader>
        {candidates.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            None of your machines is online with an agent that can sign in
            remotely. Open the desktop app or start the daemon there first.
          </p>
        ) : (
          <GlassGroup>
            <GlassPickerRow
              label="Machine"
              value={deviceId}
              onValueChange={setDeviceId}
              placeholder="Pick a machine"
              options={candidates.map((row) => ({
                value: row.deviceId,
                label: row.label || row.deviceId,
              }))}
            />
            {/* EXP-862: the ONE agent picker — brand mark + chevron, the
                label in its tooltip and its menu rows. */}
            <div className="flex items-center gap-3 px-4 py-3">
              <span className="text-sm text-foreground">Agent</span>
              <span className="ml-auto">
                <AgentPicker
                  value={agent}
                  agents={agents}
                  onChange={setAgent}
                  disabled={agents.length === 0}
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
          <Button disabled={!canContinue} onClick={submit} data-testid="add-account-continue">
            Continue
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
