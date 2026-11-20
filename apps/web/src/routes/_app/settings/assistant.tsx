import { createFileRoute } from "@tanstack/react-router"
import { ASSISTANT_COPY } from "@exp/domain-contract/agent-settings"
import { AssistantSettingsBody } from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027 + #EXP-1987: the team-less Assistant page of web md+; the
// Assistant is found WITHOUT a team (agent-settings.json `assistant`).
export const Route = createFileRoute(`/_app/settings/assistant`)({
  head: () => ({
    meta: [{ title: pageTitle(ASSISTANT_COPY.title, `Settings`) }],
  }),
  component: AssistantRoute,
})

function AssistantRoute() {
  return <AssistantSettingsBody />
}
