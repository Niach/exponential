import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { useNeedsYouCounts } from "@/hooks/use-needs-you"

// #FEED-232 (nav-badges.json `appIconBadge`): the OS app icon = the
// account's Needs you total, via `navigator.setAppBadge` where the browser
// has it (an installed PWA); 0 clears it. Mounted once, in the team layout.
// #PERF-13: the same ONE count feeds the sidebar and the team page
// (`useMemberNeedsYouCounts`); it ran three times per synced row, and at the
// layout's top it re-rendered the whole layout on every one.

type BadgingNavigator = Navigator & {
  setAppBadge?: (count?: number) => Promise<void>
  clearAppBadge?: () => Promise<void>
}

/** Paint `count` on the app icon; a no-op without the Badging API. */
export function setAppIconBadge(count: number, nav: BadgingNavigator | undefined = globalThis.navigator): void {
  if (!nav?.setAppBadge || !nav.clearAppBadge) return
  const done = count > 0 ? nav.setAppBadge(count) : nav.clearAppBadge()
  // A refused badge (no permission, not installed) changes nothing.
  done.catch(() => {})
}

type MemberNeedsYouCounts = { byTeam: ReadonlyMap<string, number>; total: number }

const NONE: MemberNeedsYouCounts = { byTeam: new Map(), total: 0 }

const MemberNeedsYouContext = createContext<MemberNeedsYouCounts>(NONE)

/** Every member team's Needs you counts, read once for everything under it;
 *  also paints the app icon. Its children are the caller's elements, so a
 *  count change re-renders only the readers. */
export function MemberNeedsYouProvider({ userId, children }: { userId: string | undefined; children: ReactNode }) {
  const { myTeams } = useTeamMemberships(userId)
  const teamIds = useMemo(() => myTeams.map((team) => team.id), [myTeams])
  const counts = useNeedsYouCounts(teamIds, userId)
  useEffect(() => {
    setAppIconBadge(counts.total)
  }, [counts.total])
  return <MemberNeedsYouContext.Provider value={counts}>{children}</MemberNeedsYouContext.Provider>
}

/** The member teams' Needs you counts (`MemberNeedsYouProvider`'s). */
export function useMemberNeedsYouCounts(): MemberNeedsYouCounts {
  return useContext(MemberNeedsYouContext)
}
