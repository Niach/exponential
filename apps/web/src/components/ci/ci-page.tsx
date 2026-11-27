import { useMemo, useState } from "react"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { CI_PAGE_TEST_IDS, type CiPageInput, type CiRunFilter, type CiSource, type CiTab } from "@exp/domain-contract/ci-page"
import pageFixture from "@exp/domain-contract/fixtures/ci-page.json"
import { useIsMobile } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { CI_PAGE_EVENTS, ciPageTree } from "@/lib/ci-page-view"

// #EXP-2172 (fixtures/ci-page.json): THE CI page's body over its input; the
// route (`/ci`) owns the data and the navigation, the `?dev=ci` specimen the
// fixture's `scenario`.

export const ALL_RUNS: CiRunFilter = { status: `all`, repositoryId: null, source: null }

const TABS = new Set<CiTab>([`runs`, `train`, `runners`])
const STATUSES = new Set<CiRunFilter[`status`]>([`all`, `running`, `failed`, `mine`])

/** The value a Tabs / Segmented / Select change carries. */
function valueOf(event: SurfaceActionEvent): string | null {
  const value = event.payload?.value
  return typeof value === `string` ? value : null
}

export function CiPageView({
  input,
  tab,
  filter,
  onTab,
  onFilter,
  onOpenRun,
  onOpenPullRequest,
  onOpenRunner,
}: {
  input: CiPageInput
  tab: CiTab
  filter: CiRunFilter
  onTab: (tab: CiTab) => void
  onFilter: (filter: CiRunFilter) => void
  onOpenRun: (runId: string) => void
  onOpenPullRequest: (pullRequestId: string) => void
  onOpenRunner: (deviceId: string) => void
}) {
  const phone = useIsMobile()
  const tree = useMemo(() => ciPageTree(input, { tab, filter, phone }), [input, tab, filter, phone])
  const onAction = (event: SurfaceActionEvent) => {
    const value = valueOf(event)
    const context = (key: string) => (typeof event.context[key] === `string` ? (event.context[key] as string) : null)
    switch (event.name) {
      case CI_PAGE_EVENTS.tab:
        if (value && TABS.has(value as CiTab)) onTab(value as CiTab)
        return
      case CI_PAGE_EVENTS.status:
        if (value && STATUSES.has(value as CiRunFilter[`status`])) onFilter({ ...filter, status: value as CiRunFilter[`status`] })
        return
      case CI_PAGE_EVENTS.repository:
        if (value) onFilter({ ...filter, repositoryId: value === `all` ? null : value })
        return
      case CI_PAGE_EVENTS.source:
        if (value) onFilter({ ...filter, source: value === `all` ? null : (value as CiSource) })
        return
      case CI_PAGE_EVENTS.openRun: {
        const run = context(`run`)
        if (run) onOpenRun(run)
        return
      }
      case CI_PAGE_EVENTS.openPr: {
        const pr = context(`pr`)
        if (pr) onOpenPullRequest(pr)
        return
      }
      case CI_PAGE_EVENTS.runners:
        onTab(`runners`)
        return
      case CI_PAGE_EVENTS.openRunner: {
        const device = context(`device`)
        if (device) onOpenRunner(device)
      }
    }
  }
  return (
    <div data-testid={CI_PAGE_TEST_IDS.page} className="w-full min-w-0">
      <CiSurface surfaceId="ci-page" tree={tree} onAction={onAction} />
    </div>
  )
}

/** DEV-ONLY `?dev=ci`: the page over ci-page.json's `scenario`, no server. */
export function CiPageSpecimen() {
  const input = pageFixture.scenario as CiPageInput
  const [tab, setTab] = useState<CiTab>(`runs`)
  const [filter, setFilter] = useState<CiRunFilter>(ALL_RUNS)
  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-4 md:py-6">
      <CiPageView
        input={input}
        tab={tab}
        filter={filter}
        onTab={setTab}
        onFilter={setFilter}
        onOpenRun={() => undefined}
        onOpenPullRequest={() => undefined}
        onOpenRunner={() => undefined}
      />
    </div>
  )
}
