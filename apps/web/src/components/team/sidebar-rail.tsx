import type * as React from "react"
import { Link, useNavigate, useRouterState } from "@tanstack/react-router"
import {
  conceptIcon,
  getBoardIcon,
  Badge,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Separator,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  UserAvatar as UserAvatarView,
} from "@exp/ui"
import { isAdminUser } from "@/lib/auth/app-user"
import { cn } from "@/lib/utils"
import type { Board, Team } from "@/db/schema"
import { useSession } from "@/hooks/use-session"
import { useSignOut } from "@/hooks/use-sign-out"
import {
  useUnreadNotificationCount,
  useUnreadSupportCount,
} from "@/hooks/use-unread-notifications"
import {
  useAgentsRunningCount,
  useReviewsOpenPrCount,
} from "@/hooks/use-nav-counts"
import { useDraftEntries } from "@/hooks/use-issue-drafts"
import { WORKFLOWS_TITLE } from "@/lib/workflow-view"
import { SidebarPinnedIcons } from "@/components/team/sidebar-pinned"
import { SidebarRunningIcons } from "@/components/team/sidebar-running"

// EXP-870: the rail never leaves. Beside a list nav or the settings nav the
// main menu MORPHS into this 48px icon column instead of sliding away —
// Inbox, Agent, the boards, the pins and the account stay one click away in
// every state (desktop `compact_rail.rs`). Strictly derived from the URL
// (`sidebarOccupant`), never a user toggle. 32px ghost buttons centred in
// the column, the name in a right-hand tooltip, badges top-right.
//
// The badges live here and are shared with the expanded rows in
// `sidebar.tsx`, so both states read one set of counts.

