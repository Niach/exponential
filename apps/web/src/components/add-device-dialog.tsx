// The Add device dialog (EXP-697, EXP-1111): one of the four hosts of the
// device-setup block (EXP-1169, `device-setup.tsx`), which owns the content:
// desktop download, the CLI one-liner with its one-time install token, the
// device-code approval that copying it reveals, and the caller's own machines.
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"
import type { SteerDevice } from "@/lib/steer-devices"
import {
  buildServerInstallSnippet,
  CopyIconButton,
  DeviceSetup,
} from "@/components/device-setup"

// Re-exported for the other install surfaces and their tests.
export { buildServerInstallSnippet, CopyIconButton }

export function AddDeviceDialog({
  open,
  onOpenChange,
  devices,
  origin,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** The caller's OWN devices (synced shape); null while loading. */
  devices: readonly SteerDevice[] | null
  origin: string
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90dvh] overflow-y-auto sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Add device</DialogTitle>
          <DialogDescription>
            To run coding sessions, install the desktop app.
          </DialogDescription>
        </DialogHeader>
        <DeviceSetup devices={devices} origin={origin} active={open} />
      </DialogContent>
    </Dialog>
  )
}
