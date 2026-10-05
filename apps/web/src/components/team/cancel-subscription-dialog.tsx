import { useState } from "react"
import { promptActions, WEB_PROMPTS } from "@/lib/prompts"
import { Prompt, toast } from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { invalidateBillingCache } from "@/hooks/use-billing"

// Self-service cancellation of the TEAM's subscription (REV2-55: the
// subscription belongs to the team, not to whoever paid for it). Always
// scheduled for the end of the paid period — the team keeps every seat, byte
// and paid feature until then. This is also the prerequisite for deleting a
// paying team: teams.delete refuses while a live subscription exists.
export function CancelSubscriptionDialog({
  teamId,
  planLabel,
  periodEnd,
  open,
  onOpenChange,
}: {
  teamId: string
  planLabel: string
  periodEnd: string | null
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const [saving, setSaving] = useState(false)

  const endDate = periodEnd
    ? new Date(periodEnd).toLocaleDateString(undefined, {
        year: `numeric`,
        month: `long`,
        day: `numeric`,
      })
    : null

  const handleCancel = async () => {
    setSaving(true)
    try {
      await trpc.billing.cancelSubscription.mutate({ teamId })
      invalidateBillingCache()
      toast.success(
        endDate
          ? `Subscription cancelled. The team stays on ${planLabel} until ${endDate}.`
          : `Subscription cancelled. The team stays on ${planLabel} until the end of the paid period.`
      )
      onOpenChange(false)
    } catch (err) {
      console.error(`[billing] cancel failed:`, err)
      toast.error(
        err instanceof Error && err.message
          ? err.message
          : `Couldn't cancel the subscription`
      )
    } finally {
      setSaving(false)
    }
  }

  const copy = WEB_PROMPTS.cancelSubscription(planLabel, endDate)

  return (
    <Prompt
      open={open}
      onOpenChange={onOpenChange}
      busy={saving}
      title={copy.title}
      body={copy.body}
      actions={promptActions(copy, {
        "cancel-subscription": { busy: saving, onSelect: handleCancel },
      })}
    />
  )
}
