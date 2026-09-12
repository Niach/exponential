import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { AccountOverview } from "@/components/account/account-overview"
import { DeleteAccountSection } from "@/components/account/delete-account-section"
import { PasskeysSection } from "@/components/account/passkeys-section"
import { getAuthConfig } from "@/lib/auth/config"

export const Route = createFileRoute(`/t/$teamSlug/settings/account`)({
  loader: async () => {
    const [timezone, authConfig] = await Promise.all([
      trpc.users.timezone.query(),
      // EXP-857: whether this instance offers passkeys at all (https base).
      getAuthConfig().catch(() => null),
    ])
    return {
      timezone: timezone.timezone,
      passkeyEnabled: authConfig?.passkeyEnabled ?? false,
    }
  },
  component: SettingsAccount,
})

// Personal section (EXP-238): identity, timezone, passkeys (EXP-857) and
// account deletion. Always visible — the layout's beforeLoad already
// guarantees a session.
function SettingsAccount() {
  const { timezone, passkeyEnabled } = Route.useLoaderData()

  return (
    <div className="space-y-6">
      <AccountOverview initialTimezone={timezone} />
      <PasskeysSection passkeyEnabled={passkeyEnabled} />
      <DeleteAccountSection />
    </div>
  )
}
