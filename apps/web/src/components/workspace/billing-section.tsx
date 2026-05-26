import { useState } from "react"
import { CreditCard, ExternalLink, Sparkles } from "lucide-react"
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

function UsageBar({
  label,
  current,
  max,
}: {
  label: string
  current: number
  max: number
}) {
  const percent = max === Infinity ? 0 : Math.round((current / max) * 100)
  const display =
    max === Infinity ? `${current} / unlimited` : `${current} / ${max}`

  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between text-sm">
        <span className="text-muted-foreground">{label}</span>
        <span className="font-medium">{display}</span>
      </div>
      {max !== Infinity && <Progress value={percent} className="h-2" />}
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
  const [loading, setLoading] = useState<string | null>(null)

  if (!billingPlan || billingPlan.plan === `unlimited`) return null

  const { plan, limits, usage } = billingPlan

  const handleCheckout = async (productId: string) => {
    setLoading(productId)
    try {
      const res = await fetch(`/api/auth/creem/create-checkout`, {
        method: `POST`,
        headers: { "Content-Type": `application/json` },
        body: JSON.stringify({
          productId,
          successUrl: window.location.href,
        }),
      })
      const data = await res.json()
      if (data?.url) window.location.href = data.url
    } catch (err) {
      console.error(`[billing] checkout failed:`, err)
    } finally {
      setLoading(null)
    }
  }

  const handlePortal = async () => {
    setLoading(`portal`)
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
      setLoading(null)
    }
  }

  const isPaid = plan === `pro` || plan === `business`

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <div>
            <CardTitle className="text-base">Plan & Billing</CardTitle>
            <CardDescription>
              Manage your workspace subscription
            </CardDescription>
          </div>
          <Badge variant={PLAN_BADGE_VARIANT[plan]}>
            {PLAN_LABELS[plan]}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-5">
        <div className="space-y-3">
          <UsageBar label="Members" current={usage.members} max={limits.members} />
          <UsageBar
            label="Projects"
            current={usage.projects}
            max={limits.projects}
          />
        </div>

        {plan === `free` && (
          <div className="rounded-md border border-dashed p-4 space-y-3">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Sparkles className="size-4" />
              Upgrade your workspace
            </div>
            <p className="text-xs text-muted-foreground">
              Unlock more members, projects, and push notifications.
              Use code <span className="font-mono font-medium">FOUNDING</span> for
              50% off forever.
            </p>
            <div className="flex gap-2">
              {proProductId && (
                <Button
                  size="sm"
                  onClick={() => handleCheckout(proProductId)}
                  disabled={loading !== null}
                >
                  <CreditCard className="mr-1.5 size-3.5" />
                  {loading === proProductId ? `Loading...` : `Pro — $18/yr`}
                </Button>
              )}
              {businessProductId && (
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => handleCheckout(businessProductId)}
                  disabled={loading !== null}
                >
                  {loading === businessProductId
                    ? `Loading...`
                    : `Business — $60/yr`}
                </Button>
              )}
            </div>
          </div>
        )}

        {isPaid && (
          <Button
            variant="outline"
            size="sm"
            onClick={handlePortal}
            disabled={loading !== null}
          >
            <ExternalLink className="mr-1.5 size-3.5" />
            {loading === `portal` ? `Loading...` : `Manage subscription`}
          </Button>
        )}
      </CardContent>
    </Card>
  )
}
