import type * as React from "react"
import { useNavigate } from "@tanstack/react-router"
import {
  conceptIcon,
  DropdownMenuItem,
  DropdownMenuSeparator,
  Badge,
  SidebarMenuTrailing,
  UserAvatar as UserAvatarView,
} from "@exp/ui"
import type { CountBadgeValue } from "@exp/domain-contract/nav-badges"
import { isAdminUser } from "@/lib/auth/app-user"
import { useSession } from "@/hooks/use-session"
import { useSignOut } from "@/hooks/use-sign-out"

// The main menu's shared pieces: the nav count badge and the account menu.

const NavAboutIcon = conceptIcon(`settings-about`)
const NavAdminIcon = conceptIcon(`nav-admin`)
const NavChangelogIcon = conceptIcon(`nav-changelog`)
const NavSignOutIcon = conceptIcon(`nav-sign-out`)

/** #FEED-232 (nav-badges.json): a nav row's count badge in the row's
 *  trailing column (EXP-1477); a count of 0 renders nothing. */
export function NavCountBadge({
  value,
  ...rest
}: { value: CountBadgeValue } & Omit<React.ComponentProps<`span`>, `children`>) {
  if (value.count <= 0) return null
  return (
    <SidebarMenuTrailing className="pointer-events-none">
      <Badge count={value.count} tone={value.tone} {...rest} />
    </SidebarMenuTrailing>
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
