import { conceptIcon } from "@exp/ui"
import { useDraftEntries } from "@/hooks/use-issue-drafts"

// Actions and the Drafts pile are ROOT entries of the navigation ×4: the
// expanded sidebar, the compact rail and the IDE rail (`sidebar.rs`) list
// Actions after Reviews and Drafts below it while any draft exists (EXP-878).
// The SLOP-5 "More" menu that held them is gone; phones carry Actions as a
// tab of their own (EXP-1187).

export const ACTIONS_LABEL = `Actions`
export const DRAFTS_LABEL = `Drafts`

export const NavActionsIcon = conceptIcon(`nav-actions`)
export const NavDraftsIcon = conceptIcon(`nav-drafts`)

/** The caller's parked drafts in this team — the Drafts entry shows only
 *  while it is above zero. */
export function useDraftCount(teamId?: string): number {
  return useDraftEntries(teamId).length
}
