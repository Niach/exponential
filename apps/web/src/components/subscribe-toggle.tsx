import { useState } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { Bell, BellOff } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { issueSubscriberCollection } from "@/lib/collections"
import { Pill } from "@/components/ui/pill"
import { IconTooltip } from "@/components/icon-tooltip"

// Live subscription state + the toggle mutation for one issue, extracted from
// the button (EXP-687) so the mobile issue-detail `…` menu can offer the same
// action as a menu ITEM without duplicating the query.
export function useIssueSubscription(issueId: string, currentUserId: string) {
  const { data: rows } = useLiveQuery(
    (query) =>
      query
        .from({ s: issueSubscriberCollection })
        .where(({ s }) =>
          and(eq(s.issueId, issueId), eq(s.userId, currentUserId))
        ),
    [issueId, currentUserId]
  )
  const row = rows?.[0]
  const subscribed = row ? !row.unsubscribed : false
  const [busy, setBusy] = useState(false)

  const toggle = async () => {
    setBusy(true)
    try {
      if (subscribed) {
        await trpc.subscriptions.unsubscribe.mutate({ issueId })
      } else {
        await trpc.subscriptions.subscribe.mutate({ issueId })
      }
    } finally {
      setBusy(false)
    }
  }

  return { subscribed, busy, toggle }
}

// Per-issue subscribe toggle with live state from the issue_subscribers shape.
// Subscribed users get inbox activity + (plan-gated) push for the issue.
export function SubscribeToggle({
  issueId,
  currentUserId,
}: {
  issueId: string
  currentUserId: string
}) {
  const { subscribed, busy, toggle } = useIssueSubscription(
    issueId,
    currentUserId
  )

  // The button already reads "Subscribe"/"Subscribed", so the tooltip spends its
  // words on what subscribing actually gets you rather than restating the label.
  return (
    <IconTooltip
      label={
        subscribed
          ? `Stop getting notified about this issue`
          : `Get notified about comments and updates`
      }
    >
      <Pill mode="action" onClick={() => void toggle()} disabled={busy}>
        {/* Icon-only below `sm` — the label doesn't fit the mobile
            breadcrumb row (EXP-189). */}
        {subscribed ? (
          <>
            <Bell className="size-3" />
            <span className="hidden sm:inline">Subscribed</span>
          </>
        ) : (
          <>
            <BellOff className="size-3" />
            <span className="hidden sm:inline">Subscribe</span>
          </>
        )}
      </Pill>
    </IconTooltip>
  )
}
