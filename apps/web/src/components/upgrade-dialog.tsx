import { Sparkles } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { PlanComparison } from "@/components/workspace/plan-comparison"

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
        <PlanComparison
          currentPlan="free"
          proProductId={proProductId}
          businessProductId={businessProductId}
          businessYearlyProductId={businessYearlyProductId}
          workspaceId={workspaceId}
        />
      </DialogContent>
    </Dialog>
  )
}
