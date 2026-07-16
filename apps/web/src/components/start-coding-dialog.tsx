import { useEffect, useState } from "react"
import { Loader2, MonitorUp } from "lucide-react"
import { contract } from "@exp/domain-contract"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { Switch } from "@/components/ui/switch"
import {
  readCodingLaunchPrefs,
  rememberCodingLaunchPrefs,
  type CodingLaunchPrefs,
} from "@/lib/coding-launch-prefs"

// The remote Start-coding options dialog (EXP-149) — the web twin of the
// desktop IDE's Start-coding dialog (single-issue mode): Model / Effort
// selects, ultracode switch (it IS `--effort ultracode`, so it disables the
// Effort select), plan-mode switch (default OFF — the session runs on an
// unattended desktop), plus the device picker when more than one desktop is
// online. Last-used options persist per device (`coding-launch-prefs.ts`).

export interface SteerDevice {
  deviceId: string
  deviceLabel: string
  connectedAt: number
}

/** The resolved dialog choices sent with `steer.startSession` — the same
 * shape the prefs module persists. */
export type StartCodingOptions = CodingLaunchPrefs

// Radix Select forbids an empty-string item value; the blank "CLI default"
// effort rides this sentinel inside the dialog only.
const CLI_DEFAULT_EFFORT = `cli-default`

// Display labels derive from the contract values (same rule as the iOS and
// Android sheets), so a new contract value can never render unlabeled.
function modelLabel(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1)
}

function effortLabel(value: string): string {
  return value === `xhigh` ? `XHigh` : modelLabel(value)
}

export function StartCodingDialog({
  open,
  onOpenChange,
  devices,
  starting,
  onStart,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  devices: SteerDevice[]
  starting: boolean
  onStart: (device: SteerDevice, options: StartCodingOptions) => void
}) {
  const [model, setModel] = useState(contract.codingModel.values[0])
  const [effortValue, setEffortValue] = useState(CLI_DEFAULT_EFFORT)
  const [ultracode, setUltracode] = useState(false)
  const [planMode, setPlanMode] = useState(false)
  const [deviceId, setDeviceId] = useState<string | null>(null)

  // Seed from the last-used options on every open (and settle the device
  // choice on the first online desktop).
  useEffect(() => {
    if (!open) return
    const prefs = readCodingLaunchPrefs()
    setModel(prefs.model)
    setEffortValue(prefs.effort === `` ? CLI_DEFAULT_EFFORT : prefs.effort)
    setUltracode(prefs.ultracode)
    setPlanMode(prefs.planMode)
    setDeviceId(devices[0]?.deviceId ?? null)
  }, [open, devices])

  const device =
    devices.find((candidate) => candidate.deviceId === deviceId) ?? devices[0]

  const submit = () => {
    if (!device) return
    const options: StartCodingOptions = {
      model,
      effort: effortValue === CLI_DEFAULT_EFFORT ? `` : effortValue,
      ultracode,
      planMode,
    }
    rememberCodingLaunchPrefs(options)
    onStart(device, options)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Start coding</DialogTitle>
          <DialogDescription>
            {devices.length === 1 && device
              ? `Launch a Claude session for this issue on ${device.deviceLabel}.`
              : `Launch a Claude session for this issue on one of your desktops.`}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          {devices.length > 1 && (
            <div className="space-y-2">
              <Label htmlFor="start-coding-device">Desktop</Label>
              <Select
                value={device?.deviceId ?? ``}
                onValueChange={setDeviceId}
              >
                <SelectTrigger id="start-coding-device" className="w-full">
                  <SelectValue placeholder="Select a desktop" />
                </SelectTrigger>
                <SelectContent>
                  {devices.map((candidate) => (
                    <SelectItem
                      key={candidate.deviceId}
                      value={candidate.deviceId}
                    >
                      {candidate.deviceLabel || candidate.deviceId}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-2">
              <Label htmlFor="start-coding-model">Model</Label>
              <Select value={model} onValueChange={setModel}>
                <SelectTrigger id="start-coding-model" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {contract.codingModel.values.map((value) => (
                    <SelectItem key={value} value={value}>
                      {modelLabel(value)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="start-coding-effort">Effort</Label>
              <Select
                value={effortValue}
                onValueChange={setEffortValue}
                disabled={ultracode}
              >
                <SelectTrigger id="start-coding-effort" className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={CLI_DEFAULT_EFFORT}>
                    CLI default
                  </SelectItem>
                  {contract.codingEffort.values.map((value) => (
                    <SelectItem key={value} value={value}>
                      {effortLabel(value)}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </div>
          <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
            <div className="space-y-0.5 pr-3">
              <Label htmlFor="start-coding-ultracode">Ultracode</Label>
              <p className="text-xs text-muted-foreground">
                Dynamic multi-agent workflows — overrides the effort level.
              </p>
            </div>
            <Switch
              id="start-coding-ultracode"
              checked={ultracode}
              onCheckedChange={setUltracode}
            />
          </div>
          <div className="flex items-center justify-between rounded-md border border-border px-3 py-2">
            <div className="space-y-0.5 pr-3">
              <Label htmlFor="start-coding-plan-mode">Plan mode</Label>
              <p className="text-xs text-muted-foreground">
                Starts with a plan you approve — from this page or at the
                desktop.
              </p>
            </div>
            <Switch
              id="start-coding-plan-mode"
              checked={planMode}
              onCheckedChange={setPlanMode}
            />
          </div>
        </div>
        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={starting}
          >
            Cancel
          </Button>
          <Button onClick={submit} disabled={starting || !device}>
            {starting ? <Loader2 className="animate-spin" /> : <MonitorUp />}
            Start coding
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
