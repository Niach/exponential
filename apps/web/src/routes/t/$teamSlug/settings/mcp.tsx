import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { SETTINGS_MCP } from "@exp/domain-contract/settings-mcp"
import type { McpSettingsRequest } from "@/components/team/mcp-servers-section"
import { McpSettingsPage } from "@/components/mcp/mcp-settings-page"
import { useSettingsPage } from "@/routes/t/$teamSlug/settings/-shared"
import { pageTitle } from "@/lib/page-title"

// #VAPP-185 (fixtures/settings-mcp.json): Settings › MCP, the merged page
// (Endpoint, Servers, Clients, Access, Built-in tools). Every member reads
// it; owner-only controls live inside each section. The servers' one-shot
// deep links ride the URL (`?connect=<serverId>`, the OAuth callback's
// `?mcp=connected|failed&server=<id>[&error=]`) and are stripped once used.
const nonEmpty = (value: unknown): string | undefined =>
  typeof value === `string` && value !== `` ? value : undefined

export const validateMcpSettingsSearch = (search: Record<string, unknown>): McpSettingsRequest => ({
  connect: nonEmpty(search.connect),
  mcp: search.mcp === `connected` || search.mcp === `failed` ? search.mcp : undefined,
  server: nonEmpty(search.server),
  error: nonEmpty(search.error),
})

export const Route = createFileRoute(`/t/$teamSlug/settings/mcp`)({
  head: () => ({ meta: [{ title: pageTitle(SETTINGS_MCP.title, `Settings`) }] }),
  validateSearch: validateMcpSettingsSearch,
  component: SettingsMcp,
})

function SettingsMcp() {
  const { teamSlug } = Route.useParams()
  const request = Route.useSearch()
  const navigate = useNavigate()
  const { session, team, permissions } = useSettingsPage(teamSlug)
  if (!team || !session?.user?.id) return null
  return (
    <McpSettingsPage
      teamId={team.id}
      teamSlug={teamSlug}
      isOwner={permissions.isOwner}
      request={request}
      onRequestConsumed={() =>
        void navigate({ to: `/t/$teamSlug/settings/mcp`, params: { teamSlug }, search: {}, replace: true })
      }
    />
  )
}
