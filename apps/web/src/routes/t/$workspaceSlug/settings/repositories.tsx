import { createFileRoute } from "@tanstack/react-router"
import { WorkspaceRepositoriesSection } from "@/components/workspace/repositories-section"
import {
  SettingsSectionGuard,
  useSettingsPage,
} from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(
  `/t/$workspaceSlug/settings/repositories`
)({
  component: SettingsRepositories,
})

function SettingsRepositories() {
  const { workspaceSlug } = Route.useParams()
  const { workspace, permissions, resolved } = useSettingsPage(workspaceSlug)

  return (
    <SettingsSectionGuard
      resolved={resolved}
      allowed={permissions.canManageRepos}
    >
      {workspace && (
        <WorkspaceRepositoriesSection
          workspaceId={workspace.id}
          // The bootstrap feedback workspace (slug `feedback`) holds the
          // protected dogfood GitHub connection — its unlink is server-refused.
          isFeedbackWorkspace={workspace.slug === `feedback`}
        />
      )}
    </SettingsSectionGuard>
  )
}
