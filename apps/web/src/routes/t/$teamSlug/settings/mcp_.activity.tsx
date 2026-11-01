import { useEffect, useState } from "react"
import { createFileRoute } from "@tanstack/react-router"
import { activityCaption, SETTINGS_MCP } from "@exp/domain-contract/settings-mcp"
import { GlassSectionHeader, ListRow, Pill, SETTINGS_LIST_CLASS, SkeletonRows } from "@exp/ui"
import { Empty } from "@/components/empty"
import { compactRelativeTime } from "@/lib/relative-time"
import { pageTitle } from "@/lib/page-title"
import { trpc } from "@/lib/trpc-client"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"

// #VAPP-185 (fixtures/settings-mcp.json `activityPage`): the `mcp_calls`
// audit, newest first: owners the team's calls, a member their own.
export const Route = createFileRoute(`/t/$teamSlug/settings/mcp_/activity`)({
  head: () => ({ meta: [{ title: pageTitle(SETTINGS_MCP.activity, `Settings`) }] }),
  component: McpActivity,
})

type Activity = Awaited<ReturnType<typeof trpc.mcpActivity.list.query>>

function McpActivity() {
  const { teamSlug } = Route.useParams()
  const { team } = useSettingsPage(teamSlug)
  const [data, setData] = useState<Activity | null>(null)
  useEffect(() => {
    if (!team) return
    trpc.mcpActivity.list
      .query({ teamId: team.id })
      .then(setData)
      .catch(() => setData({ owner: false, calls: [] }))
  }, [team?.id])
  return (
    <div data-testid={SETTINGS_MCP.testIds.activity}>
      <GlassSectionHeader label={SETTINGS_MCP.activity} />
      {data === null ? (
        <SkeletonRows rowHeight={52} />
      ) : data.calls.length === 0 ? (
        <Empty id="mcp-activity" />
      ) : (
        <div className={SETTINGS_LIST_CLASS}>
          {data.calls.map((c) => (
            <ListRow key={c.id} className="gap-3 px-3 py-2" data-testid="mcp-activity-row">
              <div className="min-w-0 flex-1">
                <div className="truncate font-mono text-sm">{c.tool}</div>
                <div className="truncate text-xs text-muted-foreground">
                  {activityCaption(data.owner, { ...c, when: compactRelativeTime(c.createdAt) })}
                </div>
              </div>
              <Pill size="sm" className="shrink-0">
                {SETTINGS_MCP.statusLabels[c.status as keyof typeof SETTINGS_MCP.statusLabels] ?? c.status}
              </Pill>
            </ListRow>
          ))}
        </div>
      )}
    </div>
  )
}
