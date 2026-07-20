import { useId, useState } from "react"
import {
  ArrowRightLeft,
  Check,
  CreditCard,
  Loader2,
  Minus,
  Plus,
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

// Per-seat model (masterplan v5 §3.2). The cards list ONLY the monetized
// axes — seats (team size), storage per team, the feedback widget, the
// helpdesk, priority support (EXP-176 unified this across the marketing
// frontpage, /pricing and this grid — canonical copy lives in
// apps/marketing/src/lib/plans.ts; keep the bullets in sync). Everything
// never-gated (unlimited boards/repos/coding sessions, native apps,
// real-time sync, push/email/steer) lives in the ONE shared
// EVERY_PLAN_INCLUDES sentence under the grid.
const EVERY_PLAN_INCLUDES = `Every plan includes unlimited boards, repos and coding sessions, all native apps, real-time sync, and push, email & remote steer.`

// Display-only union: Enterprise is a "Contact sales" card, not a PlanTier —
// it never reaches checkout/seat logic (getProductId returns null for it).
type ComparisonTier = PlanTier | `enterprise`

type TierInfo = {
  tier: ComparisonTier
  name: string
  // Per-seat monthly price, shown as the big number.
  pricePerSeat: string
  // Unit caption next to the price.
  priceUnit: string
  // Billing cadence caption under the price.
  cadence: string
  features: string[]
}

const TIERS: TierInfo[] = [
  {
    tier: `free`,
    name: `Free`,
    pricePerSeat: `$0`,
    priceUnit: `forever`,
    // The seat cap doubles as the cadence caption, so it isn't a bullet.
    cadence: `1 seat`,
    features: [`250 MB attachment storage`, `1 feedback widget`],
  },
  {
    tier: `pro`,
    name: `Pro`,
    pricePerSeat: `$5`,
    priceUnit: `/seat/mo`,
    cadence: `Billed yearly`,
    features: [
      `Everything in Free`,
      `2 GB attachment storage`,
      `3 feedback widgets`,
      `Helpdesk & support inbox`,
    ],
  },
  {
    tier: `business`,
    name: `Business`,
    pricePerSeat: `$10`,
    priceUnit: `/seat/mo`,
    cadence: `Billed monthly or yearly`,
    features: [
      `Everything in Pro`,
      `10 GB attachment storage`,
      `Unlimited feedback widgets`,
      `Priority support`,
    ],
  },
  {
    tier: `enterprise`,
    name: `Enterprise`,
    pricePerSeat: `Custom`,
    priceUnit: ``,
    cadence: `For larger teams`,
    features: [
      `Everything in Business`,
      `SSO / OIDC (coming soon)`,
      `SLA & DPA`,
      `Dedicated support channel`,
      `Onboarding & migration help`,
    ],
  },
]

function FeatureRow({ label }: { label: string }) {
  return (
    <div className="flex items-start gap-2 text-[13px] leading-snug">
      <Check className="mt-0.5 size-3.5 shrink-0 text-emerald-500" />
      <span>{label}</span>
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
    <div className="flex flex-wrap items-center justify-between gap-2">
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
  teamId,
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
  // Checkout binds seats to this team via billing.createSeatCheckout —
  // the ONLY checkout path (no legacy unbound fallback).
  teamId: string
  // The team's active subscription, when it has one. With a subscription
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
      teamId,
      productId,
      seats: quantity,
      successUrl: window.location.href,
    })
    if (url) window.location.href = url
  }

  const handleCheckout = async (tier: ComparisonTier) => {
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

  const handleSwitchPlan = async (tier: ComparisonTier) => {
    const productId = getProductId(tier)
    if (!productId) return
    setLoading(tier)
    try {
      await trpc.billing.changePlan.mutate({ teamId, productId })
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

  const getProductId = (tier: ComparisonTier): string | null => {
    if (tier === `pro`) return proProductId
    if (tier === `business`) {
      if (businessYearlyProductId && businessYearly)
        return businessYearlyProductId
      return businessProductId
    }
    // `free` has no product; `enterprise` is display-only (Contact us).
    return null
  }

  // Container-query columns (EXP-184): this grid renders inside fixed-width
  // containers (settings content ~640px, upgrade dialog) where viewport
  // breakpoints overflowed the cards — column count must follow the CONTAINER.
  // @4xl (56rem) is the narrowest that fits four seat-stepper cards.
  return (
    <div className="space-y-3">
      <div className="@container grid grid-cols-1 gap-3 @xl:grid-cols-2 @4xl:grid-cols-4">
        {TIERS.map((t) => {
          const isCurrent = t.tier === currentPlan
          const productId = getProductId(t.tier)
          // With an active subscription, "current" means the exact product —
          // so a Business-monthly team can still switch to Business-yearly.
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
            !subscription &&
            !isCurrent &&
            t.tier !== `free` &&
            productId !== null
          const showYearlyToggle =
            t.tier === `business` && Boolean(businessYearlyProductId)

          return (
            <Card
              key={t.tier}
              className={cn(
                `flex h-full min-w-0 flex-col gap-4 overflow-hidden py-4`,
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
                    <FeatureRow key={f} label={f} />
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
                          className="w-full min-w-0"
                          size="sm"
                          onClick={() => handleCheckout(t.tier)}
                          disabled={loading !== null}
                        >
                          {loading === t.tier ? (
                            <Loader2 className="mr-1.5 size-3.5 animate-spin" />
                          ) : (
                            <CreditCard className="mr-1.5 size-3.5" />
                          )}
                          <span className="truncate">
                            {loading === t.tier
                              ? `Loading...`
                              : `Upgrade${seats > 1 ? ` · ${seats} seats` : ``}`}
                          </span>
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

                {t.tier === `enterprise` && (
                  <div className="mt-auto border-t pt-3">
                    <Button
                      className="w-full"
                      size="sm"
                      variant="outline"
                      asChild
                    >
                      <a
                        href="https://exponential.at/contact/"
                        target="_blank"
                        rel="noreferrer"
                      >
                        Contact sales
                      </a>
                    </Button>
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
      </div>
      <p className="text-xs leading-snug text-muted-foreground">
        {EVERY_PLAN_INCLUDES}
      </p>
    </div>
  )
}
