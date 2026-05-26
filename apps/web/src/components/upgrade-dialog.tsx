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
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  title: string
  description: string
  proProductId: string | null
  businessProductId: string | null
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
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
        />
      </DialogContent>
    </Dialog>
  )
}
