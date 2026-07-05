import { useState } from "react"
import { Check, X, CreditCard, Loader2, Minus, Plus } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { trpc } from "@/lib/trpc-client"
import type { PlanTier } from "@/lib/billing"
import { cn } from "@/lib/utils"

// Per-seat model (masterplan v5 §3.2). Projects, repositories and coding
// sessions are UNLIMITED on every tier — that is stated explicitly as a row so
// buyers see it. Push + email notifications and remote steer are free on every
// tier and are deliberately NOT a paywalled row. The monetized axes are seats
// (team size), storage per workspace, and the feedback widget.
type Feature = { label: string; enabled: boolean }

type TierInfo = {
  tier: PlanTier
  name: string
  // Per-seat monthly price, shown as the big number.
  pricePerSeat: string
  // Billing cadence caption under the price.
  cadence: string
  features: Feature[]
}

function commonFeatures(storage: string, widget: Feature): Feature[] {
  return [
    { label: `Unlimited projects, repos & coding sessions`, enabled: true },
    { label: storage, enabled: true },
    widget,
    { label: `Push, email & remote steer`, enabled: true },
  ]
}

const TIERS: TierInfo[] = [
  {
    tier: `free`,
    name: `Free`,
    pricePerSeat: `$0`,
    cadence: `forever · 1 seat`,
    features: [
      ...commonFeatures(`250 MB storage per workspace`, {
        label: `Feedback widget`,
        enabled: false,
      }),
    ],
  },
  {
    tier: `pro`,
    name: `Pro`,
    pricePerSeat: `$5`,
    cadence: `per seat / month · billed yearly`,
    features: [
      ...commonFeatures(`5 GB storage per workspace`, {
        label: `1 feedback widget config`,
        enabled: true,
      }),
    ],
  },
  {
    tier: `business`,
    name: `Business`,
    pricePerSeat: `$10`,
    cadence: `per seat / month · monthly or yearly`,
    features: [
      ...commonFeatures(`50 GB storage per workspace`, {
        label: `Unlimited feedback widgets`,
        enabled: true,
      }),
      { label: `Priority support`, enabled: true },
      { label: `SSO / OIDC (coming soon)`, enabled: false },
    ],
  },
]

function FeatureRow({ label, enabled }: Feature) {
  return (
    <div className="flex items-center gap-2 text-sm">
      {enabled ? (
        <Check className="size-3.5 shrink-0 text-emerald-500" />
      ) : (
        <X className="size-3.5 shrink-0 text-muted-foreground/50" />
      )}
      <span className={cn(!enabled && `text-muted-foreground/50`)}>{label}</span>
    </div>
  )
}

function SeatStepper({
  seats,
  onChange,
}: {
  seats: number
  onChange: (n: number) => void
}) {
  return (
    <div className="flex items-center gap-2">
      <Label className="text-xs text-muted-foreground">Seats</Label>
      <div className="flex items-center gap-1">
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="size-7"
          onClick={() => onChange(Math.max(1, seats - 1))}
          disabled={seats <= 1}
          aria-label="Remove seat"
        >
          <Minus className="size-3" />
        </Button>
        <Input
          type="number"
          min={1}
          value={seats}
          onChange={(e) => {
            const n = Math.floor(Number(e.target.value))
            onChange(Number.isFinite(n) && n >= 1 ? n : 1)
          }}
          className="h-7 w-14 text-center"
          aria-label="Seat count"
        />
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="size-7"
          onClick={() => onChange(seats + 1)}
          aria-label="Add seat"
        >
          <Plus className="size-3" />
        </Button>
      </div>
    </div>
  )
}

export function PlanComparison({
  currentPlan,
  proProductId,
  businessProductId,
  businessYearlyProductId,
  workspaceId,
  onCheckout,
}: {
  currentPlan: PlanTier
  proProductId: string | null
  // Business monthly product id.
  businessProductId: string | null
  // Business yearly product id — when present, Business shows a monthly/yearly
  // toggle. When absent, Business bills monthly only.
  businessYearlyProductId?: string | null
  // Checkout binds seats to this workspace via billing.createSeatCheckout —
  // the ONLY checkout path (no legacy unbound fallback).
  workspaceId: string
  onCheckout?: (productId: string, seats: number) => Promise<void>
}) {
  const [loading, setLoading] = useState<string | null>(null)
  const [seats, setSeats] = useState(1)
  const [businessYearly, setBusinessYearly] = useState(true)

  const startCheckout = async (productId: string, quantity: number) => {
    if (onCheckout) {
      await onCheckout(productId, quantity)
      return
    }
    const { url } = await trpc.billing.createSeatCheckout.mutate({
      workspaceId,
      productId,
      seats: quantity,
      successUrl: window.location.href,
    })
    if (url) window.location.href = url
  }

  const handleCheckout = async (tier: PlanTier) => {
    const productId = getProductId(tier)
    if (!productId) return
    setLoading(tier)
    try {
      await startCheckout(productId, seats)
    } catch (err) {
      console.error(`[billing] checkout failed:`, err)
    } finally {
      setLoading(null)
    }
  }

  const getProductId = (tier: PlanTier): string | null => {
    if (tier === `pro`) return proProductId
    if (tier === `business`) {
      if (businessYearlyProductId && businessYearly) return businessYearlyProductId
      return businessProductId
    }
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
          const showYearlyToggle =
            t.tier === `business` && Boolean(businessYearlyProductId)

          return (
            <Card
              key={t.tier}
              className={cn(`relative`, isCurrent && `ring-2 ring-primary`)}
            >
              {isCurrent && (
                <Badge className="absolute -top-2.5 left-4" variant="default">
                  Current
                </Badge>
              )}
              <CardHeader className="pb-3">
                <CardTitle className="text-base">{t.name}</CardTitle>
                <div className="flex items-baseline gap-1">
                  <span className="text-2xl font-bold">{t.pricePerSeat}</span>
                  {t.tier !== `free` && (
                    <span className="text-sm text-muted-foreground">/seat</span>
                  )}
                </div>
                <p className="text-xs text-muted-foreground">{t.cadence}</p>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="space-y-2">
                  {t.features.map((f) => (
                    <FeatureRow
                      key={f.label}
                      label={f.label}
                      enabled={f.enabled}
                    />
                  ))}
                </div>

                {showYearlyToggle && (
                  <div className="flex items-center justify-between">
                    <Label
                      htmlFor="business-yearly"
                      className="text-xs text-muted-foreground"
                    >
                      Bill yearly
                    </Label>
                    <Switch
                      id="business-yearly"
                      checked={businessYearly}
                      onCheckedChange={setBusinessYearly}
                    />
                  </div>
                )}

                {canUpgrade && (
                  <>
                    <SeatStepper seats={seats} onChange={setSeats} />
                    <Button
                      className="w-full"
                      size="sm"
                      onClick={() => handleCheckout(t.tier)}
                      disabled={loading !== null}
                    >
                      {loading === t.tier ? (
                        <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                      ) : (
                        <CreditCard className="mr-1.5 size-3.5" />
                      )}
                      {loading === t.tier
                        ? `Loading...`
                        : `Upgrade${seats > 1 ? ` · ${seats} seats` : ``}`}
                    </Button>
                  </>
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
    </div>
  )
}
