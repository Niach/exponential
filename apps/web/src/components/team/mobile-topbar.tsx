import { useState } from "react"
import { useMatchRoute, useNavigate, useParams } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import type { Board, Team } from "@/db/schema"
import { useSession } from "@/hooks/use-session"
import { useSignOut } from "@/hooks/use-sign-out"
import { isAdminUser } from "@/lib/auth/app-user"
import { getInitials } from "@/lib/utils"
import { openFeedbackWidget } from "@/components/feedback-widget-provider"
import { useFeedbackWidgetAvailable } from "@/components/feedback-button"
import { ChangelogSheet } from "@/components/whats-new"
import { BoardSwitcherSheet } from "@/components/team/board-switcher-sheet"
import {
  resolveBoardTarget,
  useMobileChromeVisible,
} from "@/components/team/mobile-tab-bar"
import { Avatar, AvatarFallback, AvatarImage } from "@/components/ui/avatar"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"

// EXP-317: the cross-client nav glyphs come from the shared registry
// (packages/icons/icons.json) so web, desktop, iOS and Android agree.
const NavAboutIcon = conceptIcon(`settings-about`)
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavAccountIcon = conceptIcon(`nav-account`)
const NavSettingsIcon = conceptIcon(`nav-settings`)
const NavSignOutIcon = conceptIcon(`nav-sign-out`)
const NavSupportIcon = conceptIcon(`nav-support`)
const NavTeamSwitcherIcon = conceptIcon(`nav-team-switcher`)

interface TeamMobileTopbarProps {
  teamSlug: string
  team: Team | null | undefined
  boards: Board[] | undefined
}

// Mobile-only chrome (EXP-189), mirroring the native apps' Issues header:
// a board-switcher control (board name + chevron → bottom sheet) on board
// surfaces, a static title elsewhere, and the user menu (settings, account,
// what's new, feedback, sign out) behind the avatar. Primary navigation
// lives in the floating MobileTabBar; the whole header is `md:hidden` and
// hides with it on detail routes (which carry their own headers).
export function TeamMobileTopbar({
  teamSlug,
  team,
  boards,
}: TeamMobileTopbarProps) {
  const { data: session } = useSession()
  const navigate = useNavigate()
  const handleSignOut = useSignOut()
  const matchRoute = useMatchRoute()
  const visible = useMobileChromeVisible()
  const { boardSlug } = useParams({ strict: false })
  const [switcherOpen, setSwitcherOpen] = useState(false)
  const [whatsNewOpen, setWhatsNewOpen] = useState(false)
  const feedbackAvailable = useFeedbackWidgetAvailable()

  const sectionTitle = matchRoute({
    to: `/t/$teamSlug/inbox`,
    fuzzy: true,
  })
    ? `Inbox`
    : matchRoute({ to: `/t/$teamSlug/agents`, fuzzy: true })
      ? `Agents`
      : matchRoute({ to: `/t/$teamSlug/reviews`, fuzzy: true })
        ? `Reviews`
        : matchRoute({ to: `/t/$teamSlug/support`, fuzzy: true })
          ? `Support`
          : matchRoute({ to: `/t/$teamSlug/settings`, fuzzy: true })
            ? `Settings`
            : undefined

  if (!visible) return null

  const boardTarget = resolveBoardTarget(teamSlug, boards, boardSlug)
  const switcherLabel = boardTarget?.name ?? team?.name ?? teamSlug

  // Name-less accounts (Apple sign-in stores an empty name) fall back to the
  // email for initials instead of a bare "?".
  const userLabel = session?.user?.name || session?.user?.email
  const userInitials = userLabel ? getInitials(userLabel) : `?`

  return (
    <header className="sticky top-0 z-30 flex h-12 items-center gap-2 border-b border-border/60 bg-background/60 px-3 backdrop-blur-xl md:hidden">
      {sectionTitle ? (
        <span className="truncate text-sm font-medium">{sectionTitle}</span>
      ) : (
        <button
          type="button"
          onClick={() => setSwitcherOpen(true)}
          aria-label="Switch board"
          className="flex min-w-0 items-center gap-1.5 rounded-md py-1 pl-1 pr-2 text-sm font-medium hover:bg-muted/50"
        >
          {boardTarget && (
            <span
              className="size-2.5 shrink-0 rounded-full"
              style={{ backgroundColor: boardTarget.color }}
            />
          )}
          <span className="truncate">{switcherLabel}</span>
          <NavTeamSwitcherIcon className="size-3.5 shrink-0 text-muted-foreground" />
        </button>
      )}

      <div className="ml-auto flex items-center">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="size-9 rounded-full"
              aria-label="User menu"
            >
              <Avatar className="size-7">
                {session?.user?.image && (
                  <AvatarImage src={session.user.image} />
                )}
                <AvatarFallback className="text-xs">
                  {userInitials}
                </AvatarFallback>
              </Avatar>
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-56">
            {isAdminUser(session?.user) && (
              <DropdownMenuItem onClick={() => navigate({ to: `/admin` })}>
                <NavAdminIcon className="mr-2 size-4" />
                Admin
              </DropdownMenuItem>
            )}
            <DropdownMenuItem
              onClick={() =>
                navigate({
                  to: `/t/$teamSlug/settings`,
                  params: { teamSlug },
                })
              }
            >
              <NavSettingsIcon className="mr-2 size-4" />
              Settings
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() =>
                navigate({
                  to: `/t/$teamSlug/settings/account`,
                  params: { teamSlug },
                })
              }
            >
              <NavAccountIcon className="mr-2 size-4" />
              Account
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setWhatsNewOpen(true)}>
              <NavChangelogIcon className="mr-2 size-4" />
              What&apos;s new
            </DropdownMenuItem>
            {/* EXP-262: version-less About page with the third-party
                licence notices. */}
            <DropdownMenuItem onClick={() => navigate({ to: `/about` })}>
              <NavAboutIcon className="mr-2 size-4" />
              About
            </DropdownMenuItem>
            {feedbackAvailable && (
              <DropdownMenuItem onClick={() => openFeedbackWidget()}>
                <NavSupportIcon className="mr-2 size-4" />
                Feedback & support
              </DropdownMenuItem>
            )}
            <DropdownMenuSeparator />
            <DropdownMenuItem onClick={handleSignOut}>
              <NavSignOutIcon className="mr-2 size-4" />
              Sign out
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <BoardSwitcherSheet
        open={switcherOpen}
        onOpenChange={setSwitcherOpen}
        teamSlug={teamSlug}
        team={team}
        boards={boards}
        activeBoardSlug={boardSlug}
      />
      <ChangelogSheet open={whatsNewOpen} onOpenChange={setWhatsNewOpen} />
    </header>
  )
}
