import { createFileRoute } from "@tanstack/react-router"
import { WorkspaceProjectsSection } from "@/components/workspace/projects-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings/projects`)({
  component: SettingsProjects,
})

function SettingsProjects() {
  const { workspaceSlug } = Route.useParams()
  const { workspace, permissions, resolved } = useSettingsPage(workspaceSlug)

  return (
    <SettingsSectionGuard resolved={resolved} allowed={permissions.isOwner}>
      {workspace && <WorkspaceProjectsSection workspace={workspace} />}
    </SettingsSectionGuard>
  )
}
