import { createFileRoute } from "@tanstack/react-router"
import {
  loadNotificationSettings,
  NotificationSettingsBody,
} from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027: phones keep the team-scoped page; md+ forwards this URL to
// `/settings/notifications` (the same body, components/settings).
export const Route = createFileRoute(`/t/$teamSlug/settings/notifications`)({
  head: () => ({
    meta: [{ title: pageTitle(`Notifications`, `Settings`) }],
  }),
  loader: loadNotificationSettings,
  component: SettingsNotifications,
})

function SettingsNotifications() {
  return <NotificationSettingsBody data={Route.useLoaderData()} />
}
