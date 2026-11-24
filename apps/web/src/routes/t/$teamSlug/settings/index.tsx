import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import { PhoneTeamSettings } from "@/components/team/phone-settings"
import {
  firstTeamSettingsPage,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/`)({
  component: SettingsIndex,
})

// Bare /settings forwards to the first section the viewer can see (owners →
// General, plain members → Members). The role comes from live shapes, so we
// must wait for `resolved` — redirecting on transiently-false permissions
// would strand owners on the members page. #EXP-2027: md+ takes the TEAM
// nav's first page (settings-nav.json `desktopNavs.team`); #EXP-2109: a phone
// draws the team's settings list (`nav.phone.team`).
function SettingsIndex() {
  const { teamSlug } = Route.useParams()
  return useIsMobile() ? <PhoneTeamSettings teamSlug={teamSlug} /> : <WideSettingsIndex teamSlug={teamSlug} />
}

function WideSettingsIndex({ teamSlug }: { teamSlug: string }) {
  const navigate = useNavigate()
  const { permissions, config, resolved } = useSettingsPage(teamSlug)

  const isCloud = Boolean(config?.isCloud)
  const ready = resolved && config !== null

  useEffect(() => {
    if (!ready) return
    const first = firstTeamSettingsPage({ isOwner: permissions.isOwner, isCloud })
    // Members is never gated, so `first` always exists.
    if (first) {
      void navigate({
        to: first.to,
        params: { teamSlug },
        replace: true,
      })
    }
  }, [ready, permissions, isCloud, navigate, teamSlug])

  return null
}
