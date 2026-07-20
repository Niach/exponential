import { Link, useMatchRoute, useParams } from "@tanstack/react-router"
import {
  Bot,
  GitPullRequest,
  Inbox,
  LifeBuoy,
  List,
  Search,
  SquarePen,
} from "lucide-react"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { readLastVisited } from "@/lib/last-visited"
import { useSession } from "@/hooks/use-session"
import {
  useUnreadNotificationCount,
  useUnreadSupportCount,
} from "@/hooks/use-unread-notifications"
import {
  useReviewsOpenPrCount,
  useAgentsRunningCount,
} from "@/hooks/use-nav-counts"

// Bottom padding for every scroll container that sits under the floating
// tab bar, so list ends scroll clear of the glass pill. Detail routes hide
// the bar (useMobileChromeVisible) and must NOT reserve this.
export const TAB_BAR_CLEARANCE = `max-md:pb-[calc(5.5rem+env(safe-area-inset-bottom))]`

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
    const remembered = boards.find(
      (board) => board.slug === last.boardSlug
    )
    if (remembered) return remembered
  }
  return boards[0]
}

// Tiny authed-only dot components so the per-user shapes (requireAuth)
// aren't subscribed for anonymous viewers — same caveat as the sidebar
// badges. Native parity: dots, not counts.
function InboxDot() {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return <TabDot className="bg-indigo-400" />
}

function SupportDot({ teamId }: { teamId?: string }) {
  const unread = useUnreadSupportCount(teamId)
  if (unread === 0) return null
  return <TabDot className="bg-indigo-400" />
}

// Review green (EXP-214): open PRs are "stuff to do", colored like the
// in_review issue status.
function ReviewsDot({ boards }: { boards: Board[] | undefined }) {
  const count = useReviewsOpenPrCount(boards)
  if (count === 0) return null
  return <TabDot className="bg-emerald-500" />
}

// Amber while any live session waits on a plan approval / question
// (EXP-214), live green otherwise.
function AgentsDot({ teamId }: { teamId?: string }) {
  const { count, needsInput } = useAgentsRunningCount(teamId)
  if (count === 0) return null
  return (
    <TabDot className={needsInput ? `bg-amber-500` : `bg-emerald-500`} />
  )
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

function tabClass(active: boolean): string {
  return cn(
    `relative flex size-11 items-center justify-center rounded-full transition-colors`,
    active ? `bg-white/10 text-foreground` : `text-muted-foreground`
  )
}

interface MobileTabBarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  onOpenSearch: () => void
}

// Native-parity mobile navigation (EXP-189): a floating glass pill with the
// top-level destinations plus a detached compose FAB, replacing the old
// sidebar-as-drawer. Desktop keeps the persistent sidebar (`md:hidden`).
export function MobileTabBar({
  teamSlug,
  team,
  boards,
  onOpenSearch,
}: MobileTabBarProps) {
  const { data: session } = useSession()
  const isAuthed = Boolean(session?.user)
  const matchRoute = useMatchRoute()
  const visible = useMobileChromeVisible()
  const { boardSlug } = useParams({ strict: false })

  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)

  const onBoard = Boolean(
    matchRoute({ to: `/t/$teamSlug/boards/$boardSlug`, fuzzy: true })
  )
  const onTeamIndex = Boolean(matchRoute({ to: `/t/$teamSlug` }))
  const onInbox = Boolean(
    matchRoute({ to: `/t/$teamSlug/inbox`, fuzzy: true })
  )
  const onAgents = Boolean(
    matchRoute({ to: `/t/$teamSlug/agents`, fuzzy: true })
  )
  const onReviews = Boolean(
    matchRoute({ to: `/t/$teamSlug/reviews`, fuzzy: true })
  )
  const onSupport = Boolean(
    matchRoute({ to: `/t/$teamSlug/support`, fuzzy: true })
  )

  if (!visible) return null

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-0 z-[35] flex items-center justify-center gap-3 px-4 pb-[max(1rem,env(safe-area-inset-bottom))] md:hidden">
      <nav
        aria-label="Primary"
        className="pointer-events-auto flex items-center rounded-full border border-white/10 bg-zinc-900/85 p-1 shadow-lg shadow-black/40 backdrop-blur-xl"
      >
        {boardTarget ? (
          <Link
            to="/t/$teamSlug/boards/$boardSlug"
            params={{ teamSlug, boardSlug: boardTarget.slug }}
            aria-label="Issues"
            className={tabClass(onBoard || onTeamIndex)}
          >
            <List className="size-5" />
          </Link>
        ) : (
          <Link
            to="/t/$teamSlug"
            params={{ teamSlug }}
            aria-label="Issues"
            className={tabClass(onBoard || onTeamIndex)}
          >
            <List className="size-5" />
          </Link>
        )}
        {isAuthed && (
          <Link
            to="/t/$teamSlug/inbox"
            params={{ teamSlug }}
            aria-label="Inbox"
            className={tabClass(onInbox)}
          >
            <Inbox className="size-5" />
            <InboxDot />
          </Link>
        )}
        {isAuthed && team?.helpdeskEnabled === true && (
          <Link
            to="/t/$teamSlug/support"
            params={{ teamSlug }}
            aria-label="Support"
            className={tabClass(onSupport)}
          >
            <LifeBuoy className="size-5" />
            <SupportDot teamId={team?.id} />
          </Link>
        )}
        {isAuthed && (
          <Link
            to="/t/$teamSlug/agents"
            params={{ teamSlug }}
            aria-label="Agents"
            className={tabClass(onAgents)}
          >
            <Bot className="size-5" />
            <AgentsDot teamId={team?.id} />
          </Link>
        )}
        {isAuthed && (
          <Link
            to="/t/$teamSlug/reviews"
            params={{ teamSlug }}
            aria-label="Reviews"
            className={tabClass(onReviews)}
          >
            <GitPullRequest className="size-5" />
            <ReviewsDot boards={boards} />
          </Link>
        )}
        {team && (
          <button
            type="button"
            onClick={onOpenSearch}
            aria-label="Search"
            className={tabClass(false)}
          >
            <Search className="size-5" />
          </button>
        )}
      </nav>
      {isAuthed && boardTarget && (
        <Link
          to="/t/$teamSlug/boards/$boardSlug"
          params={{ teamSlug, boardSlug: boardTarget.slug }}
          search={{ new: 1 }}
          aria-label="New issue"
          className="pointer-events-auto flex size-[3.25rem] shrink-0 items-center justify-center rounded-full border border-white/10 bg-zinc-900/85 text-foreground shadow-lg shadow-black/40 backdrop-blur-xl"
        >
          <SquarePen className="size-5" />
        </Link>
      )}
    </div>
  )
}
