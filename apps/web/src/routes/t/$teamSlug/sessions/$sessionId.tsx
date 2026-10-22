import { createFileRoute, Link, Navigate, redirect } from "@tanstack/react-router"
import { Button, SkeletonRows } from "@exp/ui"
import { Empty } from "@/components/empty"
import { SessionScreen } from "@/components/thread/session-body"
import { emptyStateCopy } from "@/lib/empty-states"
import { useSessionRow } from "@/hooks/use-session-tree"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { pageTitle } from "@/lib/page-title"

// EXP-1316: a session opens INSIDE its thread (`?tab=session&session=<id>`
// on the thread route: the thread's header + strip stay, the body is the raw
// work). This route stays for deep links, notifications and the work-tab
// strip's stored session tabs: it REDIRECTS (replace) once the row resolves,
// keeping `from`. A session under a team AGENT has no thread page: it keeps
// its own screen here (back to the agent's channel).
//
// EXP-1291: the PR body's footer links `sessions/<rootId>?view=guide`; that
// URL opens the thread's Guide tab.
type SessionSearch = { view?: `guide`; from?: string }

export const Route = createFileRoute(`/t/$teamSlug/sessions/$sessionId`)({
  head: () => ({ meta: [{ title: pageTitle(`Session`) }] }),
  validateSearch: (search: Record<string, unknown>): SessionSearch => ({
    ...(search.view === `guide` ? { view: `guide` as const } : {}),
    ...(typeof search.from === `string` && search.from ? { from: search.from } : {}),
  }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: SessionPage,
})

function Loading() {
  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* D8 first load: the detail page's placeholder (3 list rows). */}
      <div className="p-6">
        <SkeletonRows />
      </div>
    </div>
  )
}

function SessionPage() {
  const { teamSlug, sessionId } = Route.useParams()
  const { view, from } = Route.useSearch()
  const team = useTeamBySlug(teamSlug)
  const { row, isReady } = useSessionRow(sessionId)
  const rootId = row ? (row.rootId ?? row.id) : null
  const { row: rootRow, isReady: rootReady } = useSessionRow(
    rootId && rootId !== row?.id ? rootId : null
  )
  const root = rootId === row?.id ? row : rootRow

  if (!team || (!row && !isReady)) return <Loading />

  if (!row || !rootId) {
    return (
      <div className="flex h-full min-h-0 flex-col items-center justify-center">
        <Empty id="run-not-found">
          <Button asChild variant="outline" size="sm">
            <Link to="/t/$teamSlug/agent" params={{ teamSlug }}>
              {emptyStateCopy(`run-not-found`).action}
            </Link>
          </Button>
        </Empty>
      </div>
    )
  }

  if (view === `guide`) {
    return (
      <Navigate
        to="/t/$teamSlug/threads/$threadId"
        params={{ teamSlug, threadId: rootId }}
        search={{ tab: `guide` }}
        replace
      />
    )
  }

  // The root decides: an agent's conversation tree has no thread page.
  if (!root && !rootReady) return <Loading />
  if (root?.agentId) {
    return <SessionScreen key={row.id} teamSlug={teamSlug} row={row} />
  }

  return (
    <Navigate
      to="/t/$teamSlug/threads/$threadId"
      params={{ teamSlug, threadId: rootId }}
      search={{ tab: `session`, session: row.id, ...(from ? { from } : {}) }}
      replace
    />
  )
}
