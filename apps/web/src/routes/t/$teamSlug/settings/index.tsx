import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import {
  firstTeamSettingsPage,
  flattenSettingsNav,
  SETTINGS_NAV,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/`)({
  component: SettingsIndexRedirect,
})

// Bare /settings forwards to the first section the viewer can see (owners →
// General, plain members → Members). The role comes from live shapes, so we
// must wait for `resolved` — redirecting on transiently-false permissions
// would strand owners on the members page. #EXP-2027: md+ takes the TEAM
// nav's first page (settings-nav.json `desktopNavs.team`); phones keep the
// whole nav's.
function SettingsIndexRedirect() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const isMobile = useIsMobile()
  const { permissions, config, resolved } = useSettingsPage(teamSlug)

  const isCloud = Boolean(config?.isCloud)
  const ready = resolved && config !== null

  useEffect(() => {
    if (!ready) return
    const first = isMobile
      ? SETTINGS_NAV.flatMap((group) => flattenSettingsNav(group.items)).find((item) =>
          item.visible(permissions, { isCloud })
        )
      : firstTeamSettingsPage({ isOwner: permissions.isOwner, isCloud })
    // Members is never gated, so `first` always exists.
    if (first) {
      void navigate({
        to: first.to,
        params: { teamSlug },
        replace: true,
      })
    }
  }, [ready, isMobile, permissions, isCloud, navigate, teamSlug])

  return null
}
