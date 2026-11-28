import { useMemo, useState } from "react"
import type { SurfaceActionEvent } from "@exponential_app/eui-react"
import { CI_PAGE_COPY, CI_PAGE_TEST_IDS, CI_TABS, ciTabCounts, type CiPageInput, type CiRunFilter, type CiTab } from "@exp/domain-contract/ci-page"
import pageFixture from "@exp/domain-contract/fixtures/ci-page.json"
import { conceptIcon, SegmentedControl, useIsMobile } from "@exp/ui"
import { CiSurface } from "@/components/ci/ci-surface"
import { PageHeading } from "@/components/page-frame"
import { CI_PAGE_EVENTS, ciPageTree } from "@/lib/ci-page-view"
import { failedAction } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"

// #EXP-2172 (fixtures/ci-page.json): THE CI page's body over its input; the
// route (`/ci`) owns the data and the navigation, the `?dev=ci` specimen the
// fixture's `scenario`.

const CiIcon = conceptIcon(`nav-ci`)

export const ALL_RUNS: CiRunFilter = { status: `all`, repositoryId: null, source: null }

const TABS = new Set<CiTab>(CI_TABS)
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
  onOpenJob,
  onOpenPullRequest,
  onOpenRunner,
}: {
  input: CiPageInput
  tab: CiTab
  filter: CiRunFilter
  onTab: (tab: CiTab) => void
  onFilter: (filter: CiRunFilter) => void
  onOpenRun: (runId: string) => void
  onOpenJob: (runId: string, jobId: string) => void
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
      case CI_PAGE_EVENTS.openJob: {
        const run = context(`run`)
        const job = context(`job`)
        if (run && job) onOpenJob(run, job)
        return
      }
      case CI_PAGE_EVENTS.openUrl: {
        const url = context(`url`)
        if (url) window.open(url, `_blank`, `noopener`)
        return
      }
      // #EXP-2214: the queue's own controls (Merge now is owners-only: the
      // server refuses others, the toast says why).
      case CI_PAGE_EVENTS.queueMenu: {
        const pr = context(`pr`)
        if (!pr) return
        if (value === `merge-now`) void trpc.pullRequests.mergeNow.mutate({ pullRequestId: pr }).catch((error) => failedAction(`merge`, `the pull request now`, error))
        if (value === `hold`) void trpc.pullRequests.hold.mutate({ pullRequestId: pr, hold: true }).catch((error) => failedAction(`hold`, `the pull request`, error))
        return
      }
      case CI_PAGE_EVENTS.resume: {
        const pr = context(`pr`)
        if (pr) void trpc.pullRequests.hold.mutate({ pullRequestId: pr, hold: false }).catch((error) => failedAction(`resume`, `the pull request`, error))
        return
      }
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
  const counts = ciTabCounts(input)
  // #EXP-2169: THE app's segmented control (the board's and the repo page's
  // switcher), each count after its label in muted text.
  return (
    <div data-testid={CI_PAGE_TEST_IDS.page} className="w-full min-w-0 space-y-4">
      <div data-testid={CI_PAGE_TEST_IDS.tabs}>
      <SegmentedControl
        value={tab}
        onValueChange={(next) => TABS.has(next) && onTab(next)}
        className="w-max"
        options={CI_TABS.map((value) => {
          const count = counts[value]
          return {
            value,
            label: (
              <span className="flex items-center gap-1.5 whitespace-nowrap" data-tab={value}>
                {(phone ? CI_PAGE_COPY.phoneTabs : CI_PAGE_COPY.tabs)[value]}
                {count > 0 && <span className="text-muted-foreground tabular-nums">{count}</span>}
              </span>
            ),
          }
        })}
      />
      </div>
      <CiSurface surfaceId="ci-page" tree={tree} onAction={onAction} />
    </div>
  )
}

/** DEV-ONLY `?dev=ci`: the page over ci-page.json's `scenario`, no server. */
export function CiPageSpecimen() {
  const input = pageFixture.scenario as unknown as CiPageInput
  const [tab, setTab] = useState<CiTab>(`train`)
  const [filter, setFilter] = useState<CiRunFilter>(ALL_RUNS)
  const phone = useIsMobile()
  // The /ci route's wide chrome (#EXP-2169): the entity heading in the
  // device page's column.
  return (
    <div className="mx-auto w-full max-w-4xl space-y-6">
      {!phone && <PageHeading icon={CiIcon} title={CI_PAGE_COPY.title} />}
      <CiPageView
        input={input}
        tab={tab}
        filter={filter}
        onTab={setTab}
        onFilter={setFilter}
        onOpenRun={() => undefined}
        onOpenJob={() => undefined}
        onOpenPullRequest={() => undefined}
        onOpenRunner={() => undefined}
      />
    </div>
  )
}
