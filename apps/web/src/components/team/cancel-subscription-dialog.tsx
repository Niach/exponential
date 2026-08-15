import { useState } from "react"
import { LoaderCircle } from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"

// Self-service cancellation of the TEAM's subscription (REV2-55: the
// subscription belongs to the team, not to whoever paid for it). Always
// scheduled for the end of the paid period — the team keeps every seat, byte
// and paid feature until then. This is also the prerequisite for deleting a
// paying team: teams.delete refuses while a live subscription exists.
export function CancelSubscriptionDialog({
  teamId,
  planLabel,
  seats,
  periodEnd,
  open,
  onOpenChange,
}: {
  teamId: string
  planLabel: string
  seats: number
  periodEnd: string | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [saving, setSaving] = useState(false)

  const endDate = periodEnd
    ? new Date(periodEnd).toLocaleDateString(undefined, {
        year: `numeric`,
        month: `long`,
        day: `numeric`,
      })
    : null

  const handleCancel = async () => {
    setSaving(true)
    try {
      await trpc.billing.cancelSubscription.mutate({ teamId })
      invalidateBillingCache()
      toast.success(
        endDate
          ? `Subscription cancelled. The team stays on ${planLabel} until ${endDate}.`
          : `Subscription cancelled. The team stays on ${planLabel} until the end of the paid period.`
      )
      onOpenChange(false)
    } catch (err) {
      console.error(`[billing] cancel failed:`, err)
      toast.error(
        err instanceof Error && err.message
          ? err.message
          : `Couldn't cancel the subscription`
      )
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Cancel subscription</DialogTitle>
          <DialogDescription>
            {endDate
              ? `This team keeps ${planLabel} and all ${seats} seat${seats === 1 ? `` : `s`} until ${endDate}, then drops to the Free plan. You won't be charged again.`
              : `This team keeps ${planLabel} until the end of the paid period, then drops to the Free plan. You won't be charged again.`}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-2 text-sm text-muted-foreground">
          <p>
            On Free the team is limited to 3 seats, 250 MB of attachment
            storage and 1 feedback widget. Existing members keep working. Only
            new invites are blocked.
          </p>
          <p>You can resume the subscription any time before it ends.</p>
        </div>

        <DialogFooter>
          <Button
            variant="ghost"
            onClick={() => onOpenChange(false)}
            disabled={saving}
          >
            Keep subscription
          </Button>
          <Button
            variant="destructive"
            onClick={handleCancel}
            disabled={saving}
          >
            {saving && <LoaderCircle className="mr-1.5 size-3.5 animate-spin" />}
            {saving ? `Cancelling...` : `Cancel subscription`}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
