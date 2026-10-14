import { useMemo } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { SkeletonRows, useIsMobile } from "@exp/ui"
import { ThreadsListView, useTeamPullRequestsByRoot } from "@/components/thread/threads-list"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useAgents, useTeamSessions, useThreadsList } from "@/hooks/use-session-tree"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"
import { THREADS_LIST_COPY } from "@/lib/session-tree"

// EXP-1293 Threads v2: THE Threads page (replaces Reviews + the Agent page's
// Recent): the ACTIVE team's threads in three bands, Needs you · Running ·
// Recent (`partitionThreads`, fixture `threads-list.json` ×4). Rows open the
// thread; phones put the team's Agents band on top (rows open the agent's
// channel).

export const Route = createFileRoute(`/t/$teamSlug/threads/`)({
  head: () => ({ meta: [{ title: pageTitle(THREADS_LIST_COPY.title) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ThreadsPage,
})

function ThreadsPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const team = useTeamBySlug(teamSlug)
  const { groups, extras, isReady } = useThreadsList(team?.id)
  const { rows } = useTeamSessions(team?.id)
  const { agents } = useAgents(isMobile ? team?.id : undefined)
  const prs = useTeamPullRequestsByRoot(team?.id)
  const cliByRoot = useMemo(
    () => new Map(rows.filter((row) => row.parentId === null).map((row) => [row.id, row.cli])),
    [rows]
  )

  return (
    // EXP-771: the SCROLLER is full width, the reading column lives inside it.
    <div className="h-full overflow-y-auto">
      <div className={`mx-auto w-full max-w-3xl px-4 py-4 ${TAB_BAR_CLEARANCE}`}>
        {!team ? (
          <SkeletonRows rowHeight={36} />
        ) : (
          <ThreadsListView
            groups={groups}
            identifierByRoot={extras.identifierByRoot}
            cliByRoot={cliByRoot}
            prsByRoot={prs.byRoot}
            teamPrs={prs.all}
            agents={isMobile ? agents : undefined}
            loading={!isReady}
            onOpenThread={(threadId) =>
              void navigate({
                to: `/t/$teamSlug/threads/$threadId`,
                params: { teamSlug, threadId },
              })
            }
            onOpenAgent={(agentId) =>
              void navigate({
                to: `/t/$teamSlug/agents/$agentId`,
                params: { teamSlug, agentId },
              })
            }
          />
        )}
      </div>
    </div>
  )
}
