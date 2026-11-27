import { useMemo } from "react"
import { createFileRoute, useNavigate, useRouter } from "@tanstack/react-router"
import { CI_PAGE_COPY, type CiPageInput, type CiRunFilter, type CiSource, type CiTab } from "@exp/domain-contract/ci-page"
import { conceptIcon, MobileDetailHeader, SkeletonRows, useIsMobile } from "@exp/ui"
import { CiPageView } from "@/components/ci/ci-page"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { PushedTitle } from "@/components/team/mobile-page-header"
import { PageHeading, pageFrameClass } from "@/components/page-frame"
import { useCiPageRows, useCiRepos } from "@/hooks/use-ci-page"
import { useNow } from "@/hooks/use-now"
import { ciRunCollection, deviceCollection, pullRequestCollection, sessionCollection } from "@/lib/collections"
import { pageTitle } from "@/lib/page-title"

// #EXP-2172 (ci-page.json): THE CI page, a root page across every member
// team (sidebar-tree.json root `ci`; the phones' More menu). The tab and the
// filters ride the search params, so a reload and Back keep them.

const CiIcon = conceptIcon(`nav-ci`)

type CiSearch = { tab?: CiTab; status?: CiRunFilter[`status`]; repo?: string; source?: CiSource }

const TABS: readonly CiTab[] = [`runs`, `train`, `runners`]
const STATUSES: readonly CiRunFilter[`status`][] = [`all`, `running`, `failed`, `mine`]

export const Route = createFileRoute(`/_app/ci`)({
  head: () => ({ meta: [{ title: pageTitle(CI_PAGE_COPY.title) }] }),
  validateSearch: (search: Record<string, unknown>): CiSearch => ({
    ...(TABS.includes(search.tab as CiTab) && search.tab !== `runs` ? { tab: search.tab as CiTab } : {}),
    ...(STATUSES.includes(search.status as CiRunFilter[`status`]) && search.status !== `all`
      ? { status: search.status as CiRunFilter[`status`] }
      : {}),
    ...(typeof search.repo === `string` && search.repo ? { repo: search.repo } : {}),
    ...(search.source === `github` || search.source === `exponential` ? { source: search.source } : {}),
  }),
  component: CiRoute,
})

function CiRoute() {
  const { user } = Route.useRouteContext()
  const search = Route.useSearch()
  const navigate = useNavigate()
  const router = useRouter()
  const isMobile = useIsMobile()
  const { teamIds, byId } = useMemberTeams(user.id)
  const repos = useCiRepos(teamIds)
  const rows = useCiPageRows(user.id)
  const running = rows.runs.some((run) => run.state === `pending`)
  const now = useNow(running ? 1_000 : 60_000)
  const input = useMemo<CiPageInput>(
    () => ({ ...rows, repos, now: now.toISOString(), utcOffsetMinutes: -now.getTimezoneOffset() }),
    [rows, repos, now]
  )
  const tab = search.tab ?? `runs`
  const filter: CiRunFilter = { status: search.status ?? `all`, repositoryId: search.repo ?? null, source: search.source ?? null }

  const setSearch = (next: { tab: CiTab; filter: CiRunFilter }) =>
    void navigate({
      to: `/ci`,
      search: {
        ...(next.tab !== `runs` ? { tab: next.tab } : {}),
        ...(next.filter.status !== `all` ? { status: next.filter.status } : {}),
        ...(next.filter.repositoryId ? { repo: next.filter.repositoryId } : {}),
        ...(next.filter.source ? { source: next.filter.source } : {}),
      },
      replace: true,
    })
  const teamSlugOf = (teamId: string | null | undefined) => (teamId ? (byId.get(teamId)?.slug ?? null) : null)
  const openRun = (runId: string) => {
    const slug = teamSlugOf(ciRunCollection.get(runId)?.teamId)
    if (slug) void navigate({ to: `/t/$teamSlug/ci/$runId`, params: { teamSlug: slug, runId } })
  }
  // A pull request opens its chat (where it was made), else its page.
  const openPullRequest = (pullRequestId: string) => {
    const pr = pullRequestCollection.get(pullRequestId)
    if (!pr) return
    const root = pr.rootSessionId ? sessionCollection.get(pr.rootSessionId) : undefined
    const slug = teamSlugOf(root?.teamId ?? pr.teamId)
    if (root && slug) void navigate({ to: `/t/$teamSlug/sessions/$sessionId`, params: { teamSlug: slug, sessionId: root.id } })
    else window.open(pr.url, `_blank`, `noopener`)
  }
  const openRunner = (rowId: string) => {
    const device = deviceCollection.get(rowId)
    if (device) void navigate({ to: `/devices/$deviceId`, params: { deviceId: device.deviceId } })
  }

  return (
    <div className="h-full overflow-y-auto max-md:h-dvh">
      {isMobile && (
        <div className="sticky top-0 z-20 pt-[env(safe-area-inset-top)]">
          <MobileDetailHeader
            title={<PushedTitle>{CI_PAGE_COPY.title}</PushedTitle>}
            onBack={() => (router.history.canGoBack() ? router.history.back() : void navigate({ to: `/chats` }))}
          />
        </div>
      )}
      <div className={pageFrameClass()}>
        {/* #EXP-2169: wide pages carry the entity heading (the device page's). */}
        {!isMobile && <PageHeading icon={CiIcon} title={CI_PAGE_COPY.title} />}
        {rows.ready ? (
          <CiPageView
            input={input}
            tab={tab}
            filter={filter}
            onTab={(next) => setSearch({ tab: next, filter })}
            onFilter={(next) => setSearch({ tab, filter: next })}
            onOpenRun={openRun}
            onOpenPullRequest={openPullRequest}
            onOpenRunner={openRunner}
          />
        ) : (
          <SkeletonRows />
        )}
      </div>
    </div>
  )
}

