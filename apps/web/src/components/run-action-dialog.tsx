import { useEffect, useMemo, useState } from "react"
import { Loader2, MonitorOff, MonitorUp } from "lucide-react"
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
import type { SteerDevice } from "@/components/start-coding-dialog"
import {
  deviceCanRunActions,
  type RunActionOptions,
} from "@/hooks/use-run-action"

// The "Run action" dialog (EXP-253) — the action twin of the Start-coding
// dialog's device-select half. Action runs are Claude-only v1, so the only
// launch options are the claude Model/Effort contract lists, both defaulting
// to the desktop's own settings. Only actions-capable desktops are offered
// (`steer.startSession` enforces the same server-side).

// Radix Select forbids an empty-string item value; the blank "Desktop
// default" model/effort rides this sentinel inside the dialog only.
const DESKTOP_DEFAULT = `desktop-default`

function valueLabel(value: string): string {
  return value === `xhigh`
    ? `XHigh`
    : value.charAt(0).toUpperCase() + value.slice(1)
}

export function RunActionDialog({
  open,
  onOpenChange,
  actionName,
  devices,
  starting,
  onStart,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  actionName: string
  /** The caller's online desktops; null while presence is in flight. */
  devices: SteerDevice[] | null
  starting: boolean
  onStart: (device: SteerDevice, options: RunActionOptions) => void
}) {
  const [deviceId, setDeviceId] = useState<string | null>(null)
  const [model, setModel] = useState(DESKTOP_DEFAULT)
  const [effort, setEffort] = useState(DESKTOP_DEFAULT)

  const capable = useMemo(
    () => (devices ?? []).filter(deviceCanRunActions),
    [devices]
  )

  // Reset the launch options on OPEN; settle the device on open + whenever
  // the capable list changes (a desktop connecting mid-dialog).
  useEffect(() => {
    if (!open) return
    setModel(DESKTOP_DEFAULT)
    setEffort(DESKTOP_DEFAULT)
  }, [open])

  useEffect(() => {
    if (!open) return
    setDeviceId((current) =>
      current && capable.some((d) => d.deviceId === current)
        ? current
        : (capable[0]?.deviceId ?? null)
    )
  }, [open, capable])

  const device =
    capable.find((candidate) => candidate.deviceId === deviceId) ?? capable[0]

  const submit = () => {
    if (!device) return
    onStart(device, {
      model: model === DESKTOP_DEFAULT ? undefined : model,
      effort: effort === DESKTOP_DEFAULT ? undefined : effort,
    })
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Run action</DialogTitle>
          <DialogDescription>
            {device
              ? `Run "${actionName}" with Claude on ${device.deviceLabel || device.deviceId}.`
              : `Run "${actionName}" with Claude on one of your desktops.`}
          </DialogDescription>
        </DialogHeader>
        {devices === null ? (
          <div className="py-2 text-sm text-muted-foreground">Loading…</div>
        ) : capable.length === 0 ? (
          <div className="flex items-start gap-2 py-2 text-sm text-muted-foreground">
            <MonitorOff className="mt-0.5 size-4 shrink-0" />
            No actions-capable desktop online — open (or update) the
            Exponential desktop app.
          </div>
        ) : (
          <div className="space-y-3">
            {capable.length > 1 && (
              <div className="space-y-2">
                <Label htmlFor="run-action-device">Desktop</Label>
                <Select
                  value={device?.deviceId ?? ``}
                  onValueChange={setDeviceId}
                >
                  <SelectTrigger id="run-action-device" className="w-full">
                    <SelectValue placeholder="Select a desktop" />
                  </SelectTrigger>
                  <SelectContent>
                    {capable.map((candidate) => (
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
                <Label htmlFor="run-action-model">Model</Label>
                <Select value={model} onValueChange={setModel}>
                  <SelectTrigger id="run-action-model" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={DESKTOP_DEFAULT}>
                      Desktop default
                    </SelectItem>
                    {contract.codingModel.values.map((value) => (
                      <SelectItem key={value} value={value}>
                        {valueLabel(value)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="space-y-2">
                <Label htmlFor="run-action-effort">Effort</Label>
                <Select value={effort} onValueChange={setEffort}>
                  <SelectTrigger id="run-action-effort" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={DESKTOP_DEFAULT}>
                      Desktop default
                    </SelectItem>
                    {contract.codingEffort.values.map((value) => (
                      <SelectItem key={value} value={value}>
                        {valueLabel(value)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </div>
          </div>
        )}
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
            Run action
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
