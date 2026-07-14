import { useId, useState } from "react"
import {
  ArrowRightLeft,
  Check,
  CreditCard,
  Loader2,
  Mail,
  Minus,
  Plus,
  X,
} from "lucide-react"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { trpc } from "@/lib/trpc-client"
import type { PlanTier } from "@/lib/billing"
import {
  invalidateBillingCache,
  type BillingSubscription,
} from "@/hooks/use-billing"
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
  // Unit caption next to the price.
  priceUnit: string
  // Billing cadence caption under the price.
  cadence: string
  features: Feature[]
}

function commonFeatures(storage: string, widget: Feature): Feature[] {
  return [
    { label: `Unlimited projects & repos`, enabled: true },
    { label: `Unlimited coding sessions`, enabled: true },
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
    priceUnit: `forever`,
    cadence: `1 seat`,
    features: [
      ...commonFeatures(`250 MB storage`, {
        label: `Feedback widget`,
        enabled: false,
      }),
    ],
  },
  {
    tier: `pro`,
    name: `Pro`,
    pricePerSeat: `$5`,
    priceUnit: `/seat/mo`,
    cadence: `Billed yearly`,
    features: [
      ...commonFeatures(`5 GB storage`, {
        label: `1 feedback widget`,
        enabled: true,
      }),
    ],
  },
  {
    tier: `business`,
    name: `Business`,
    pricePerSeat: `$10`,
    priceUnit: `/seat/mo`,
    cadence: `Billed monthly or yearly`,
    features: [
      ...commonFeatures(`50 GB storage`, {
        label: `Unlimited feedback widgets`,
        enabled: true,
      }),
      { label: `Priority support`, enabled: true },
      { label: `SSO / OIDC (coming soon)`, enabled: false },
    ],
  },
]

// Enterprise is contact-sales only — no Creem product, no checkout. It is
// deliberately NOT a TIERS entry: TierInfo.tier is a PlanTier, and the
// server-side PlanTier union must not grow a display-only member.
const ENTERPRISE_FEATURES: Feature[] = [
  { label: `Everything in Business`, enabled: true },
  { label: `SLA & dedicated support`, enabled: true },
  { label: `Custom contracts & procurement`, enabled: true },
  { label: `Security review & DPA`, enabled: true },
]

const CONTACT_SALES_URL = `https://exponential.at/contact/`

