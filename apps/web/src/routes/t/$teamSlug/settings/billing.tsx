import { createFileRoute } from "@tanstack/react-router"
import { TeamBillingSection } from "@/components/team/billing-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/billing`)({
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
          proProductId={config.creemProProductId}
          businessProductId={config.creemBusinessProductId}
          businessYearlyProductId={config.creemBusinessYearlyProductId}
        />
      )}
    </SettingsSectionGuard>
  )
}
