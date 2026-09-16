import { createFileRoute } from "@tanstack/react-router"
import { TeamBillingSection } from "@/components/team/billing-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/billing`)({
  head: () => ({
    meta: [{ title: pageTitle(`Plan & Billing`, `Settings`) }],
  }),
  component: SettingsBilling,
})

function SettingsBilling() {
  const { teamSlug } = Route.useParams()
  const { team, permissions, config, resolved } =
    useSettingsPage(teamSlug)

  // Billing is cloud-only; self-hosted instances have no billing surface.
  if (config && !config.isCloud) return null

  return (
    <SettingsSectionGuard
      resolved={resolved && config !== null}
      allowed={permissions.canManageTeam}
    >
      {team && config && (
        <TeamBillingSection
          teamId={team.id}
          teamProductId={config.creemTeamProductId}
          teamYearlyProductId={config.creemTeamYearlyProductId}
        />
      )}
    </SettingsSectionGuard>
  )
}
