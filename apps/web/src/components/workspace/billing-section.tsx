import { useState } from "react"
import { ChevronDown, ChevronUp, ExternalLink } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Progress } from "@/components/ui/progress"
import { useBillingPlan } from "@/hooks/use-billing"
import type { PlanTier } from "@/lib/billing"
import { PlanComparison } from "@/components/workspace/plan-comparison"

const PLAN_LABELS: Record<PlanTier, string> = {
  free: `Free`,
  pro: `Pro`,
  business: `Business`,
  unlimited: `Unlimited`,
}

const PLAN_BADGE_VARIANT: Record<
  PlanTier,
  `default` | `secondary` | `outline`
> = {
  free: `secondary`,
  pro: `default`,
  business: `default`,
  unlimited: `outline`,
}

function formatStorage(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1).replace(/\.0$/, ``)} GB`
  return `${Math.round(mb)} MB`
}

function UsageBar({
  label,
  current,
  max,
  formatValue,
}: {
  label: string
  current: number
  max: number
  formatValue?: (n: number) => string
}) {
  const percent = max === Infinity ? 0 : Math.round((current / max) * 100)
  const fmt = formatValue ?? ((n: number) => String(n))
  const display =
    max === Infinity ? `${fmt(current)} / unlimited` : `${fmt(current)} / ${fmt(max)}`

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-medium">{display}</span>
      </div>
      {max !== Infinity && <Progress value={Math.min(percent, 100)} className="h-2" />}
    </div>
  )
}

export function WorkspaceBillingSection({
  workspaceId,
  proProductId,
  businessProductId,
}: {
  workspaceId: string
  proProductId: string | null
  businessProductId: string | null
}) {
  const billingPlan = useBillingPlan(workspaceId)
  const [portalLoading, setPortalLoading] = useState(false)
  const [showPlans, setShowPlans] = useState(false)

  if (!billingPlan || billingPlan.plan === `unlimited`) return null

  const { plan, limits, usage } = billingPlan
  const isPaid = plan === `pro` || plan === `business`

  const handlePortal = async () => {
    setPortalLoading(true)
    try {
      const res = await fetch(`/api/auth/creem/create-portal`, {
        method: `POST`,
        headers: { "Content-Type": `application/json` },
        body: JSON.stringify({}),
      })
      const data = await res.json()
      if (data?.url) window.location.href = data.url
    } catch (err) {
      console.error(`[billing] portal failed:`, err)
    } finally {
      setPortalLoading(false)
    }
  }

  return (
    // `billing` anchors the upgrade-nudge deep links (e.g. from the
    // repositories section when a plan cap is hit).
    <div id="billing" className="scroll-mt-6 space-y-4">
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="text-base">Plan & Billing</CardTitle>
              <CardDescription>
                Manage your workspace subscription
              </CardDescription>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant={PLAN_BADGE_VARIANT[plan]}>
                {PLAN_LABELS[plan]}
              </Badge>
              {isPaid && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handlePortal}
                  disabled={portalLoading}
                >
                  <ExternalLink className="mr-1.5 size-3.5" />
                  {portalLoading ? `Loading...` : `Manage`}
                </Button>
              )}
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <UsageBar
            label="Members"
            current={usage.members}
            max={limits.members}
          />
          <UsageBar
            label="Projects"
            current={usage.projects}
            max={limits.projects}
          />
          <UsageBar
            label="Repositories"
            current={usage.repositories}
            max={limits.repositories}
          />
          <UsageBar
            label="Storage"
            current={usage.storageMb}
            max={limits.storageMb}
            formatValue={formatStorage}
          />
        </CardContent>
      </Card>

      {plan === `free` ? (
        <PlanComparison
          currentPlan={plan}
          proProductId={proProductId}
          businessProductId={businessProductId}
        />
      ) : (
        <div>
          <Button
            variant="ghost"
            size="sm"
            className="w-full text-muted-foreground"
            onClick={() => setShowPlans(!showPlans)}
          >
            {showPlans ? (
              <ChevronUp className="mr-1.5 size-3.5" />
            ) : (
              <ChevronDown className="mr-1.5 size-3.5" />
            )}
            {showPlans ? `Hide plans` : `Compare plans`}
          </Button>
          {showPlans && (
            <div className="mt-3">
              <PlanComparison
                currentPlan={plan}
                proProductId={proProductId}
                businessProductId={businessProductId}
              />
            </div>
          )}
        </div>
      )}
    </div>
  )
}
