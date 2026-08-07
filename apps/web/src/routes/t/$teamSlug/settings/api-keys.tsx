import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { ApiKeysSection } from "@/components/account/api-keys-section"

export const Route = createFileRoute(`/t/$teamSlug/settings/api-keys`)({
  loader: async () => {
    const { keys } = await trpc.users.listPersonalApiKeys.query()
    return { keys }
  },
  component: SettingsApiKeys,
})

// Personal section (EXP-238): self-service expu_ API keys. Account-level —
// the team in the URL is just the settings surface the user is on.
function SettingsApiKeys() {
  const { keys } = Route.useLoaderData()
  return <ApiKeysSection initialKeys={keys} />
}
