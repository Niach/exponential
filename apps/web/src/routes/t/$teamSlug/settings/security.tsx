import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { ApiKeysSection } from "@/components/account/api-keys-section"
import { pageTitle } from "@/lib/page-title"

export const Route = createFileRoute(`/t/$teamSlug/settings/security`)({
  head: () => ({
    meta: [{ title: pageTitle(`Security`, `Settings`) }],
  }),
  loader: async () => {
    const { keys } = await trpc.users.listPersonalApiKeys.query()
    return { keys }
  },
  component: SettingsSecurity,
})

// Personal section (EXP-862): what acts AS you — the self-service expu_ API
// keys. What signs you in (email, providers, passkeys) is Settings › Account
// › Sign-in methods since EXP-1126. Account-level: the team in the URL is
// just the settings surface you are on.
function SettingsSecurity() {
  const { keys } = Route.useLoaderData()
  return (
    <div className="space-y-6">
      <ApiKeysSection initialKeys={keys} />
    </div>
  )
}
