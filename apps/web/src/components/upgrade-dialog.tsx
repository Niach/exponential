import { useState } from "react"
import { Sparkles, Users } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { PlanComparison } from "@/components/workspace/plan-comparison"
import { AdjustSeatsDialog } from "@/components/workspace/adjust-seats-dialog"
import { useBillingPlan } from "@/hooks/use-billing"

export function UpgradeDialog({
  open,
  onOpenChange,
  title,
  description,
  proProductId,
  businessProductId,
  businessYearlyProductId,
  workspaceId,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  proProductId: string | null
  businessProductId: string | null
  businessYearlyProductId?: string | null
  // Checkout binds purchased seats to this workspace (the per-seat path —
  // billing.createSeatCheckout is the only checkout).
  workspaceId: string
}) {
  // An already-subscribed workspace hitting a limit must switch plans on its
  // EXISTING subscription, not run a second checkout (which would stack a
  // second full-price subscription) — resolve the real plan + subscription.
  const billingPlan = useBillingPlan(open ? workspaceId : undefined)
  const [seatDialogOpen, setSeatDialogOpen] = useState(false)
  const subscription = billingPlan?.subscription ?? null
  const canAdjustSeats = Boolean(subscription && !subscription.cancelAtPeriodEnd)

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {/* `sm:` prefix required — the base DialogContent class has `sm:max-w-lg`
          and tailwind-merge only dedupes same-variant classes, so an unprefixed
          max-w-* silently loses on desktop. */}
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles className="size-4" />
            {title}
          </DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {canAdjustSeats && subscription && (
          <div className="flex items-center justify-between gap-3 rounded-md border px-3 py-2.5">
            <p className="text-sm text-muted-foreground">
              Your plan has {subscription.seats} seat
              {subscription.seats === 1 ? `` : `s`} — add more without
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
          proProductId={proProductId}
          businessProductId={businessProductId}
          businessYearlyProductId={businessYearlyProductId}
          workspaceId={workspaceId}
          subscription={subscription}
        />

        {subscription && (
          <AdjustSeatsDialog
            workspaceId={workspaceId}
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
