import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import {
  ActionPage,
  parseActionPageTab,
  type ActionPageTab,
} from "@/components/action-page"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { pageTitle } from "@/lib/page-title"

// SLOP-2: ONE action — its prompt, its triggers and its runs. A phone shows
// the three as tabs with the active one in `?tab=`, so a back/refresh (and a
// run's Back, which returns to `?tab=runs`) lands where the person was; a
// desktop viewport shows all three and ignores the param.

type ActionSearch = { tab?: Exclude<ActionPageTab, `prompt`> }

export const Route = createFileRoute(`/t/$teamSlug/actions/$actionId`)({
  head: () => ({ meta: [{ title: pageTitle(`Actions`) }] }),
  validateSearch: (search: Record<string, unknown>): ActionSearch => {
    const tab = parseActionPageTab(search.tab)
    return { tab: tab === `prompt` ? undefined : tab }
  },
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: ActionDetailPage,
})

function ActionDetailPage() {
  const { teamSlug, actionId } = Route.useParams()
  const { tab } = Route.useSearch()
  const navigate = useNavigate()
  const team = useTeamBySlug(teamSlug)

  return (
    <div className="h-full overflow-y-auto">
      {/* EXP-1190: fills the scroller so a swipe anywhere below the
          phone's tabs pages them. */}
      <div
        className={`mx-auto flex min-h-full w-full max-w-3xl flex-col px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
      >
        {team ? (
          <ActionPage
            key={actionId}
            team={team}
            teamSlug={teamSlug}
            actionId={actionId}
            tab={tab ?? `prompt`}
            onTabChange={(next) =>
              void navigate({
                to: `/t/$teamSlug/actions/$actionId`,
                params: { teamSlug, actionId },
                search: next === `prompt` ? {} : { tab: next },
                replace: true,
              })
            }
          />
        ) : (
          <div className="p-6 text-sm text-muted-foreground">Loading…</div>
        )}
      </div>
    </div>
  )
}
