import { useEffect } from "react"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  SETTINGS_NAV,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/`)({
  component: SettingsIndexRedirect,
})

// Bare /settings forwards to the first section the viewer can see (owners →
// General, plain members → Members). The role comes from live shapes, so we
// must wait for `resolved` — redirecting on transiently-false permissions
// would strand owners on the members page.
function SettingsIndexRedirect() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { permissions, solo, config, resolved } =
    useSettingsPage(teamSlug)

  const isCloud = Boolean(config?.isCloud)
  const ready = resolved && config !== null

  useEffect(() => {
    if (!ready) return
    const first = SETTINGS_NAV.flatMap((group) => group.items).find((item) =>
      item.visible(permissions, { isCloud, solo })
    )
    // Members is never gated, so `first` always exists.
    if (first) {
      void navigate({
        to: first.to,
        params: { teamSlug },
        replace: true,
      })
    }
  }, [ready, permissions, isCloud, solo, navigate, teamSlug])

  return null
}
