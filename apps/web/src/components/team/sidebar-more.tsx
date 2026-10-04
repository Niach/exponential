import type { ReactNode } from "react"
import { Link, useMatchRoute } from "@tanstack/react-router"
import {
  Badge,
  conceptIcon,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@exp/ui"
import { useDraftEntries } from "@/hooks/use-issue-drafts"

// SLOP-5: the sidebar's ONE advanced entry: the action catalog (authoring;
// the composer's action chip is the everyday way to RUN one) and the parked
// drafts sit behind "More". The menu is the styleguide's `menu` surface, so
// the sidebar row and the compact rail's icon open the same rows; desktop
// (`sidebar.rs`) draws the same entry with the same copy. EXP-1187: PHONES
// have no More (web, iOS, Android): Actions is a tab of its own and Settings
// lives in the topbar's avatar menu.

export const MORE_LABEL = `More`
export const MORE_ACTIONS_LABEL = `Actions`
export const MORE_DRAFTS_LABEL = `Drafts`

export const NavMoreIcon = conceptIcon(`nav-more`)
const NavActionsIcon = conceptIcon(`nav-actions`)
const NavDraftsIcon = conceptIcon(`nav-drafts`)

/** Whether one of More's destinations is on screen — the entry reads active
 *  then, exactly like the desktop rail's Actions entry stayed lit on one
 *  action's page (SLOP-2). */
export function useMoreActive(): boolean {
  const matchRoute = useMatchRoute()
  const onActions = Boolean(
    matchRoute({ to: `/t/$teamSlug/actions`, fuzzy: true })
  )
  const onDrafts = Boolean(
    matchRoute({ to: `/t/$teamSlug/drafts`, fuzzy: true })
  )
  return onActions || onDrafts
}

/** The Drafts row — hidden while the caller has no draft, like the old
 *  sidebar entry. */
function DraftsMenuItem({
  teamSlug,
  teamId,
}: {
  teamSlug: string
  teamId?: string
}) {
  const count = useDraftEntries(teamId).length
  if (count === 0) return null
  return (
    <DropdownMenuItem asChild>
      <Link to="/t/$teamSlug/drafts" params={{ teamSlug }} data-testid="menu-drafts">
        <NavDraftsIcon />
        <span className="flex-1">{MORE_DRAFTS_LABEL}</span>
        <Badge count={count} data-testid="drafts-count-badge" />
      </Link>
    </DropdownMenuItem>
  )
}

export interface MoreMenuProps {
  teamSlug: string
  teamId?: string
  /** List the parked drafts (the sidebar and the rail both do). */
  drafts: boolean
  /** Where the surface opens relative to the trigger. */
  side: `right` | `top` | `bottom`
  align?: `start` | `center` | `end`
  /** The trigger — a sidebar row, a rail icon or a tab. */
  children: ReactNode
}

/** The More menu around its trigger. */
export function MoreMenu({
  teamSlug,
  teamId,
  drafts,
  side,
  align = `start`,
  children,
}: MoreMenuProps) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>{children}</DropdownMenuTrigger>
      <DropdownMenuContent side={side} align={align} className="w-56">
        <DropdownMenuItem asChild>
          <Link
            to="/t/$teamSlug/actions"
            params={{ teamSlug }}
            data-testid="menu-actions"
          >
            <NavActionsIcon />
            <span>{MORE_ACTIONS_LABEL}</span>
          </Link>
        </DropdownMenuItem>
        {drafts && <DraftsMenuItem teamSlug={teamSlug} teamId={teamId} />}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
