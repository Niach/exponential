import { useEffect, useState } from "react"
import { CreditCard } from "lucide-react"
import { getRuntimeConfig } from "@/lib/runtime-config"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { PlanComparison } from "@/components/workspace/plan-comparison"
import type { StepProps } from "./wizard"

export function StepPlan({ onNext, onSkip }: StepProps) {
  const [productIds, setProductIds] = useState<{
    pro: string | null
    business: string | null
  }>({ pro: null, business: null })

  useEffect(() => {
    void getRuntimeConfig().then((config) => {
      setProductIds({
        pro: config.creemProProductId,
        business: config.creemBusinessProductId,
      })
    })
  }, [])

  return (
    <Card>
      <CardHeader className="text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-primary/10">
          <CreditCard className="size-6 text-primary" />
        </div>
        <CardTitle className="text-xl">Choose your plan</CardTitle>
        <CardDescription>
          Start free and upgrade anytime. All plans include AI agents.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <PlanComparison
          currentPlan="free"
          proProductId={productIds.pro}
          businessProductId={productIds.business}
        />
        <div className="flex justify-between">
          <Button variant="ghost" onClick={onSkip}>
            Skip
          </Button>
          <Button onClick={onNext}>
            Continue with Free
          </Button>
        </div>
      </CardContent>
    </Card>
  )
}
