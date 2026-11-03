import type { ReactNode } from "react"
import { useNavigate, useRouterState } from "@tanstack/react-router"
import { ResizeHandle, useIsMobile } from "@exp/ui"
import { Empty } from "@/components/empty"
import type { DetailOrigin } from "@/lib/detail-origin"
import { InboxListNav } from "@/components/team/list-nav"
import { useSecondSidebar } from "@/hooks/use-sidebar-occupant"
import { cn } from "@/lib/utils"
import {
  resetSidebarWidth,
  setSidebarResizing,
  setSidebarWidth,
  sidebarUnitScale,
  sidebarWidthCss,
  SIDEBAR_HANDLE_WIDTH,
  SIDEBAR_KEYBOARD_STEP,
  useSidebarWidth,
  useSidebarWidthBounds,
} from "@/lib/sidebar-widths"

// EXP-1246: the ONE list-detail host (desktop `second_sidebar_for` parity).
// One list stays beside what it opens: the Inbox (both its lists; EXP-1293
// retired the Agent › Recent arm with the Agent page's history). #EXP-1672:
// the host is the content card's SECOND SIDEBAR (`SecondSidebarHost`, the
// team layout), next to the full main menu that never folds — the IDE's
// `render_with_side`. Kept mounted across the list's details, so walking it
// one detail at a time never remounts it — the selection opens to the right,
// the list stays put. On the list SCREEN itself (the md+ Inbox page) nothing
// is selected: the page draws `ListDetailEmpty`. Which list is up is the
// URL's (`secondSidebarFor`).

// The sidebar column's motion tokens (`sidebar.tsx` `SLOT_MOTION`): a drag
// (`data-resizing` on the sidebar wrapper) switches the width easing off.
const SLOT_MOTION = `duration-standard ease-standard motion-reduce:transition-none group-data-[resizing=true]/sidebar-wrapper:transition-none`

/** Whether the location is the list screen itself, not a detail beside it. */
function isListScreen(pathname: string, origin: DetailOrigin): boolean {
  const rest = pathname.replace(/^\/t\/[^/]+/, ``).replace(/\/$/, ``)
  return origin.kind === `inbox` && rest === `/inbox`
}

/** A stable key per list — the sidebar remounts the host only between lists. */
export function listDetailKey(origin: DetailOrigin): string {
  return origin.kind
}

/**
 * #EXP-1672: the content card's `[second sidebar | screen]` row, md+ — the
 * list at its dragged `list` width with a right hairline and the drag edge
 * on it; the slot is 0 wide (and empty) with no list, so the screen beside
 * it never remounts when the list comes or goes. Phones: the screen alone.
 */
export function SecondSidebarHost({
  teamSlug,
  children,
}: {
  teamSlug: string
  children: ReactNode
}) {
  const isMobile = useIsMobile()
  const origin = useSecondSidebar()
  const width = useSidebarWidth(`list`)
  const { min, max } = useSidebarWidthBounds(`list`)
  const open = !isMobile && origin !== null
  return (
    <div className="contents md:flex md:min-h-0 md:min-w-0 md:flex-1">
      <div
        data-testid="second-sidebar"
        className={cn(
          `relative hidden shrink-0 overflow-hidden transition-[width] md:flex`,
          SLOT_MOTION,
          open ? `border-r border-glass-stroke` : `w-0`
        )}
        style={open ? { width: sidebarWidthCss(width) } : undefined}
      >
        <div
          inert={!open}
          className="absolute inset-y-0 left-0 flex flex-col text-sidebar-foreground"
          style={{ width: sidebarWidthCss(width) }}
        >
          {open && (
            <ListDetailPane
              key={listDetailKey(origin)}
              teamSlug={teamSlug}
              origin={origin}
            />
          )}
        </div>
      </div>
      {open && (
        <div className="relative hidden md:block">
          <ResizeHandle
            aria-label="Resize sidebar"
            className="z-20"
            style={{
              left: -SIDEBAR_HANDLE_WIDTH / 2 - 0.5,
              width: SIDEBAR_HANDLE_WIDTH,
            }}
            value={width}
            min={min}
            max={max}
            step={SIDEBAR_KEYBOARD_STEP}
            scale={sidebarUnitScale()}
            onChange={(next) => setSidebarWidth(`list`, next, { persist: false })}
            onCommit={(next) => setSidebarWidth(`list`, next)}
            onReset={() => resetSidebarWidth(`list`)}
            onDraggingChange={setSidebarResizing}
          />
        </div>
      )}
      {children}
    </div>
  )
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

  const tab = origin.kind === `inbox` && origin.tab === `my-issues` ? `my-issues` : null
  return (
    <>
      {/* A plain column, not `SidebarContent`: each list below owns its own
          scrollport, and two nested `overflow-auto` boxes make the column
          scroll twice. Desktop parity: no title row — the list starts at
          its Inbox | My issues strip. */}
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
