import { useState } from "react"
import {
  ChevronDown,
  ChevronUp,
  ExternalLink,
  AlertTriangle,
  Users,
} from "lucide-react"
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
import { PlanComparison } from "@/components/team/plan-comparison"
import { AdjustSeatsDialog } from "@/components/team/adjust-seats-dialog"

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

// Scroll the plan-comparison grid (`#plans` below) into view. A JS scroll
// rather than an `<a href="#plans">` so repeat clicks keep working — hash
// navigation is a no-op once the hash is already set. Also the target of the
// repositories section's upgrade nudge (EXP-35).
export function scrollToPlans() {
  document
    .getElementById(`plans`)
    ?.scrollIntoView({ behavior: `smooth`, block: `start` })
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

export function TeamBillingSection({
  teamId,
  proProductId,
  businessProductId,
  businessYearlyProductId,
}: {
  teamId: string
  proProductId: string | null
  businessProductId: string | null
  businessYearlyProductId?: string | null
}) {
  const billingPlan = useBillingPlan(teamId)
  const [portalLoading, setPortalLoading] = useState(false)
  const [showPlans, setShowPlans] = useState(false)
  const [showSeatDialog, setShowSeatDialog] = useState(false)

  if (!billingPlan || billingPlan.plan === `unlimited`) return null

  const { plan, limits, usage, subscription } = billingPlan
  const isPaid = plan === `pro` || plan === `business`
  // Seat changes mutate the existing subscription (billing.updateSeats) — a
  // second checkout would stack a second full-price subscription (pay-twice).
  const canAdjustSeats = Boolean(subscription && !subscription.cancelAtPeriodEnd)
  // Seats are counted from non-agent members (usage.members already excludes
  // the widget's synthetic isAgent user). A full or over-provisioned team
  // blocks new invites (downgrade policy: existing members keep working).
  const seatsFull =
    limits.seats !== Infinity && usage.members >= limits.seats

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
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <CardTitle className="text-base">Plan & Billing</CardTitle>
              <CardDescription>
                Manage your team subscription
              </CardDescription>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant={PLAN_BADGE_VARIANT[plan]}>
                {PLAN_LABELS[plan]}
              </Badge>
              {canAdjustSeats && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setShowSeatDialog(true)}
                >
                  <Users className="mr-1.5 size-3.5" />
                  Adjust seats
                </Button>
              )}
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
            label="Seats"
            current={usage.members}
            max={limits.seats}
          />
          <UsageBar
            label="Storage"
            current={usage.storageMb}
            max={limits.storageMb}
            formatValue={formatStorage}
          />
          <UsageBar
            label="Feedback widgets"
            current={usage.widgetConfigs}
            max={limits.widgetConfigs}
          />

          {seatsFull && (
            <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm">
              <AlertTriangle className="mt-0.5 size-4 shrink-0 text-amber-500" />
              <div className="space-y-1.5">
                <p className="text-muted-foreground">
                  {usage.members > limits.seats
                    ? `This team has ${usage.members} members but only ${limits.seats} seat${
                        limits.seats === 1 ? `` : `s`
                      }. New invites are blocked until you add seats.`
                    : `All ${limits.seats} seat${
                        limits.seats === 1 ? `` : `s`
                      } are in use. Add seats or upgrade to invite more teammates.`}
                </p>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() =>
                    canAdjustSeats
                      ? setShowSeatDialog(true)
                      : isPaid
                        ? handlePortal()
                        : // Free plan: the comparison grid is always rendered
                          // below — scroll it into view (EXP-35: the old
                          // setShowPlans(true) was a no-op on this branch).
                          scrollToPlans()
                  }
                  disabled={!canAdjustSeats && isPaid && portalLoading}
                >
                  {!isPaid
                    ? `Upgrade`
                    : !canAdjustSeats && portalLoading
                      ? `Loading...`
                      : `Add seats`}
                </Button>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {/* `plans` anchors the upgrade nudges (the seats-full button above and
          the repositories section's plan-cap nudge). Only one branch renders,
          so the id stays unique. */}
      {plan === `free` ? (
        <div id="plans" className="scroll-mt-6">
          <PlanComparison
            currentPlan={plan}
            teamId={teamId}
            proProductId={proProductId}
            businessProductId={businessProductId}
            businessYearlyProductId={businessYearlyProductId}
            subscription={subscription}
          />
        </div>
      ) : (
        <div id="plans" className="scroll-mt-6">
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
                teamId={teamId}
                proProductId={proProductId}
                businessProductId={businessProductId}
                businessYearlyProductId={businessYearlyProductId}
                subscription={subscription}
              />
            </div>
          )}
        </div>
      )}

      {subscription && (
        <AdjustSeatsDialog
          teamId={teamId}
          currentSeats={subscription.seats}
          memberCount={usage.members}
          periodEnd={subscription.periodEnd}
          open={showSeatDialog}
          onOpenChange={setShowSeatDialog}
        />
      )}
    </div>
  )
}
