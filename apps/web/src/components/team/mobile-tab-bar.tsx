import { Link, useMatchRoute, useParams } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { readLastVisited } from "@/lib/last-visited"
import { useChromeHeightVar } from "@/hooks/use-chrome-height-var"
import { useSession } from "@/hooks/use-session"
import {
  useUnreadNotificationCount,
  useUnreadSupportCount,
} from "@/hooks/use-unread-notifications"
import {
  useReviewsOpenPrCount,
  useAgentsRunningCount,
} from "@/hooks/use-nav-counts"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const ActionChatIcon = conceptIcon(`action-chat`)
const NavActionsIcon = conceptIcon(`nav-actions`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavIssuesIcon = conceptIcon(`nav-issues`)
const NavReviewsIcon = conceptIcon(`nav-reviews`)
const NavSupportIcon = conceptIcon(`nav-support`)

// Bottom padding for every scroll container that sits under the floating
// tab bar, so list ends scroll clear of the glass pill. Detail routes hide
// the bar (useMobileChromeVisible) and must NOT reserve this.
// EXP-698: the bar MEASURES itself into `--tabbar-h` (its own safe-area
// padding included) instead of every scroller re-guessing the pill geometry.
// The literal covers ONLY the frames before the first measurement — a hidden
// bar publishes `0px` rather than dropping the property, so no route ever
// falls back to it while there is no pill on screen. `+1.25rem` is the gap
// above the pill.
export const TAB_BAR_CLEARANCE = `max-md:pb-[calc(var(--tabbar-h,4.25rem)+1.25rem)]`

// Mobile chrome (topbar + tab bar) hides on the detail routes — they carry
// their own breadcrumb/back headers, mirroring the native apps pushing a
// bar-less detail screen. Settings and the other top-level surfaces keep it.
export function useMobileChromeVisible(): boolean {
  const matchRoute = useMatchRoute()
  const onIssueDetail = matchRoute({
    to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
    fuzzy: true,
  })
  const onReviewDetail = matchRoute({
    to: `/t/$teamSlug/reviews/$issueIdentifier`,
    fuzzy: true,
  })
  return !onIssueDetail && !onReviewDetail
}

// The board the Issues tab / compose FAB / topbar switcher target: the
// active board, else this device's last-used board in the team (EXP-69,
// same resolution as the team index route), else the first board.
export function resolveBoardTarget(
  teamSlug: string,
  boards: Board[] | undefined,
  activeBoardSlug: string | undefined
): Board | undefined {
  if (!boards || boards.length === 0) return undefined
  if (activeBoardSlug) {
    const active = boards.find((board) => board.slug === activeBoardSlug)
    if (active) return active
  }
  const last = readLastVisited()
  if (last?.teamSlug === teamSlug && last.boardSlug) {
    const remembered = boards.find((board) => board.slug === last.boardSlug)
    if (remembered) return remembered
  }
  return boards[0]
}

// Tiny unread-dot components fed by the per-user shapes. Native parity:
// dots, not counts.
function InboxDot() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return <TabDot className="bg-primary" />
}

function SupportDot({ teamId }: { teamId?: string }) {
  const unread = useUnreadSupportCount(teamId)
  if (unread === 0) return null
  return <TabDot className="bg-primary" />
}

// Review green (EXP-214): open PRs are "stuff to do", colored like the
// in_review issue status. green-500/yellow-400 match the natives'
// semantic tokens (EXP-699).
function ReviewsDot({ boards }: { boards: Board[] | undefined }) {
  const count = useReviewsOpenPrCount(boards)
  if (count === 0) return null
  return <TabDot className="bg-green-500" />
}

// Amber while any live session waits on a plan approval / question
// (EXP-214), live green otherwise.
function DevicesDot({ teamId }: { teamId?: string }) {
  const { data: session } = useSession()
  const { count, needsInput } = useAgentsRunningCount(teamId, session?.user?.id)
  if (count === 0) return null
  return <TabDot className={needsInput ? `bg-yellow-400` : `bg-green-500`} />
}

function TabDot({ className }: { className: string }) {
  return (
    <span
      className={cn(
        `pointer-events-none absolute right-2 top-2 size-2 rounded-full`,
        className
      )}
    />
  )
}

// The detached circular FAB beside the nav pill — one slot, whatever the
// active surface puts in it.
const FAB_CLASS = `pointer-events-auto flex size-[3.25rem] shrink-0 items-center justify-center rounded-full border border-glass-stroke-card bg-popover/85 text-foreground shadow-lg shadow-black/40 backdrop-blur-xl`

function tabClass(active: boolean): string {
  return cn(
    `relative flex size-11 items-center justify-center rounded-full transition-colors`,
    active ? `bg-glass-active text-foreground` : `text-muted-foreground`
  )
}

interface MobileTabBarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
}

