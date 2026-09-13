import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { ApiKeysSection } from "@/components/account/api-keys-section"
import { PasskeysSection } from "@/components/account/passkeys-section"
import { getAuthConfig } from "@/lib/auth/config"

export const Route = createFileRoute(`/t/$teamSlug/settings/security`)({
  loader: async () => {
    const [{ keys }, authConfig] = await Promise.all([
      trpc.users.listPersonalApiKeys.query(),
      // EXP-857: whether this instance offers passkeys at all (https base).
      getAuthConfig().catch(() => null),
    ])
    return { keys, passkeyEnabled: authConfig?.passkeyEnabled ?? false }
  },
  component: SettingsSecurity,
})

// Personal section (EXP-862): everything that signs you in or acts as you —
// self-service expu_ API keys and passkeys (moved off the Account page).
// Account-level: the team in the URL is just the settings surface you are on.
function SettingsSecurity() {
  const { keys, passkeyEnabled } = Route.useLoaderData()
  return (
    <div className="space-y-6">
      <ApiKeysSection initialKeys={keys} />
      <PasskeysSection passkeyEnabled={passkeyEnabled} />
    </div>
  )
}
