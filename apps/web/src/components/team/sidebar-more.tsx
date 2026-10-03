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

// SLOP-5: the ONE advanced entry of the navigation ×4. Everything that is
// not one of the four nouns (People / Devices / Apps / Actions) plus Reviews
// and Inbox sits behind "More": the action catalog (authoring; the composer's
// action chip is the everyday way to RUN one), the parked drafts and, on a
// phone, Settings (phones have no footer gear). The menu is the styleguide's
// `menu` surface — pointer density from md up, touch density below — so the
// sidebar row, the compact rail's icon and the tab bar's tab all open the
// same rows. Desktop (`sidebar.rs`), iOS (`MobileTabBar.swift`) and Android
// (`BottomNavBar.kt`) draw the same entry with the same copy.

export const MORE_LABEL = `More`
export const MORE_ACTIONS_LABEL = `Actions`
export const MORE_DRAFTS_LABEL = `Drafts`
export const MORE_SETTINGS_LABEL = `Settings`

export const NavMoreIcon = conceptIcon(`nav-more`)
const NavActionsIcon = conceptIcon(`nav-actions`)
const NavDraftsIcon = conceptIcon(`nav-drafts`)
const NavSettingsIcon = conceptIcon(`nav-settings`)

/** Whether one of More's destinations is on screen — the entry reads active
 *  then, exactly like the desktop rail's Actions entry stayed lit on one
 *  action's page (SLOP-2). `settings` is a destination only on phones. */
export function useMoreActive({ settings }: { settings: boolean }): boolean {
  const matchRoute = useMatchRoute()
  const onActions = Boolean(
    matchRoute({ to: `/t/$teamSlug/actions`, fuzzy: true })
  )
  const onDrafts = Boolean(
    matchRoute({ to: `/t/$teamSlug/drafts`, fuzzy: true })
  )
  const onSettings =
    settings &&
    Boolean(matchRoute({ to: `/t/$teamSlug/settings`, fuzzy: true }))
  return onActions || onDrafts || onSettings
}

/** The Drafts row — its own component so the phone variant (which keeps
 *  Drafts as an Inbox segment, EXP-878) never runs the drafts query. Hidden
 *  while the caller has no draft, like the old sidebar entry. */
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
  /** Web md+ and desktop list the parked drafts here; phones do not. */
  drafts: boolean
  /** Phones carry Settings here; the sidebar keeps its footer gear. */
  settings: boolean
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
  settings,
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
        {settings && (
          <DropdownMenuItem asChild>
            <Link
              to="/t/$teamSlug/settings"
              params={{ teamSlug }}
              data-testid="menu-settings"
            >
              <NavSettingsIcon />
              <span>{MORE_SETTINGS_LABEL}</span>
            </Link>
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
