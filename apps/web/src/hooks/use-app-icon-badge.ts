import { useEffect } from "react"
import { useTeamMemberships } from "@/hooks/use-team-data"
import { useNeedsYouCounts } from "@/hooks/use-needs-you"

// #FEED-232 (nav-badges.json `appIconBadge`): the OS app icon = every team's
// Needs you rows (distinct), via `navigator.setAppBadge` where the browser
// has it (an installed PWA); 0 clears it. Mounted once, in the team layout.

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

export function useAppIconBadge(userId: string | undefined): void {
  const { myTeams } = useTeamMemberships(userId)
  const { total } = useNeedsYouCounts(
    myTeams.map((team) => team.id),
    userId
  )
  useEffect(() => {
    setAppIconBadge(total)
  }, [total])
}
