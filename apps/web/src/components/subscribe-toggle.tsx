import { useState } from "react"
import { and, eq, useLiveQuery } from "@tanstack/react-db"
import { Bell, BellOff } from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { issueSubscriberCollection } from "@/lib/collections"
import { Button } from "@/components/ui/button"

// Per-issue subscribe toggle with live state from the issue_subscribers shape.
// Subscribed users get inbox activity + (plan-gated) push for the issue.
export function SubscribeToggle({
  issueId,
  currentUserId,
}: {
  issueId: string
  currentUserId: string
}) {
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

  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-6 gap-1.5 px-2 text-xs"
      onClick={() => void toggle()}
      disabled={busy}
      title={subscribed ? `Unsubscribe from this issue` : `Subscribe to this issue`}
    >
      {subscribed ? (
        <>
          <Bell className="size-3" /> Subscribed
        </>
      ) : (
        <>
          <BellOff className="size-3" /> Subscribe
        </>
      )}
    </Button>
  )
}