function FeatureRow({ label, enabled }: Feature) {
  return (
    <div className="flex items-start gap-2 text-[13px] leading-snug">
      {enabled ? (
        <Check className="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
      ) : (
        <X className="mt-0.5 size-3.5 shrink-0 text-muted-foreground/50" />
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
    <div className="flex items-center justify-between gap-2">
      <Label className="text-xs text-muted-foreground">Seats</Label>
      <div className="flex items-center gap-1">
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="size-7 shrink-0"
          onClick={() => onChange(Math.max(1, seats - 1))}
          disabled={seats <= 1}
          aria-label="Remove seat"
        >
          <Minus className="size-3" />
        </Button>
        <Input
          type="text"
          inputMode="numeric"
          value={seats}
          onChange={(e) => {
            const n = Number.parseInt(e.target.value.replace(/\D/g, ``), 10)
            onChange(Number.isFinite(n) && n >= 1 ? n : 1)
          }}
          className="h-7 w-11 px-1 text-center"
          aria-label="Seat count"
        />
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="size-7 shrink-0"
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
  subscription,
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
  // The workspace's active subscription, when it has one. With a subscription
  // present, plan changes go through billing.changePlan (mutating the existing
  // subscription) — a second checkout would stack a second full-price
  // subscription on top of the first (pay-twice bug).
  subscription?: BillingSubscription | null
  onCheckout?: (productId: string, seats: number) => Promise<void>
}) {
  const [loading, setLoading] = useState<string | null>(null)
  const [seats, setSeats] = useState(1)
  const [businessYearly, setBusinessYearly] = useState(true)
  const yearlyToggleId = useId()

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

  const handleSwitchPlan = async (tier: PlanTier) => {
    const productId = getProductId(tier)
    if (!productId) return
    setLoading(tier)
    try {
      await trpc.billing.changePlan.mutate({ workspaceId, productId })
      invalidateBillingCache()
      toast.success(`Plan updated`)
    } catch (err) {
      console.error(`[billing] plan change failed:`, err)
      toast.error(
        err instanceof Error && err.message
          ? err.message
          : `Couldn't change the plan`
      )
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
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
      {TIERS.map((t) => {
        const isCurrent = t.tier === currentPlan
        const productId = getProductId(t.tier)
        // With an active subscription, "current" means the exact product —
        // so a Business-monthly workspace can still switch to Business-yearly.
        const isCurrentProduct = subscription
          ? productId === subscription.productId
          : isCurrent
        const canSwitch =
          Boolean(subscription) &&
          !subscription?.cancelAtPeriodEnd &&
          t.tier !== `free` &&
          productId !== null &&
          !isCurrentProduct
        const canUpgrade =
          !subscription && !isCurrent && t.tier !== `free` && productId !== null
        const showYearlyToggle =
          t.tier === `business` && Boolean(businessYearlyProductId)

        return (
          <Card
            key={t.tier}
            className={cn(
              `flex h-full flex-col gap-4 py-4`,
              isCurrent && `border-primary/40`
            )}
          >
            <CardHeader className="gap-1.5 px-4">
              <div className="flex items-center justify-between gap-2">
                <CardTitle className="text-sm">{t.name}</CardTitle>
                {isCurrent && (
                  <Badge variant="secondary" className="text-[10px]">
                    Current
                  </Badge>
                )}
              </div>
              <div className="flex items-baseline gap-1">
                <span className="text-2xl font-semibold tracking-tight">
                  {t.pricePerSeat}
                </span>
                <span className="text-xs text-muted-foreground">
                  {t.priceUnit}
                </span>
              </div>
              <p className="text-xs text-muted-foreground">{t.cadence}</p>
            </CardHeader>
            <CardContent className="flex flex-1 flex-col gap-4 px-4">
              <div className="space-y-2">
                {t.features.map((f) => (
                  <FeatureRow key={f.label} label={f.label} enabled={f.enabled} />
                ))}
              </div>

              {(showYearlyToggle || canUpgrade || canSwitch) && (
                <div className="mt-auto space-y-2.5 border-t pt-3">
                  {showYearlyToggle && (
                    <div className="flex items-center justify-between gap-2">
                      <Label
                        htmlFor={yearlyToggleId}
                        className="text-xs text-muted-foreground"
                      >
                        Bill yearly
                      </Label>
                      <Switch
                        id={yearlyToggleId}
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

                  {canSwitch && (
                    <>
                      <Button
                        className="w-full"
                        size="sm"
                        onClick={() => handleSwitchPlan(t.tier)}
                        disabled={loading !== null}
                      >
                        {loading === t.tier ? (
                          <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                        ) : (
                          <ArrowRightLeft className="mr-1.5 size-3.5" />
                        )}
                        {loading === t.tier ? `Switching...` : `Switch plan`}
                      </Button>
                      <p className="text-center text-[11px] leading-snug text-muted-foreground">
                        Applies now · billed at your next renewal
                      </p>
                    </>
                  )}
                </div>
              )}

              {isCurrentProduct && t.tier !== `free` && (
                <p className="mt-auto text-center text-xs text-muted-foreground">
                  Your current plan
                </p>
              )}
            </CardContent>
          </Card>
        )
      })}

      <Card className="flex h-full flex-col gap-4 py-4">
        <CardHeader className="gap-1.5 px-4">
          <CardTitle className="text-sm">Enterprise</CardTitle>
          <div className="flex items-baseline gap-1">
            <span className="text-2xl font-semibold tracking-tight">
              Let&apos;s talk
            </span>
          </div>
          <p className="text-xs text-muted-foreground">Custom pricing & SLA</p>
        </CardHeader>
        <CardContent className="flex flex-1 flex-col gap-4 px-4">
          <div className="space-y-2">
            {ENTERPRISE_FEATURES.map((f) => (
              <FeatureRow key={f.label} label={f.label} enabled={f.enabled} />
            ))}
          </div>
          <div className="mt-auto space-y-2.5 border-t pt-3">
            <Button asChild variant="outline" className="w-full" size="sm">
              <a href={CONTACT_SALES_URL} target="_blank" rel="noreferrer">
                <Mail className="mr-1.5 size-3.5" />
                Contact sales
              </a>
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
