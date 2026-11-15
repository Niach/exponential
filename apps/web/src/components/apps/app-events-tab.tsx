import { useEffect, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { GlassGroup, GlassSectionHeader, ListRow, Pill } from "@exp/ui"
import { APPS_EVENTS_TAB, APPS_TEST_IDS, eventRunRow } from "@exp/domain-contract/apps"
import type { AgentTrigger } from "@exp/db-schema/domain"
import { agentCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { relativeTime } from "@/components/comment-rows/format"
import { AgentTriggersFor, readTriggers } from "@/components/agents/agent-triggers"
import { AppMcpSections } from "@/components/apps/app-mcp-sections"

// #EXP-1850 (apps.json `eventsTab`): an app's EVENTS tab = THE trigger list
// of the app's agent (the #EXP-1846 piece, every kind), the Webhooks group
// (moved here from Settings) and Recent runs (tRPC `apps.events`).

const TAB = APPS_EVENTS_TAB
const REFRESH_MS = 15_000

type Runs = Awaited<ReturnType<typeof trpc.apps.events.query>>[`runs`]

export function AppEventsTab({
  app,
  teamSlug,
  canManage,
  onOpenChat,
}: {
  app: { id: string; agentId: string | null; headVersion: number }
  teamSlug: string
  canManage: boolean
  /** A `woke` run opens the Chat tab. */
  onOpenChat: () => void
}) {
  const { data: agentRows } = useLiveQuery(
    (query) =>
      app.agentId ? query.from({ agents: agentCollection }).where(({ agents }) => eq(agents.id, app.agentId!)) : undefined,
    [app.agentId]
  )
  const agent = agentRows?.[0] as { id: string; triggers: unknown } | undefined
  const triggers: AgentTrigger[] = readTriggers(agent?.triggers)
  const runs = useRecentRuns(app.id, JSON.stringify(agent?.triggers ?? null))

  return (
    <div className="space-y-6" data-testid={APPS_TEST_IDS.eventsTab}>
      {agent && <AgentTriggersFor agent={agent} readOnly={!canManage} />}
      <AppMcpSections appId={app.id} teamSlug={teamSlug} headVersion={app.headVersion} part="events" />
      <div data-testid={APPS_TEST_IDS.recentRuns}>
        <GlassSectionHeader label={TAB.copy.recentRuns} />
        <GlassGroup>
          {runs === null ? null : runs.length === 0 ? (
            <div className="px-4 py-3 text-sm text-muted-foreground">{TAB.copy.noRuns}</div>
          ) : (
            runs.map((run, index) => {
              const row = eventRunRow(run, triggers)
              const woke = row.badge === `woke`
              return (
                <ListRow
                  key={`${run.at}-${index}`}
                  interactive={woke}
                  onClick={woke ? onOpenChat : undefined}
                  className="gap-3 rounded-none px-4 py-2"
                  data-testid={APPS_TEST_IDS.runRow}
                >
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-sm">{row.title}</div>
                    {row.detail && <div className="truncate text-xs text-muted-foreground">{row.detail}</div>}
                  </div>
                  <span className="shrink-0 text-xs text-muted-foreground tabular-nums">{relativeTime(run.at)}</span>
                  <Pill size="sm" className="shrink-0">
                    {TAB.copy.badges[row.badge]}
                  </Pill>
                </ListRow>
              )
            })
          )}
        </GlassGroup>
      </div>
    </div>
  )
}

/** The app's recent runs, refetched while the tab is open and whenever the
 *  agent's triggers change. Null while the first read is in flight. */
function useRecentRuns(appId: string, triggersKey: string): Runs | null {
  const [runs, setRuns] = useState<Runs | null>(null)
  useEffect(() => {
    let cancelled = false
    const load = () =>
      trpc.apps.events
        .query({ id: appId })
        .then((next) => !cancelled && setRuns(next.runs))
        .catch(() => !cancelled && setRuns((current) => current ?? []))
    void load()
    const timer = setInterval(() => void load(), REFRESH_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [appId, triggersKey])
  return runs
}
