import { createFileRoute, redirect } from "@tanstack/react-router"
import { validateMcpSettingsSearch } from "@/routes/t/$teamSlug/settings/mcp"

// #VAPP-185: MCP servers became Settings › MCP; old links (bookmarks, the
// launch picker's "Connect first", the OAuth callback's returnTo of a flow
// started before the deploy) land there with their query.
export const Route = createFileRoute(`/t/$teamSlug/settings/mcp-servers`)({
  validateSearch: validateMcpSettingsSearch,
  beforeLoad: ({ params, search }) => {
    throw redirect({ to: `/t/$teamSlug/settings/mcp`, params, search, replace: true })
  },
})