const NavAboutIcon = conceptIcon(`settings-about`)
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavActionsIcon = conceptIcon(`nav-actions`)
const NavAgentIcon = conceptIcon(`action-chat`)
const NavAutomationsIcon = conceptIcon(`nav-automations`)
const NavWorkflowsIcon = conceptIcon(`nav-workflows`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavDraftsIcon = conceptIcon(`nav-drafts`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavReviewsIcon = conceptIcon(`nav-reviews`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const NavSignOutIcon = conceptIcon(`nav-sign-out`)
const NavSupportIcon = conceptIcon(`nav-support`)

export type BadgePlacement = `row` | `icon`

// EXP-699: nav badges are dots — same colors and logic as the mobile tab
// bars. A row carries it at the right edge, an icon at its top-right corner.
function NavDot({
  className,
  placement,
}: {
  className: string
  placement: BadgePlacement
}) {
  return (
    <span
      className={cn(
        `pointer-events-none absolute size-2 rounded-full`,
        placement === `row`
          ? `right-2 top-1/2 -translate-y-1/2`
          : `right-1 top-1`,
        className
      )}
    />
  )
}

/** EXP-1075: live runs of MINE in ANOTHER team — the team picker's dot
 * (`lib/sessions/team-live-runs.ts` decides where it hangs). Same tones as
 * `AgentRunningBadge`: amber while one of those runs waits on the person,
 * green otherwise. A dot, never a count — the picker only says "look over
 * there"; the team you are in counts its runs in the Running section.
 * `corner` hangs it off an icon (the switcher chevron), `trailing` sits at
 * the right edge of a team row. */
export function TeamLiveDot({
  live,
  placement,
  className,
  ...rest
}: {
  /** Absent = nothing of mine is live there, and the dot does not render. */
  live: { needsInput: boolean } | undefined | null
  placement: `corner` | `trailing`
} & React.ComponentProps<`span`>) {
  if (!live) return null
  return (
    <span
      className={cn(
        // Hit-testable on purpose (unlike `NavDot`): the `title` is the only
        // thing that says WHY the dot is there, and a click still bubbles to
        // the row or trigger underneath.
        `size-2 shrink-0 rounded-full`,
        live.needsInput ? `bg-yellow-400` : `bg-green-500`,
        placement === `corner`
          ? `absolute -right-0.5 -top-0.5 ring-2 ring-sidebar`
          : `ml-auto`,
        className
      )}
      {...rest}
    />
  )
}

/** Unread notifications from the per-user shape. */
export function InboxUnreadBadge({ placement }: { placement: BadgePlacement }) {
  const unread = useUnreadNotificationCount()
  if (unread === 0) return null
  return <NavDot className="bg-primary" placement={placement} />
}

/** EXP-878: how many drafts the caller is keeping in this team — a NEUTRAL
 *  count, not an alert: a draft is work you parked, not work waiting on you.
 *  Zero renders nothing, and the entry itself is hidden at zero. */
export function DraftsCountBadge({
  teamId,
  placement,
}: {
  teamId?: string
  placement: BadgePlacement
}) {
  const count = useDraftEntries(teamId).length
  // EXP-962: `Badge` owns the shape, the zero and the 99+ cap; the rail owns
  // only where it hangs.
  return (
    <Badge
      count={count}
      data-testid="drafts-count-badge"
      className={cn(
        `absolute`,
        placement === `row`
          ? `right-2 top-1/2 -translate-y-1/2`
          : `-right-0.5 -top-0.5`
      )}
    />
  )
}

/** Unread helpdesk activity in THIS team, for the Support entry. */
export function SupportUnreadBadge({
  teamId,
  placement,
}: {
  teamId?: string
  placement: BadgePlacement
}) {
  const unread = useUnreadSupportCount(teamId)
  if (unread === 0) return null
  return <NavDot className="bg-primary" placement={placement} />
}

/** Any open PR across the team's boards. */
export function ReviewsOpenBadge({
  boards,
  teamId,
  placement,
}: {
  boards: Board[] | undefined
  teamId?: string
  placement: BadgePlacement
}) {
  const count = useReviewsOpenPrCount(boards, teamId)
  if (count === 0) return null
  return <NavDot className="bg-green-500" placement={placement} />
}

/** My live runs in the team (`useMyLiveRuns`) — the Agent entry's dot (EXP-880:
 *  no count). Amber while a run waits on the person, green otherwise. */
export function AgentRunningBadge({
  teamId,
  placement,
}: {
  teamId?: string
  placement: BadgePlacement
}) {
  const { data: session } = useSession()
  const { count, needsInput } = useAgentsRunningCount(teamId, session?.user?.id)
  if (count === 0) return null
  return (
    <NavDot
      className={needsInput ? `bg-yellow-400` : `bg-green-500`}
      placement={placement}
    />
  )
}

/** The account menu's items — one list for the expanded footer and the
 *  compact rail's avatar. */
export function UserMenuItems({ onWhatsNew }: { onWhatsNew: () => void }) {
  const { data: session } = useSession()
  const navigate = useNavigate()
  const handleSignOut = useSignOut()
  return (
    <>
      {isAdminUser(session?.user) && (
        <DropdownMenuItem onClick={() => navigate({ to: `/admin` })}>
          <NavAdminIcon className="mr-2 h-4 w-4" />
          Admin
        </DropdownMenuItem>
      )}
      {/* Re-entry point once the footer card is dismissed. */}
      <DropdownMenuItem onClick={onWhatsNew}>
        <NavChangelogIcon className="mr-2 h-4 w-4" />
        What&apos;s new
      </DropdownMenuItem>
      {/* EXP-262: version-less About page with the third-party licence
          notices. */}
      <DropdownMenuItem onClick={() => navigate({ to: `/about` })}>
        <NavAboutIcon className="mr-2 h-4 w-4" />
        About
      </DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem onClick={handleSignOut}>
        <NavSignOutIcon className="mr-2 h-4 w-4" />
        Sign out
      </DropdownMenuItem>
    </>
  )
}

/** The signed-in user's avatar — the @exp/ui composition bound to the session.
 *  Name-less accounts (Apple sign-in) fall back to the email for initials
 *  instead of a bare "?", which the primitive does for every caller. */
export function UserAvatar({ className }: { className?: string }) {
  const { data: session } = useSession()
  return <UserAvatarView user={session?.user} className={className} />
}

const RAIL_BUTTON = cn(
  `relative size-8 shrink-0 text-sidebar-foreground/80 hover:bg-sidebar-accent hover:text-sidebar-accent-foreground`,
  `data-[status=active]:bg-sidebar-accent data-[status=active]:text-sidebar-accent-foreground`
)

/** One 32px rail button with its tooltip. `link` renders a router Link (its
 *  own `data-status=active` drives the fill); otherwise a plain button. */
function RailItem({
  label,
  active,
  link,
  onClick,
  className,
  children,
}: {
  label: string
  active?: boolean
  link?: { to: string; params: Record<string, string> }
  onClick?: () => void
  className?: string
  children: React.ReactNode
}) {
  const classes = cn(
    RAIL_BUTTON,
    active && `bg-sidebar-accent text-sidebar-accent-foreground`,
    className
  )
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        {link ? (
          <Button asChild variant="ghost" size="icon" className={classes}>
            <Link
              to={link.to as never}
              params={link.params as never}
              aria-label={label}
            >
              {children}
            </Link>
          </Button>
        ) : (
          <Button
            variant="ghost"
            size="icon"
            className={classes}
            aria-label={label}
            onClick={onClick}
          >
            {children}
          </Button>
        )}
      </TooltipTrigger>
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  )
}

/** The rail's Drafts button — present only while there IS a draft, directly
 *  after Inbox in both rail states. */
function DraftsRailItem({
  teamId,
  params,
}: {
  teamId?: string
  params: Record<string, string>
}) {
  const count = useDraftEntries(teamId).length
  if (count === 0) return null
  return (
    <RailItem label="Drafts" link={{ to: `/t/$teamSlug/drafts`, params }}>
      <NavDraftsIcon className="size-4" />
      <DraftsCountBadge teamId={teamId} placement="icon" />
    </RailItem>
  )
}

export function TeamSidebarRail({
  teamSlug,
  team,
  boards,
  onWhatsNew,
}: {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  onWhatsNew: () => void
}) {
  const params = { teamSlug }
  const { data: session } = useSession()
  // EXP-862: while a pinned action seeds the composer, its pin owns the
  // highlight — the Agent icon must not claim the page too.
  const composerSeeded = useRouterState({
    select: (s) => {
      const value = (s.location.search as { action?: unknown }).action
      return (
        s.location.pathname.endsWith(`/agent`) &&
        typeof value === `string` &&
        value !== ``
      )
    },
  })

  return (
    <div className="flex h-full w-12 flex-col" data-testid="sidebar-compact-rail">
      <div className="flex min-h-0 flex-1 flex-col items-center gap-1 overflow-x-hidden overflow-y-auto py-2 [scrollbar-width:none]">
        <RailItem label="Inbox" link={{ to: `/t/$teamSlug/inbox`, params }}>
          <NavInboxIcon className="size-4" />
          <InboxUnreadBadge placement="icon" />
        </RailItem>
        <DraftsRailItem teamId={team?.id} params={params} />
        {team?.helpdeskEnabled === true && (
          <RailItem label="Support" link={{ to: `/t/$teamSlug/support`, params }}>
            <NavSupportIcon className="size-4" />
            <SupportUnreadBadge teamId={team.id} placement="icon" />
          </RailItem>
        )}
        <RailItem label="Devices" link={{ to: `/t/$teamSlug/devices`, params }}>
          <NavDevicesIcon className="size-4" />
        </RailItem>
        <RailItem label="Actions" link={{ to: `/t/$teamSlug/actions`, params }}>
          <NavActionsIcon className="size-4" />
        </RailItem>
        <RailItem
          label="Automations"
          link={{ to: `/t/$teamSlug/automations`, params }}
        >
          <NavAutomationsIcon className="size-4" />
        </RailItem>
        {/* EXP-981: directly after Automations, like the expanded sidebar. */}
        <RailItem
          label={WORKFLOWS_TITLE}
          link={{ to: `/t/$teamSlug/workflows`, params }}
        >
          <NavWorkflowsIcon className="size-4" />
        </RailItem>
        <RailItem label="Reviews" link={{ to: `/t/$teamSlug/reviews`, params }}>
          <NavReviewsIcon className="size-4" />
          <ReviewsOpenBadge boards={boards} teamId={team?.id} placement="icon" />
        </RailItem>
        <RailItem
          label="Agent"
          link={{ to: `/t/$teamSlug/agent`, params }}
          className={cn(
            composerSeeded &&
              `data-[status=active]:bg-transparent data-[status=active]:text-sidebar-foreground/80`
          )}
        >
          <NavAgentIcon className="size-4" />
          <AgentRunningBadge teamId={team?.id} placement="icon" />
        </RailItem>

        {team && (
          <PinnedIconGroup teamId={team.id} teamSlug={teamSlug} />
        )}

        {boards && boards.length > 0 && (
          <>
            <Separator className="my-1 w-6! shrink-0" />
            {boards.map((board) => {
              const BoardIcon = getBoardIcon(board)
              return (
                <RailItem
                  key={board.id}
                  label={board.name}
                  link={{
                    to: `/t/$teamSlug/boards/$boardSlug`,
                    params: { teamSlug, boardSlug: board.slug },
                  }}
                >
                  <BoardIcon className="size-4" style={{ color: board.color }} />
                </RailItem>
              )
            })}
          </>
        )}

        {/* EXP-923: the live runs, under the boards — brand mark plus the
            amber badge, the expanded row's whole label in the tooltip. */}
        <SidebarRunningIcons
          teamId={team?.id}
          currentUserId={session?.user?.id}
          separator={<Separator className="my-1 w-6! shrink-0" />}
          renderItem={(item) => (
            <RailItem
              key={item.key}
              label={item.label}
              active={item.active}
              onClick={item.onClick}
            >
              {item.icon}
            </RailItem>
          )}
        />
      </div>

      <div className="flex shrink-0 flex-col items-center gap-1 py-2">
        <DropdownMenu>
          <Tooltip>
            <TooltipTrigger asChild>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="icon"
                  className={RAIL_BUTTON}
                  aria-label="User menu"
                >
                  <UserAvatar />
                </Button>
              </DropdownMenuTrigger>
            </TooltipTrigger>
            <TooltipContent side="right">Account</TooltipContent>
          </Tooltip>
          <DropdownMenuContent side="right" align="end" className="w-56">
            <UserMenuItems onWhatsNew={onWhatsNew} />
          </DropdownMenuContent>
        </DropdownMenu>
        <RailItem label="Settings" link={{ to: `/t/$teamSlug/settings`, params }}>
          <NavSettingsIcon className="size-4" />
        </RailItem>
      </div>
    </div>
  )
}

/** The pinned icons, fenced by a separator only when there are any. */
function PinnedIconGroup({
  teamId,
  teamSlug,
}: {
  teamId: string
  teamSlug: string
}) {
  return (
    <SidebarPinnedIcons
      teamId={teamId}
      teamSlug={teamSlug}
      renderItem={(item) => (
        <RailItem
          key={item.key}
          label={item.label}
          active={item.active}
          link={item.link}
          onClick={item.onClick}
        >
          {item.icon}
        </RailItem>
      )}
      separator={<Separator className="my-1 w-6! shrink-0" />}
    />
  )
}
