import { useState } from "react"
import { Check, X, CreditCard, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import type { PlanTier } from "@/lib/billing"
import { cn } from "@/lib/utils"

// The real paid differentiators: members, projects, repositories, storage,
// concurrent coding sessions. Push + email notifications are free on every
// tier and deliberately NOT a row here — delivery is never paywalled.
type TierInfo = {
  tier: PlanTier
  name: string
  price: string
  period: string
  members: string
  projects: string
  repositories: string
  storage: string
  codingSessions: string
}

const TIERS: TierInfo[] = [
  {
    tier: `free`,
    name: `Free`,
    price: `$0`,
    period: `forever`,
    members: `1 member`,
    projects: `3 projects`,
    repositories: `1 repository`,
    storage: `50 MB`,
    codingSessions: `1 concurrent coding session`,
  },
  {
    tier: `pro`,
    name: `Pro`,
    price: `$18`,
    period: `/year`,
    members: `5 members`,
    projects: `10 projects`,
    repositories: `10 repositories`,
    storage: `1 GB`,
    codingSessions: `3 concurrent coding sessions`,
  },
  {
    tier: `business`,
    name: `Business`,
    price: `$60`,
    period: `/year`,
    members: `25 members`,
    projects: `Unlimited projects`,
    repositories: `Unlimited repositories`,
    storage: `10 GB`,
    codingSessions: `Unlimited coding sessions`,
  },
]

function FeatureRow({
  label,
  enabled,
}: {
  label: string
  enabled: boolean
}) {
  return (
    <div className="flex items-center gap-2 text-sm">
      {enabled ? (
        <Check className="size-3.5 shrink-0 text-emerald-500" />
      ) : (
        <X className="size-3.5 shrink-0 text-muted-foreground/50" />
      )}
      <span className={cn(!enabled && `text-muted-foreground/50`)}>
        {label}
      </span>
    </div>
  )
}

export function PlanComparison({
  currentPlan,
  proProductId,
  businessProductId,
  onCheckout,
}: {
  currentPlan: PlanTier
  proProductId: string | null
  businessProductId: string | null
  onCheckout?: (productId: string) => Promise<void>
}) {
  const [loading, setLoading] = useState<string | null>(null)

  const handleCheckout = async (productId: string) => {
    if (onCheckout) {
      setLoading(productId)
      try {
        await onCheckout(productId)
      } finally {
        setLoading(null)
      }
      return
    }
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

  const getProductId = (tier: PlanTier): string | null => {
    if (tier === `pro`) return proProductId
    if (tier === `business`) return businessProductId
    return null
  }

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
        {TIERS.map((t) => {
          const isCurrent = t.tier === currentPlan
          const productId = getProductId(t.tier)
          const canUpgrade =
            !isCurrent && t.tier !== `free` && productId !== null

          return (
            <Card
              key={t.tier}
              className={cn(
                `relative`,
                isCurrent && `ring-2 ring-primary`
              )}
            >
              {isCurrent && (
                <Badge className="absolute -top-2.5 left-4" variant="default">
                  Current
                </Badge>
              )}
              <CardHeader className="pb-3">
                <CardTitle className="text-base">{t.name}</CardTitle>
                <div className="flex items-baseline gap-1">
                  <span className="text-2xl font-bold">{t.price}</span>
                  <span className="text-sm text-muted-foreground">
                    {t.period}
                  </span>
                </div>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="space-y-2">
                  <FeatureRow label={t.members} enabled />
                  <FeatureRow label={t.projects} enabled />
                  <FeatureRow label={t.repositories} enabled />
                  <FeatureRow label={`${t.storage} storage`} enabled />
                  <FeatureRow label={t.codingSessions} enabled />
                </div>

                {canUpgrade && (
                  <Button
                    className="w-full"
                    size="sm"
                    onClick={() => handleCheckout(productId!)}
                    disabled={loading !== null}
                  >
                    {loading === productId ? (
                      <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                    ) : (
                      <CreditCard className="mr-1.5 size-3.5" />
                    )}
                    {loading === productId ? `Loading...` : `Upgrade`}
                  </Button>
                )}

                {isCurrent && t.tier !== `free` && (
                  <p className="text-center text-xs text-muted-foreground">
                    Your current plan
                  </p>
                )}
              </CardContent>
            </Card>
          )
        })}
      </div>

      {currentPlan === `free` && (
        <p className="text-center text-xs text-muted-foreground">
          Use code{` `}
          <span className="font-mono font-medium">FOUNDING</span>
          {` `}at checkout for 50% off forever.
        </p>
      )}
    </div>
  )
}
