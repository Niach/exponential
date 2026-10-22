import { conceptIcon } from "@exp/ui"
import { useDraftEntries } from "@/hooks/use-issue-drafts"

// The Drafts pile is a ROOT entry of the navigation ×4: the expanded
// sidebar, the compact rail and the IDE rail (`sidebar.rs`) list it below
// Devices while any draft exists (EXP-878). NEW-2: there is no Actions entry
// any more (prompts live in the composer's `/` menu and Settings → Prompts).

export const DRAFTS_LABEL = `Drafts`

export const NavDraftsIcon = conceptIcon(`nav-drafts`)

/** The caller's parked drafts in this team — the Drafts entry shows only
 *  while it is above zero. */
export function useDraftCount(teamId?: string): number {
  return useDraftEntries(teamId).length
}
