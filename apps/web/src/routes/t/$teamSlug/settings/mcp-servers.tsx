import { createFileRoute } from "@tanstack/react-router"
import { TeamMcpServersSection } from "@/components/team/mcp-servers-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"

export const Route = createFileRoute(`/t/$teamSlug/settings/mcp-servers`)({
  component: SettingsMcpServers,
})

// EXP-792: member-visible — every member reads the list and signs in on their
// OWN machines from here; add/edit/remove are owner-only inside the section
// (the `mcpServers` router gates the writes the same way).
function SettingsMcpServers() {
  const { teamSlug } = Route.useParams()
  const { session, team, permissions } = useSettingsPage(teamSlug)

  return (
    <>
      {team && session?.user?.id && (
        <TeamMcpServersSection
          teamId={team.id}
          currentUserId={session.user.id}
          isOwner={permissions.isOwner}
        />
      )}
    </>
  )
}
