import type { ReactNode } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { conceptIcon, EmptyState } from "@exp/ui"
import type { Team } from "@/db/schema"
import {
  originLabel,
  originListNavigation,
  type DetailOrigin,
} from "@/lib/detail-origin"
import { setRecentRunsPanelOpen } from "@/lib/recent-runs-panel"
import { InboxListNav } from "@/components/team/list-nav"
import { RecentRunsNav } from "@/components/team/recent-runs-nav"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"

// EXP-1246: the ONE list-detail host (desktop `second_sidebar_for` parity).
// Exactly two lists stay beside what they open: the Inbox (both its lists) and
// the Agent page's Recent runs. The host lives in the sidebar's panel slot
// (`sidebar.tsx`), keyed by the list, so walking the list one detail at a time
// never remounts it — the selection opens to the right, the list stays put.
// On the list SCREEN itself (the md+ Inbox page, the Agent page with Recent
// open) nothing is selected: the Inbox page draws `ListDetailEmpty`, the Agent
// page its composer. Which list is up is the URL's (`sidebarOccupant`).

const InboxIcon = conceptIcon(`nav-inbox`)

/** Whether the location is the list screen itself, not a detail beside it. */
function isListScreen(pathname: string, origin: DetailOrigin): boolean {
  const rest = pathname.replace(/^\/t\/[^/]+/, ``).replace(/\/$/, ``)
  if (origin.kind === `inbox`) return rest === `/inbox`
  if (origin.kind === `agent`) return rest === `/agent`
  return false
}

/** A stable key per list — the sidebar remounts the host only between lists. */
export function listDetailKey(origin: DetailOrigin): string {
  return origin.kind === `agent` ? `agent:recent` : origin.kind
}

export function ListDetailPane({
  teamSlug,
  team,
  origin,
  currentUserId,
}: {
  teamSlug: string
  team: Team | null | undefined
  origin: DetailOrigin
  currentUserId: string | undefined
}) {
  const navigate = useNavigate()
  const pathname = useRouterState({ select: (s) => s.location.pathname })
  const listScreen = isListScreen(pathname, origin)

  const backToList = () => {
    const target = originListNavigation(teamSlug, origin)
    if (target) void navigate(target as never)
  }

  if (origin.kind === `agent`) {
    return (
      <>
        <SidebarBackRow
          label={originLabel(origin)}
          onBack={
            listScreen
              ? () => {
                  setRecentRunsPanelOpen(false)
                  // The panel (and this row) unmounts: hand focus back to the
                  // page's history button so a keyboard user is not dropped
                  // on <body>.
                  requestAnimationFrame(() =>
                    document
                      .querySelector<HTMLElement>(
                        `[data-testid="recent-runs-toggle"]`
                      )
                      ?.focus()
                  )
                }
              : backToList
          }
        />
        {team && <RecentRunsNav teamId={team.id} currentUserId={currentUserId} />}
      </>
    )
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
      <EmptyState
        icon={InboxIcon}
        title={tab === `my-issues` ? `No issue selected` : `No notification selected`}
        description={
          tab === `my-issues`
            ? `Pick an issue to open it here.`
            : `Pick a notification to open its issue here.`
        }
      >
        {children}
      </EmptyState>
    </div>
  )
}
