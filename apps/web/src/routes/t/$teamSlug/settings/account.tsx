import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { AccountOverview } from "@/components/account/account-overview"
import { DeleteAccountSection } from "@/components/account/delete-account-section"

export const Route = createFileRoute(`/t/$teamSlug/settings/account`)({
  loader: async () => {
    const timezone = await trpc.users.timezone.query()
    return { timezone: timezone.timezone }
  },
  component: SettingsAccount,
})

// Personal section (EXP-238): identity, timezone, and account deletion.
// Always visible — the layout's beforeLoad already guarantees a session.
function SettingsAccount() {
  const { timezone } = Route.useLoaderData()

  return (
    <div className="space-y-6">
      <AccountOverview initialTimezone={timezone} />
      <DeleteAccountSection />
    </div>
  )
}
