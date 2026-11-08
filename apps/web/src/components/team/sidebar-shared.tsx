import type * as React from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  conceptIcon,
  DropdownMenuItem,
  DropdownMenuSeparator,
  SidebarMenuDot,
  SidebarMenuTrailing,
  type SidebarDotTone,
  UserAvatar as UserAvatarView,
} from "@exp/ui"
import { isAdminUser } from "@/lib/auth/app-user"
import { cn } from "@/lib/utils"
import { useSession } from "@/hooks/use-session"
import { useSignOut } from "@/hooks/use-sign-out"

// The main menu's shared pieces: the nav dots, the team's live dot and the
// account menu. One set of counts for every row that reads them.

const NavAboutIcon = conceptIcon(`settings-about`)
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavSignOutIcon = conceptIcon(`nav-sign-out`)

// EXP-699: nav badges are dots — same colors and logic as the mobile tab
// bars, in the row's trailing column (EXP-1477).
function NavDot({ tone }: { tone: SidebarDotTone }) {
  return (
    <SidebarMenuTrailing className="pointer-events-none">
      <SidebarMenuDot tone={tone} className="pointer-events-none" />
    </SidebarMenuTrailing>
  )
}

/** EXP-1075: live sessions of MINE in ANOTHER team — the team picker's dot
 * (`lib/sessions/team-live-runs.ts` decides where it hangs). Same tones as
 * the mobile tab bar's chat badge: yellow while one of them waits on the
 * person, green otherwise. A dot, never a count — the picker only says
 * "look over there"; the team you are in lists its chats in the Running
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
    <SidebarMenuDot
      tone={live.needsInput ? `needsYou` : `live`}
      className={cn(
        // Hit-testable on purpose (unlike `NavDot`): the `title` is the only
        // thing that says WHY the dot is there, and a click still bubbles to
        // the row or trigger underneath.
        placement === `corner`
          ? `absolute -right-0.5 -top-0.5 ring-2 ring-sidebar`
          : `ml-auto`,
        className
      )}
      {...rest}
    />
  )
}

/** EXP-1293: the Chats entry's dot — green while a chat of the team
 *  runs, yellow while one needs you (`chatsNavState`); EXP-1323: else the
 *  unread dot (the Inbox's) while a chat holds an unread reply. */
export function ChatsLiveBadge({
  state,
}: {
  state: { live: boolean; needsYou: boolean; unread?: boolean }
}) {
  if (!state.live && !state.unread) return null
  return (
    <NavDot tone={state.needsYou ? `needsYou` : state.live ? `live` : `unread`} />
  )
}

/** The account menu's items. */
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