// Native-parity mobile navigation (EXP-189): a floating glass pill with the
// top-level destinations plus a detached compose FAB, replacing the old
// sidebar-as-drawer. Desktop keeps the persistent sidebar (`md:hidden`).
// EXP-686: Issues, Inbox, Support, Devices, Actions, Reviews — Search left
// the bar for the board header (`use-issue-search.tsx`) to make room.
export function MobileTabBar({
  teamSlug,
  team,
  boards,
}: MobileTabBarProps) {
  const matchRoute = useMatchRoute()
  const visible = useMobileChromeVisible()
  const { boardSlug } = useParams({ strict: false })
  // EXP-698: what TAB_BAR_CLEARANCE spends. `0px` on the detail routes that
  // hide the bar (never removed — the literal fallback would reserve a
  // phantom pill there), and 0 on md+ where the element is `display:none`.
  const publishTabBarHeight = useChromeHeightVar(`--tabbar-h`)

  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)

  const onBoard = Boolean(
    matchRoute({ to: `/t/$teamSlug/boards/$boardSlug`, fuzzy: true })
  )
  const onTeamIndex = Boolean(matchRoute({ to: `/t/$teamSlug` }))
  const onInbox = Boolean(matchRoute({ to: `/t/$teamSlug/inbox`, fuzzy: true }))
  const onDevices = Boolean(
    matchRoute({ to: `/t/$teamSlug/devices`, fuzzy: true })
  )
  // EXP-686: `/automations` replace-navigates into the Actions page's
  // Automations tab on mobile, but a slow network can render this bar first.
  const onActions = Boolean(
    matchRoute({ to: `/t/$teamSlug/actions`, fuzzy: true }) ||
      matchRoute({ to: `/t/$teamSlug/automations`, fuzzy: true })
  )
  const onReviews = Boolean(
    matchRoute({ to: `/t/$teamSlug/reviews`, fuzzy: true })
  )
  const onSupport = Boolean(
    matchRoute({ to: `/t/$teamSlug/support`, fuzzy: true })
  )

  if (!visible) return null

  return (
    <div
      ref={publishTabBarHeight}
      className="pointer-events-none fixed inset-x-0 bottom-0 z-[35] flex items-center justify-center gap-3 px-4 pb-[max(1rem,env(safe-area-inset-bottom))] md:hidden"
    >
      <nav
        aria-label="Primary"
        className="pointer-events-auto flex items-center rounded-full border border-glass-stroke-strong bg-glass-card-opaque p-1"
      >
        {boardTarget ? (
          <Link
            to="/t/$teamSlug/boards/$boardSlug"
            params={{ teamSlug, boardSlug: boardTarget.slug }}
            aria-label="Issues"
            className={tabClass(onBoard || onTeamIndex)}
          >
            <NavIssuesIcon className="size-5" />
          </Link>
        ) : (
          <Link
            to="/t/$teamSlug"
            params={{ teamSlug }}
            aria-label="Issues"
            className={tabClass(onBoard || onTeamIndex)}
          >
            <NavIssuesIcon className="size-5" />
          </Link>
        )}
        <Link
          to="/t/$teamSlug/inbox"
          params={{ teamSlug }}
          aria-label="Inbox"
          className={tabClass(onInbox)}
        >
          <NavInboxIcon className="size-5" />
          <InboxDot />
        </Link>
        {team?.helpdeskEnabled === true && (
          <Link
            to="/t/$teamSlug/support"
            params={{ teamSlug }}
            aria-label="Support"
            className={tabClass(onSupport)}
          >
            <NavSupportIcon className="size-5" />
            <SupportDot teamId={team?.id} />
          </Link>
        )}
        <Link
          to="/t/$teamSlug/devices"
          params={{ teamSlug }}
          aria-label="Devices"
          className={tabClass(onDevices)}
        >
          <NavDevicesIcon className="size-5" />
          <DevicesDot teamId={team?.id} />
        </Link>
        <Link
          to="/t/$teamSlug/actions"
          params={{ teamSlug }}
          aria-label="Actions"
          className={tabClass(onActions)}
        >
          <NavActionsIcon className="size-5" />
        </Link>
        <Link
          to="/t/$teamSlug/reviews"
          params={{ teamSlug }}
          aria-label="Reviews"
          className={tabClass(onReviews)}
        >
          <NavReviewsIcon className="size-5" />
          <ReviewsDot boards={boards} />
        </Link>
      </nav>
      {/* EXP-631: the Devices tab's FAB slot starts a chat instead of an
          issue — the same launcher the device rows open, on its Chat tab
          (native parity: iOS/Android hide compose on Devices too).
          EXP-694: the Actions tab gets the same chat FAB — there is no issue
          to compose there either, and every client offers chat from both. */}
      {onDevices ? (
        <Link
          to="/t/$teamSlug/devices"
          params={{ teamSlug }}
          search={{ chat: 1 }}
          aria-label="Start chat"
          className={FAB_CLASS}
        >
          <ActionChatIcon className="size-5" />
        </Link>
      ) : onActions ? (
        <Link
          to="/t/$teamSlug/actions"
          params={{ teamSlug }}
          search={{ chat: 1 }}
          aria-label="Start chat"
          className={FAB_CLASS}
        >
          <ActionChatIcon className="size-5" />
        </Link>
      ) : (
        boardTarget && (
          <Link
            to="/t/$teamSlug/boards/$boardSlug"
            params={{ teamSlug, boardSlug: boardTarget.slug }}
            search={{ new: 1 }}
            aria-label="New issue"
            className={FAB_CLASS}
          >
            <NavCreateIssueIcon className="size-5" />
          </Link>
        )
      )}
    </div>
  )
}
