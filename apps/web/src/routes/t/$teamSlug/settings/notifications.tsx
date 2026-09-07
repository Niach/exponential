import { createFileRoute } from "@tanstack/react-router"
import { trpc } from "@/lib/trpc-client"
import { EmailNotificationsCard } from "@/components/account/email-notifications-card"

export const Route = createFileRoute(`/t/$teamSlug/settings/notifications`)({
  loader: async () => {
    const emailPrefs = await trpc.notifications.emailPrefs.query()
    return { emailPrefs }
  },
  component: SettingsNotifications,
})

// Personal section (EXP-238): the email digest preferences, formerly the
// /account/notifications page.
function SettingsNotifications() {
  const { emailPrefs } = Route.useLoaderData()

  return <EmailNotificationsCard emailPrefs={emailPrefs} />
}
