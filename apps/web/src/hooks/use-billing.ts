import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { getRuntimeConfig } from "@/lib/runtime-config"
import type { PlanTier } from "@/lib/billing"

// Per-seat model (masterplan v5 §3.2). The only monetized axes are seats
// (non-agent members), attachment storage, and feedback-widget configs.
export type BillingSubscription = {
  productId: string
  seats: number
  periodEnd: string | null
  cancelAtPeriodEnd: boolean
}

export type BillingPlan = {
  plan: PlanTier
  limits: {
    seats: number
    storageMb: number
    widgetConfigs: number
  }
  usage: {
    members: number
    storageMb: number
    widgetConfigs: number
  }
  // The workspace's active subscription, when it has one. Its presence
  // switches the settings UI from "checkout" to "adjust seats / switch plan"
  // (mutating the existing subscription — never a second checkout).
  subscription: BillingSubscription | null
}

const UNLIMITED_PLAN: BillingPlan = {
  plan: `unlimited`,
  limits: {
    seats: Infinity,
    storageMb: Infinity,
    widgetConfigs: Infinity,
  },
  usage: { members: 0, storageMb: 0, widgetConfigs: 0 },
  subscription: null,
}

let isCloudCached: boolean | undefined
const listeners = new Set<() => void>()

export function invalidateBillingCache(): void {
  listeners.forEach((l) => l())
}

export function useBillingPlan(
  workspaceId: string | undefined
): BillingPlan | null {
  const [plan, setPlan] = useState<BillingPlan | null>(null)
  const [fetchKey, setFetchKey] = useState(0)

  useEffect(() => {
    const listener = () => setFetchKey((k) => k + 1)
    listeners.add(listener)
    return () => {
      listeners.delete(listener)
    }
  }, [])

  useEffect(() => {
    if (!workspaceId) return

    let cancelled = false

    void (async () => {
      if (isCloudCached === undefined) {
        const config = await getRuntimeConfig()
        isCloudCached = config.isCloud
      }

      if (!isCloudCached) {
        if (!cancelled) setPlan(UNLIMITED_PLAN)
        return
      }

      const data = await trpc.billing.workspacePlan.query({ workspaceId })
      // tRPC has no transformer (plain JSON), so `Infinity` limits serialize
      // to `null` on the wire — normalize back so `=== Infinity` checks and
      // `usage < limit` comparisons behave for unlimited caps.
      const n = (v: number | null | undefined): number =>
        v == null ? Infinity : v
      const result: BillingPlan = {
        plan: data.plan as PlanTier,
        limits: {
          seats: n(data.limits.seats),
          storageMb: n(data.limits.storageMb),
          widgetConfigs: n(data.limits.widgetConfigs),
        },
        usage: data.usage,
        subscription: data.subscription ?? null,
      }
      if (!cancelled) setPlan(result)
    })()

    return () => {
      cancelled = true
    }
  }, [workspaceId, fetchKey])

  return plan
}
