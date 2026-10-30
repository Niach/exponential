import { CHATS_COPY } from "@exp/domain-contract/chats"
import { useApps } from "@/hooks/use-apps"
import { SidebarAppsIcons } from "@/components/team/sidebar-apps"
import type * as React from "react"
import { Link, useNavigate, useRouterState } from "@tanstack/react-router"
import {
  conceptIcon,
  getBoardIcon,
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  Separator,
  SidebarMenuTrailing,
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
import { useUnreadNotificationCount } from "@/hooks/use-unread-notifications"
import type { AgentRow } from "@/lib/session-tree"
import { SidebarPinnedIcons } from "@/components/team/sidebar-pinned"
import {
  SidebarRunningIcons,
  type SidebarThreads,
} from "@/components/team/sidebar-running"
import { SidebarAgentsIcons } from "@/components/team/sidebar-agents"
import {
  DRAFTS_LABEL,
  NavDraftsIcon,
  useDraftCount,
} from "@/components/team/sidebar-nav-entries"

// EXP-870: the rail never leaves. Beside a list nav or the settings nav the
// main menu MORPHS into this 48px icon column instead of sliding away —
// Inbox, Agent, Threads, the agents, the boards, the pins and the running
// threads and the account stay one click away in
// every state (desktop `compact_rail.rs`). Strictly derived from the URL
// (`sidebarOccupant`), never a user toggle. 32px ghost buttons centred in
// the column, the name in a right-hand tooltip, badges top-right.
//
// The badges live here and are shared with the expanded rows in
// `sidebar.tsx`, so both states read one set of counts.

const NavAboutIcon = conceptIcon(`settings-about`)
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavAgentIcon = conceptIcon(`action-chat`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavDevicesIcon = conceptIcon(`nav-devices`)
const NavInboxIcon = conceptIcon(`nav-inbox`)
const NavThreadsIcon = conceptIcon(`nav-threads`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const NavSignOutIcon = conceptIcon(`nav-sign-out`)

export type BadgePlacement = `row` | `icon`

// EXP-699: nav badges are dots — same colors and logic as the mobile tab
// bars. A row carries it in its trailing column (EXP-1477), an icon at its
// top-right corner.
function NavDot({
  className,
  placement,
}: {
  className: string
  placement: BadgePlacement
}) {
  const dot = (
    <span
      className={cn(
        `pointer-events-none size-2 rounded-full`,
        placement === `icon` && `absolute right-1 top-1`,
        className
      )}
    />
  )
  if (placement === `icon`) return dot
  return <SidebarMenuTrailing className="pointer-events-none">{dot}</SidebarMenuTrailing>
}

/** EXP-1075: live sessions of MINE in ANOTHER team — the team picker's dot
 * (`lib/sessions/team-live-runs.ts` decides where it hangs). Same tones as
 * the mobile tab bar's chat badge: amber while one of them waits on the
 * person, green otherwise. A dot, never a count — the picker only says
 * "look over there"; the team you are in lists its threads in the Running
 * section.
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

/** EXP-1293: the Threads entry's dot — green while a thread of the team
 *  runs, amber while one needs you (`threadsNavState`); EXP-1323: else the
 *  unread dot (the Inbox's) while a thread holds an unread reply. */
export function ThreadsLiveBadge({
  state,
  placement,
}: {
  state: { live: boolean; needsYou: boolean; unread?: boolean }
  placement: BadgePlacement
}) {
  if (!state.live && !state.unread) return null
  return (
    <NavDot
      className={
        state.needsYou ? `bg-yellow-400` : state.live ? `bg-green-500` : `bg-primary`
      }
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

export function TeamSidebarRail({
  teamSlug,
  team,
  boards,
  agents,
  threads,
  threadsNav,
  onWhatsNew,
}: {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
  /** The team's agents (`useAgents`), read once by the sidebar. */
  agents: readonly AgentRow[]
  /** The Threads list (`useThreadsList`), read once by the sidebar. */
  threads: SidebarThreads
  threadsNav: { live: boolean; needsYou: boolean; unread?: boolean }
  onWhatsNew: () => void
}) {
  const params = { teamSlug }
  const draftCount = useDraftCount(team?.id)
  // VAPP-110: the team's apps (the icons after the agents).
  const { apps } = useApps(team?.id)
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
        {/* Order ×4 (EXP-1293, NEW-2): Agent, Inbox, Threads, Devices, Drafts. */}
        <RailItem
          label="Agent"
          link={{ to: `/t/$teamSlug/agent`, params }}
          className={cn(
            composerSeeded &&
              `data-[status=active]:bg-transparent data-[status=active]:text-sidebar-foreground/80`
          )}
        >
          <NavAgentIcon className="size-4" />
        </RailItem>
        <RailItem label="Inbox" link={{ to: `/t/$teamSlug/inbox`, params }}>
          <NavInboxIcon className="size-4" />
          <InboxUnreadBadge placement="icon" />
        </RailItem>
        <RailItem label={CHATS_COPY.title} link={{ to: `/t/$teamSlug/threads`, params }}>
          <NavThreadsIcon className="size-4" />
          <ThreadsLiveBadge state={threadsNav} placement="icon" />
        </RailItem>
        <RailItem label="Devices" link={{ to: `/t/$teamSlug/devices`, params }}>
          <NavDevicesIcon className="size-4" />
        </RailItem>
        {draftCount > 0 && (
          <RailItem label={DRAFTS_LABEL} link={{ to: `/t/$teamSlug/drafts`, params }}>
            <NavDraftsIcon className="size-4" />
          </RailItem>
        )}

        {team && (
          <PinnedIconGroup teamId={team.id} teamSlug={teamSlug} />
        )}

        {/* EXP-1293: the team's agents, above the boards. */}
        <SidebarAgentsIcons
          teamSlug={teamSlug}
          agents={agents}
          separator={<Separator className="my-1 w-6! shrink-0" />}
          renderItem={(item) => (
            <RailItem key={item.key} label={item.label} link={item.link}>
              {item.icon}
            </RailItem>
          )}
        />

        {/* VAPP-110: the team's apps, after the agents. */}
        <SidebarAppsIcons
          teamSlug={teamSlug}
          apps={apps}
          separator={<Separator className="my-1 w-6! shrink-0" />}
          renderItem={(item) => (
            <RailItem key={item.key} label={item.label} link={item.link}>
              {item.icon}
            </RailItem>
          )}
        />

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

        {/* EXP-923 / EXP-1293: the running threads, under the boards —
            the agent mark plus the amber badge, the expanded row's whole
            label in the tooltip. */}
        <SidebarRunningIcons
          teamSlug={teamSlug}
          threads={threads}
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
