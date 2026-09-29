import { createFileRoute, useNavigate } from "@tanstack/react-router"
import {
  TeamMcpServersSection,
  type McpSettingsRequest,
} from "@/components/team/mcp-servers-section"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

// EXP-792: member-visible — every member reads the list and connects their
// OWN account from here; add/edit/remove are owner-only inside the section
// (the `mcpServers` router gates the writes the same way).
//
// Two one-shot deep links ride the URL and are stripped once handed over:
// `?connect=<serverId>` (the launch picker's "Connect first", the MCP tools'
// `connectUrl`) starts that server's connect; `?mcp=connected|failed&server=
// <id>[&error=]` is where the server-side OAuth callback lands.
const nonEmpty = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

export const Route = createFileRoute(`/t/$teamSlug/settings/mcp-servers`)({
  head: () => ({
    meta: [{ title: pageTitle(`MCP servers`, `Settings`) }],
  }),
  validateSearch: (search: Record<string, unknown>): McpSettingsRequest => ({
    connect: nonEmpty(search.connect),
    mcp:
      search.mcp === `connected` || search.mcp === `failed`
        ? search.mcp
        : undefined,
    server: nonEmpty(search.server),
    error: nonEmpty(search.error),
  }),
  component: SettingsMcpServers,
})

function SettingsMcpServers() {
  const { teamSlug } = Route.useParams()
  const request = Route.useSearch()
  const navigate = useNavigate()
  const { session, team, permissions } = useSettingsPage(teamSlug)

  return (
    <>
      {team && session?.user?.id && (
        <TeamMcpServersSection
          teamId={team.id}
          isOwner={permissions.isOwner}
          request={request}
          onRequestConsumed={() =>
            void navigate({
              to: `/t/$teamSlug/settings/mcp-servers`,
              params: { teamSlug },
              search: {},
              replace: true,
            })
          }
        />
      )}
    </>
  )
}
