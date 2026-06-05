import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { ChevronDown, ChevronRight, Activity } from "lucide-react"
import type { IssueEvent, Label, User } from "@/db/schema"
import { issueEventCollection, labelCollection } from "@/lib/collections"
import { Button } from "@/components/ui/button"
import { EventRow } from "@/components/comment-rows/event"
import { AGENT_EVENT_TYPES } from "@/components/agent-plan-panel"

// A quiet, collapsible feed of agent lifecycle events (started, plan ready,
// question, answer, PR opened/merged, error). Separate from the human comment
// thread so routine agent activity doesn't read as conversation.
export function AgentActivityFeed({
  issueId,
  users,
}: {
  issueId: string
  users: User[]
}) {
  const [open, setOpen] = useState(false)

  const { data: events } = useLiveQuery(
    (query) =>
      query
        .from({ e: issueEventCollection })
        .where(({ e }) => eq(e.issueId, issueId))
        .orderBy(({ e }) => e.createdAt),
    [issueId]
  )
  const { data: labels } = useLiveQuery((query) =>
    query.from({ labels: labelCollection })
  )

  const userMap = useMemo(() => new Map(users.map((u) => [u.id, u])), [users])
  const labelMap = useMemo(
    () => new Map((labels ?? []).map((l) => [l.id, l as Label])),
    [labels]
  )

  const agentEvents = ((events ?? []) as IssueEvent[]).filter((e) =>
    AGENT_EVENT_TYPES.has(e.type)
  )
  if (agentEvents.length === 0) return null

  return (
    <div className="mx-4 mt-2 rounded-md border border-border/60">
      <Button
        type="button"
        variant="ghost"
        onClick={() => setOpen((v) => !v)}
        className="flex h-auto w-full items-center justify-start gap-2 px-3 py-2 text-xs font-normal text-muted-foreground"
      >
        {open ? (
          <ChevronDown className="size-3.5" />
        ) : (
          <ChevronRight className="size-3.5" />
        )}
        <Activity className="size-3.5" />
        <span>Agent activity ({agentEvents.length})</span>
      </Button>
      {open && (
        <div className="border-t border-border/60 px-2 py-1">
          {agentEvents.map((event) => (
            <EventRow
              key={`a-${event.id}`}
              event={event}
              userMap={userMap}
              labelMap={labelMap}
            />
          ))}
        </div>
      )}
    </div>
  )
}
