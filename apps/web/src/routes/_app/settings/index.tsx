import { createFileRoute, Navigate } from "@tanstack/react-router"
import { useIsMobile } from "@exp/ui"
import { PhoneSettingsRoot } from "@/components/team/phone-settings"

// Bare `/settings`: the phones' Settings tab (#EXP-2109, settings-nav.json
// `nav.phone`); md+ opens the first Personal page, Account (#EXP-2027).
export const Route = createFileRoute(`/_app/settings/`)({
  component: PersonalSettingsIndex,
})

function PersonalSettingsIndex() {
  const { defaultTeamSlug } = Route.useRouteContext()
  return useIsMobile() ? <PhoneSettingsRoot teamSlug={defaultTeamSlug} /> : <Navigate to="/settings/account" replace />
}
