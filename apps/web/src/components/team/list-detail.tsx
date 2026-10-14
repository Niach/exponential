import type { ReactNode } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { Empty } from "@/components/empty"
import {
  originLabel,
  originListNavigation,
  type DetailOrigin,
} from "@/lib/detail-origin"
import { InboxListNav } from "@/components/team/list-nav"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"

// EXP-1246: the ONE list-detail host (desktop `second_sidebar_for` parity).
// One list stays beside what it opens: the Inbox (both its lists; EXP-1293
// retired the Agent › Recent arm with the Agent page's history). The host
// lives in the sidebar's panel slot (`sidebar.tsx`), keyed by the list, so
// walking the list one detail at a time never remounts it — the selection
// opens to the right, the list stays put. On the list SCREEN itself (the md+
// Inbox page) nothing is selected: the page draws `ListDetailEmpty`. Which
// list is up is the URL's (`sidebarOccupant`).

/** Whether the location is the list screen itself, not a detail beside it. */
function isListScreen(pathname: string, origin: DetailOrigin): boolean {
  const rest = pathname.replace(/^\/t\/[^/]+/, ``).replace(/\/$/, ``)
  return origin.kind === `inbox` && rest === `/inbox`
}

/** A stable key per list — the sidebar remounts the host only between lists. */
export function listDetailKey(origin: DetailOrigin): string {
  return origin.kind
}

export function ListDetailPane({
  teamSlug,
  origin,
}: {
  teamSlug: string
  origin: DetailOrigin
}) {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const listScreen = isListScreen(pathname, origin)

  const backToList = () => {
    const target = originListNavigation(teamSlug, origin)
    if (target) void navigate(target as never)
  }

  const tab = origin.kind === `inbox` && origin.tab === `my-issues` ? `my-issues` : null
  return (
    <>
      <SidebarBackRow
        label={originLabel(origin)}
        onBack={listScreen ? undefined : backToList}
      />
      {/* A plain column, not `SidebarContent`: each list below owns its own
          scrollport, and two nested `overflow-auto` boxes make the sidebar
          scroll twice. */}
      <div className="flex min-h-0 flex-1 flex-col">
        <InboxListNav
          teamSlug={teamSlug}
          tab={tab}
          onTabChange={
            listScreen
              ? (next) =>
                  void navigate({
                    to: `/t/$teamSlug/inbox`,
                    params: { teamSlug },
                    search: { tab: next === `inbox` ? undefined : next },
                    replace: true,
                  })
              : undefined
          }
        />
      </div>
    </>
  )
}

/** The detail side with nothing selected — desktop `render_inbox_empty`. */
export function ListDetailEmpty({
  tab,
  children,
}: {
  tab: `inbox` | `my-issues`
  children?: ReactNode
}) {
  return (
    <div
      className="flex h-full items-center justify-center"
      data-testid="list-detail-empty"
    >
      <Empty
        id={
          tab === `my-issues`
            ? `no-selection-issue`
            : `no-selection-notification`
        }
      >
        {children}
      </Empty>
    </div>
  )
}
