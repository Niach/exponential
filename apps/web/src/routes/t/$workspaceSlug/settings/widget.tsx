import { createFileRoute } from "@tanstack/react-router"
import { WorkspaceWidgetSection } from "@/components/workspace/widget-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings/widget`)({
  component: SettingsWidget,
})

function SettingsWidget() {
  const { workspaceSlug } = Route.useParams()
  const { workspace, permissions, resolved } = useSettingsPage(workspaceSlug)

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageWidgets}
    >
      {workspace && <WorkspaceWidgetSection workspaceId={workspace.id} />}
    </SettingsSectionGuard>
  )
}
