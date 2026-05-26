import { useEffect, useState } from "react"
import { trpc } from "@/lib/trpc-client"
import { getRuntimeConfig } from "@/lib/runtime-config"
import type { PlanTier } from "@/lib/billing"

export type BillingPlan = {
  plan: PlanTier
  limits: {
    members: number
    projects: number
    storageMb: number
    push: boolean
  }
  usage: {
    members: number
    projects: number
  }
}

const UNLIMITED_PLAN: BillingPlan = {
  plan: `unlimited`,
  limits: {
    members: Infinity,
    projects: Infinity,
    storageMb: Infinity,
    push: true,
  },
  usage: { members: 0, projects: 0 },
}

const planCache = new Map<string, BillingPlan>()
let isCloudCached: boolean | undefined

export function useBillingPlan(
  workspaceId: string | undefined
): BillingPlan | null {
  const [plan, setPlan] = useState<BillingPlan | null>(
    workspaceId ? (planCache.get(workspaceId) ?? null) : null
  )

  useEffect(() => {
    if (!workspaceId) return

    let cancelled = false

    void (async () => {
      if (isCloudCached === undefined) {
        const config = await getRuntimeConfig()
        isCloudCached = config.isCloud
      }

      if (!isCloudCached) {
        planCache.set(workspaceId, UNLIMITED_PLAN)
        if (!cancelled) setPlan(UNLIMITED_PLAN)
        return
      }

      const data = await trpc.billing.workspacePlan.query({ workspaceId })
      const result: BillingPlan = {
        plan: data.plan as PlanTier,
        limits: data.limits,
        usage: data.usage,
      }
      planCache.set(workspaceId, result)
      if (!cancelled) setPlan(result)
    })()

    return () => {
      cancelled = true
    }
  }, [workspaceId])

  return plan
}

export function invalidateBillingCache(workspaceId: string): void {
  planCache.delete(workspaceId)
}
