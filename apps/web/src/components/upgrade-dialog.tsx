import { useState } from "react"
import { Sparkles, Users } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { PlanComparison } from "@/components/team/plan-comparison"
import { AdjustSeatsDialog } from "@/components/team/adjust-seats-dialog"
import { useBillingPlan } from "@/hooks/use-billing"

export function UpgradeDialog({
  open,
  onOpenChange,
  title,
  description,
  teamProductId,
  teamYearlyProductId,
  teamId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  teamProductId: string | null
  teamYearlyProductId?: string | null
  // Checkout binds purchased seats to this team (the per-seat path —
  // billing.createSeatCheckout is the only checkout).
  teamId: string
}) {
  // An already-subscribed team hitting a limit must switch plans on its
  // EXISTING subscription, not run a second checkout (which would stack a
  // second full-price subscription) — resolve the real plan + subscription.
  const billingPlan = useBillingPlan(open ? teamId : undefined)
  const [seatDialogOpen, setSeatDialogOpen] = useState(false)
  const subscription = billingPlan?.subscription ?? null
  const canAdjustSeats = Boolean(
    subscription && !subscription.cancelAtPeriodEnd
  )

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* `sm:` prefix required — the base DialogContent class has `sm:max-w-lg`
          and tailwind-merge only dedupes same-variant classes, so an unprefixed
          max-w-* silently loses on desktop. */}
      <DialogContent
        mobile="sheet-full"
        className="sm:max-w-[min(64rem,calc(100vw-2rem))]"
      >
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="size-4" />
            {title}
          </DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        <DialogBody className="space-y-4">
          {canAdjustSeats && subscription && (
            <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
              <p className="text-sm text-muted-foreground">
                Your plan has {subscription.seats} seat
                {subscription.seats === 1 ? `` : `s`}. Add more without
                switching plans.
              </p>
              <Button
                size="sm"
                variant="outline"
                className="shrink-0"
                onClick={() => setSeatDialogOpen(true)}
              >
                <Users className="mr-1.5 size-3.5" />
                Adjust seats
              </Button>
            </div>
          )}

          <PlanComparison
            currentPlan={billingPlan?.plan ?? `free`}
            teamProductId={teamProductId}
            teamYearlyProductId={teamYearlyProductId}
            teamId={teamId}
            subscription={subscription}
          />
        </DialogBody>

        {subscription && (
          <AdjustSeatsDialog
            teamId={teamId}
            currentSeats={subscription.seats}
            memberCount={billingPlan?.usage.members ?? 0}
            periodEnd={subscription.periodEnd}
            open={seatDialogOpen}
            onOpenChange={setSeatDialogOpen}
          />
        )}
      </DialogContent>
    </Dialog>
  )
}
