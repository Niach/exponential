import { createFileRoute, redirect } from "@tanstack/react-router"

// S-3 (settings-nav.json `nav`): Personal → Voice is gone; the voice block
// is the Assistant page's Voice section (agent-settings.json `assistant`).
// Old links land there.
export const Route = createFileRoute(`/t/$teamSlug/settings/voice`)({
  beforeLoad: ({ params }) => {
    throw redirect({
      to: `/t/$teamSlug/settings/assistant`,
      params: { teamSlug: params.teamSlug },
      replace: true,
    })
  },
})
