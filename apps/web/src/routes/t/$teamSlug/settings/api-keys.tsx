import { createFileRoute, redirect } from "@tanstack/react-router"

// EXP-862: "API keys" became "Security" (keys + passkeys). The old path stays
// as a redirect — it is what older desktop builds, bookmarks and the MCP
// setup card link to.
export const Route = createFileRoute(`/t/$teamSlug/settings/api-keys`)({
  beforeLoad: ({ params }) => {
    throw redirect({
      to: `/t/$teamSlug/settings/security`,
      params: { teamSlug: params.teamSlug },
      replace: true,
    })
  },
})
