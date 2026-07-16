import { createFileRoute } from "@tanstack/react-router"
import { WorkspaceBillingSection } from "@/components/workspace/billing-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings/billing`)({
  component: SettingsBilling,
})

function SettingsBilling() {
  const { workspaceSlug } = Route.useParams()
  const { workspace, permissions, config, resolved } =
    useSettingsPage(workspaceSlug)

  // Billing is cloud-only; self-hosted instances have no billing surface.
  if (config && !config.isCloud) return null

  return (
    <SettingsSectionGuard
      resolved={resolved && config !== null}
      allowed={permissions.canManageWorkspace}
    >
      {workspace && config && (
        <WorkspaceBillingSection
          workspaceId={workspace.id}
          proProductId={config.creemProProductId}
          businessProductId={config.creemBusinessProductId}
          businessYearlyProductId={config.creemBusinessYearlyProductId}
        />
      )}
    </SettingsSectionGuard>
  )
}
