import { Link, useMatchRoute, useParams } from "@tanstack/react-router"
import { Button, FAB_CHROME_CLASS, conceptIcon } from "@exp/ui"
import type { Board, Team } from "@/db/schema"
import { cn } from "@/lib/utils"
import { readLastVisited } from "@/lib/last-visited"
import { useChromeHeightVar } from "@/hooks/use-chrome-height-var"
import { useMobileChrome } from "@/hooks/use-mobile-chrome"
import { useOpenNewDraft } from "@/hooks/use-open-new-draft"
import { useSession } from "@/hooks/use-session"
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import {
  useReviewsNav,
  useAgentsRunningCount,
} from "@/hooks/use-nav-counts"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { ACTIONS_LABEL } from "@/components/team/sidebar-nav-entries"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const ActionChatIcon = conceptIcon(`action-chat`)
const NavCreateIssueIcon = conceptIcon(`nav-create-issue`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavIssuesIcon = conceptIcon(`nav-issues`)
const NavReviewsIcon = conceptIcon(`nav-reviews`)
const NavActionsIcon = conceptIcon(`nav-actions`)

// Bottom padding for every scroll container that sits under the floating
// tab bar, so list ends scroll clear of the glass pill. Detail routes hide
// the bar (useMobileChromeVisible) and must NOT reserve this.
// EXP-698: the bar MEASURES itself into `--tabbar-h` (its own safe-area
// padding included) instead of every scroller re-guessing the pill geometry.
// The literal covers ONLY the frames before the first measurement — a hidden
// bar publishes `0px` rather than dropping the property, so no route ever
// falls back to it while there is no pill on screen. `+1.25rem` is the gap
// above the pill.
// EXP-698 r5: the bulk bar takes the tab bar's slot on a selection, so the
// clearance is whichever bar is actually down there — `max()` of the two
// published heights, never their sum.
export const TAB_BAR_CLEARANCE = `max-md:pb-[calc(max(var(--tabbar-h,4.25rem),var(--bulkbar-h,0px))+1.25rem)]`

// Mobile chrome (topbar + tab bar) hides on the detail routes — they carry
// their own breadcrumb/back headers, mirroring the native apps pushing a
// bar-less detail screen. Settings and the other top-level surfaces keep it.
export function useMobileChromeVisible(): boolean {
  const matchRoute = useMatchRoute()
  const onIssueDetail = matchRoute({
    to: `/t/$teamSlug/boards/$boardSlug/issues/$issueIdentifier`,
    fuzzy: true,
  })
  // EXP-740/EXP-739: the two session pages are full-screen steering views
  // with their own back header — the native apps push them bar-less too.
  const onSessionDetail = matchRoute({
    to: `/t/$teamSlug/sessions/$sessionId`,
    fuzzy: true,
  })
  // EXP-1170: the New issue page is the issue detail in draft mode.
  const onDraftPage = matchRoute({
    to: `/t/$teamSlug/drafts/$draftId`,
    fuzzy: true,
  })
  // SLOP-2: one action's page is a detail with its own back header.
  const onActionDetail = matchRoute({
    to: `/t/$teamSlug/actions/$actionId`,
    fuzzy: true,
  })
  // EXP-851: the Agent page is a LIST screen again (composer over Running /
  // Recent in one scroller), so it keeps the standard phone chrome.
  return (
    !onIssueDetail &&
    !onSessionDetail &&
    !onActionDetail &&
    !onDraftPage
  )
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

// Review green (EXP-214): open PRs are "stuff to do", colored like the
// in_review issue status. green-500/yellow-400 match the natives'
// semantic tokens (EXP-699). EXP-1186: across every member team — the
// phone's Reviews is cross-team like the inbox.
function ReviewsDot({ dot }: { dot: boolean }) {
  if (!dot) return null
  return <TabDot className="bg-green-500" />
}

// Any of MY live coding sessions, on the Chat launcher — the Agent page holds
// the sessions list (EXP-818), so the dot rides its button. Amber while one
// waits on a plan approval / question (EXP-214). EXP-1186: in ANY member team
// on the phone, matching the Agent page's cross-team Running list.
function AgentDot({ teamIds }: { teamIds: readonly string[] }) {
  const { data: session } = useSession()
  const { count, needsInput } = useAgentsRunningCount(teamIds, session?.user?.id)
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

// The detached FAB beside the nav pill: ONE 52px capsule with two 52px arms
// — Start chat | New issue — split by a hairline (`FAB_GROUP_CLASS` +
// `FAB_ARM_CLASS`). The capsule is the circle STRETCHED, so it says 52px the
// same way the circle does, never `3.25rem`.
// EXP-973: it is the SAME capsule on every tab-bar route. EXP-827 drew it
// only on a board and left a lone chat circle everywhere else, so filing a
// bug from Devices or Reviews meant a trip back to a board first. With no
// board in the team at all there is nothing to file into — the arm then dims
// (disabled, "New issue (no board)") instead of disappearing, so the control
// never moves under the thumb. Same shape ×3 (iOS / Android `composeEnabled`).
// The Agent page is the phone's FIRST screen and keeps the bar, so the chat
// arm is SELECTED there — the tabs' active circle inside the arm — the way a
// tab is lit on its own surface.
const FAB_GROUP_CLASS = `pointer-events-auto flex h-[52px] shrink-0 items-stretch overflow-hidden rounded-full text-foreground ${FAB_CHROME_CLASS}`
const FAB_ARM_CLASS = `relative flex w-[52px] items-center justify-center text-foreground transition-colors active:bg-glass-active`

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
// EXP-686: Search left the bar for the board header (`use-issue-search.tsx`).
// EXP-1187: Issues · Inbox · Devices · Reviews · Actions — no More on the
// phone: Actions is a tab of its own and Settings lives in the topbar's
// avatar menu (×3 with the iOS and Android bars). The phone OPENS on the
// Agent page (`routes/index.tsx`), whose entry is the FAB's chat arm.
export function MobileTabBar({
  teamSlug,
  team,
  boards,
}: MobileTabBarProps) {
  const matchRoute = useMatchRoute()
  // EXP-698 r5: a multi-selection replaces the bar with the bulk bar (native
  // parity) — one occupant of the bottom edge, FAB included.
  const { bulkBarPresent } = useMobileChrome()
  const visible = useMobileChromeVisible() && !bulkBarPresent
  const { boardSlug } = useParams({ strict: false })
  // EXP-698: what TAB_BAR_CLEARANCE spends. `0px` on the detail routes that
  // hide the bar (never removed — the literal fallback would reserve a
  // phantom pill there), and 0 on md+ where the element is `display:none`.
  const publishTabBarHeight = useChromeHeightVar(`--tabbar-h`)

  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  // EXP-1186: only Issues is team-scoped on the phone; Reviews and the
  // Agent dot read every member team (md+ = the active team, the bar is
  // hidden there anyway).
  const scope = useCrossTeamScope(team)
  const reviewsNav = useReviewsNav(scope.teams)
  const showsReviews = reviewsNav.shows
  const openNewDraft = useOpenNewDraft(teamSlug)

  const onBoard = Boolean(
    matchRoute({ to: `/t/$teamSlug/boards/$boardSlug`, fuzzy: true })
  )
  const onTeamIndex = Boolean(matchRoute({ to: `/t/$teamSlug` }))
  const onAgent = Boolean(matchRoute({ to: `/t/$teamSlug/agent`, fuzzy: true }))
  const onInbox = Boolean(matchRoute({ to: `/t/$teamSlug/inbox`, fuzzy: true }))
  const onDevices = Boolean(
    matchRoute({ to: `/t/$teamSlug/devices`, fuzzy: true })
  )
  // EXP-1187: lit on the list AND on one action's page (SLOP-2).
  const onActions = Boolean(
    matchRoute({ to: `/t/$teamSlug/actions`, fuzzy: true })
  )
  const onReviews = Boolean(
    matchRoute({ to: `/t/$teamSlug/reviews`, fuzzy: true })
  )
  // EXP-973: the New-issue arm is always DRAWN; it only goes dead when the
  // team has no board to file into. The arm targets the current board when
  // there is one, else the team's remembered/first board
  // (`resolveBoardTarget`). Same predicate as iOS / Android `composeEnabled`.
  const composeEnabled = boardTarget !== undefined

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
        <Link
          to="/t/$teamSlug/devices"
          params={{ teamSlug }}
          aria-label="Devices"
          className={tabClass(onDevices)}
        >
          <NavDevicesIcon className="size-5" />
        </Link>
        {showsReviews && (
          <Link
            to="/t/$teamSlug/reviews"
            params={{ teamSlug }}
            aria-label="Reviews"
            className={tabClass(onReviews)}
          >
            <NavReviewsIcon className="size-5" />
            <ReviewsDot dot={reviewsNav.dot} />
          </Link>
        )}
        {/* EXP-1187: Actions is a tab; Drafts stay the Inbox's third
            segment on the phone (EXP-878), Settings the avatar menu's. */}
        <Link
          to="/t/$teamSlug/actions"
          params={{ teamSlug }}
          aria-label={ACTIONS_LABEL}
          data-testid="tab-actions"
          className={tabClass(onActions)}
        >
          <NavActionsIcon className="size-5" />
        </Link>
      </nav>
      {/* EXP-631/694: the chat launcher started on Devices and Actions,
          EXP-739 made it a LINK to the team's Agent page, EXP-827 merged it
          with New issue on a board (×3 mobile), and EXP-973 made that one
          capsule the whole story — it rides every top-level surface, live dot
          and all, with the New-issue arm dimmed only when the team has no
          board. */}
      <div className={FAB_GROUP_CLASS} data-testid="fab-group">
        <Link
          to="/t/$teamSlug/agent"
          params={{ teamSlug }}
          aria-label="Start chat"
          data-testid="chat-button"
          aria-current={onAgent ? `page` : undefined}
          className={FAB_ARM_CLASS}
        >
          <span
            className={cn(
              `flex size-11 items-center justify-center rounded-full transition-colors`,
              onAgent && `bg-glass-active`
            )}
          >
            <ActionChatIcon className="size-5" />
          </span>
          <AgentDot teamIds={scope.teamIds} />
        </Link>
        <span aria-hidden className="my-3 w-px shrink-0 bg-glass-stroke-card" />
        {composeEnabled ? (
          // EXP-1170: a fresh draft id at TAP time, then the New issue page.
          <Button
            variant="ghost"
            aria-label="New issue"
            data-testid="compose-button"
            className={cn(FAB_ARM_CLASS, `h-auto rounded-none px-0`)}
            onClick={() => openNewDraft({ boardId: boardTarget.id })}
          >
            <NavCreateIssueIcon className="size-5" />
          </Button>
        ) : (
          <span
            role="button"
            aria-disabled
            aria-label="New issue (no board)"
            data-testid="compose-button"
            className={cn(
              FAB_ARM_CLASS,
              `pointer-events-none text-muted-foreground opacity-50`
            )}
          >
            <NavCreateIssueIcon className="size-5" />
          </span>
        )}
      </div>
    </div>
  )
}
