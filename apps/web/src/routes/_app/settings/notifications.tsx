import { createFileRoute } from "@tanstack/react-router"
import {
  loadNotificationSettings,
  NotificationSettingsBody,
} from "@/components/settings/personal-settings"
import { pageTitle } from "@/lib/page-title"

// #EXP-2027: the team-less Notifications page of web md+ (per user).
export const Route = createFileRoute(`/_app/settings/notifications`)({
  head: () => ({
    meta: [{ title: pageTitle(`Notifications`, `Settings`) }],
  }),
  loader: loadNotificationSettings,
  component: NotificationsRoute,
})

function NotificationsRoute() {
  return <NotificationSettingsBody data={Route.useLoaderData()} />
}
