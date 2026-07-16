import { createFileRoute } from "@tanstack/react-router"
import { WorkspaceLabelsSection } from "@/components/workspace/labels-section"
import { useSettingsPage } from "@/routes/t/$workspaceSlug/settings/-shared"

export const Route = createFileRoute(`/t/$workspaceSlug/settings/labels`)({
  component: SettingsLabels,
})

function SettingsLabels() {
  const { workspaceSlug } = Route.useParams()
  const { workspace } = useSettingsPage(workspaceSlug)

  if (!workspace) return null
  return <WorkspaceLabelsSection workspaceId={workspace.id} />
}
