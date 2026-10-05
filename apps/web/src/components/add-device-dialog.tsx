// The Add device dialog (EXP-697, EXP-1111): one of the four hosts of the
// device-setup block (EXP-1169, `device-setup.tsx`), which owns the content:
// desktop download, the CLI one-liner with its one-time install token and
// the device-code approval that copying it reveals. No device list: the
// dialog opens over the page that shows them.
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@exp/ui"
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
  origin,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  origin: string
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* EXP-1196: no description line — the two cards say it. */}
      <DialogContent
        className="max-h-[90dvh] overflow-y-auto sm:max-w-lg"
        aria-describedby={undefined}
      >
        <DialogHeader>
          <DialogTitle>Add device</DialogTitle>
        </DialogHeader>
        <DeviceSetup origin={origin} active={open} />
      </DialogContent>
    </Dialog>
  )
}
